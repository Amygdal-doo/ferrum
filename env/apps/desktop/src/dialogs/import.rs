//! Importing a bundle: paste the armored text (or open a `.fenv` file),
//! decrypt it with the project's stored key or one typed in, preview the
//! added / changed / removed keys per environment, then import.

use ferrum_ui::components::Checkbox;
use ferrum_ui::gpui::PathPromptOptions;
use ferrum_ui::prelude::*;
use ferrumenv_core::ShareKey;
use ferrumenv_core::bundle::{self, BundleContents};
use ferrumenv_core::diff::{self, ChangeKind, EnvironmentPlan};
use ferrumenv_core::model::Project;
use zeroize::Zeroizing;

use super::{Dialog, input};
use crate::app::EnvApp;
use crate::widgets::{field, muted, primary_button, text_button};

pub(crate) struct ImportBundleDialog {
    pub(crate) text: Entity<TextInput>,
    pub(crate) key: Entity<TextInput>,
    pub(crate) preview: Option<Preview>,
    pub(crate) remove_missing: bool,
}

/// A decrypted bundle waiting for confirmation. Dropped (and wiped) with
/// the dialog.
pub(crate) struct Preview {
    pub(crate) contents: BundleContents,
    pub(crate) plans: Vec<EnvironmentPlan>,
    pub(crate) key: ShareKey,
    /// The vault project of the same name, if any.
    pub(crate) target: Option<usize>,
    /// The key used differs from the one stored for the project.
    pub(crate) replaces_key: bool,
}

impl EnvApp {
    pub(crate) fn open_import_bundle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_unlocked() {
            return;
        }
        let mut subscriptions = Vec::new();
        let text = input(window, cx, &mut subscriptions, |t| {
            t.monospace()
                .multiline(6)
                .placeholder("-----BEGIN FERRUMENV BUNDLE-----  (⌘↵ previews)")
        });
        let key = input(window, cx, &mut subscriptions, |t| {
            t.monospace()
                .masked(true)
                .placeholder("Only needed if this project's key isn't stored yet")
        });
        let dialog = ImportBundleDialog {
            text: text.clone(),
            key,
            preview: None,
            remove_missing: false,
        };
        self.open_dialog(
            Dialog::ImportBundle(dialog),
            subscriptions,
            Some(text),
            window,
            cx,
        );
    }

    fn open_bundle_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open bundle".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let text = std::fs::read_to_string(&path);
            this.update(cx, |this, cx| match text {
                Ok(text) => {
                    if let Some(Dialog::ImportBundle(d)) = this.dialog_mut() {
                        d.text.update(cx, |input, cx| input.set_text(text, cx));
                    }
                    this.preview_bundle(cx);
                }
                Err(e) => {
                    this.set_dialog_error(format!("Could not read {}: {e}", path.display()), cx)
                }
            })
            .ok();
        })
        .detach();
    }

    /// Parses and decrypts the pasted bundle and computes the preview.
    pub(crate) fn preview_bundle(&mut self, cx: &mut Context<Self>) {
        let preview = (|| -> Result<Preview, String> {
            let u = self.vault().ok_or("Locked")?;
            let Some(Dialog::ImportBundle(d)) = &u.dialog else {
                return Err(String::new());
            };
            let sealed = bundle::parse(d.text.read(cx).text()).map_err(|e| e.to_string())?;
            let target = u.vault.project_by_name(&sealed.project);
            let stored = target.and_then(|i| u.vault.projects[i].share_key.clone());
            let typed = Zeroizing::new(d.key.read(cx).text().trim().to_string());
            let key = if typed.is_empty() {
                stored.clone().ok_or_else(|| {
                    format!(
                        "Enter the share key for {} (key {}).",
                        sealed.project, sealed.key_fingerprint
                    )
                })?
            } else {
                ShareKey::parse(&typed).map_err(|e| e.to_string())?
            };
            let contents = sealed.open(&key).map_err(|e| e.to_string())?;
            let empty = Project::new(contents.project.clone(), None);
            let current = target.map_or(&empty, |i| &u.vault.projects[i]);
            let plans = diff::plan_import(current, &contents);
            let replaces_key = stored.is_some_and(|s| s != key);
            Ok(Preview {
                contents,
                plans,
                key,
                target,
                replaces_key,
            })
        })();
        match preview {
            Ok(preview) => {
                if let Some(u) = self.vault_mut() {
                    u.dialog_error = None;
                    if let Some(Dialog::ImportBundle(d)) = u.dialog.as_mut() {
                        d.preview = Some(preview);
                    }
                }
                cx.notify();
            }
            Err(message) if message.is_empty() => {}
            Err(message) => self.set_dialog_error(message, cx),
        }
    }

    pub(crate) fn import_bundle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::ImportBundle(d)) = self.dialog_mut() else {
            return;
        };
        let remove_missing = d.remove_missing;
        let Some(preview) = d.preview.take() else {
            return;
        };
        let done = self.change(cx, |u| {
            let index = match preview.target {
                Some(i) => i,
                None => u
                    .vault
                    .add_project(&preview.contents.project, None)
                    .map_err(|e| e.to_string())?,
            };
            let project = &mut u.vault.projects[index];
            let stats = diff::apply_import(project, &preview.contents, remove_missing);
            project.share_key = Some(preview.key.clone());
            Ok((index, stats))
        });
        if let Some((index, stats)) = done {
            self.close_dialog(window, cx);
            self.select_project(index, window, cx);
            self.show_info(
                format!(
                    "Imported: {} added, {} changed, {} removed.",
                    stats.added, stats.changed, stats.removed
                ),
                cx,
            );
        }
    }

    pub(crate) fn render_import_bundle(
        &self,
        d: &ImportBundleDialog,
        cx: &mut Context<Self>,
    ) -> Modal {
        let modal = self
            .modal("Import bundle", cx)
            .width(px(640.))
            .child(field("Bundle", d.text.clone()))
            .child(field("Share key", d.key.clone()));
        let Some(preview) = &d.preview else {
            return modal
                .footer(text_button(
                    "open-fenv",
                    "Open .fenv…",
                    cx,
                    |this, window, cx| {
                        this.open_bundle_file(window, cx);
                    },
                ))
                .footer(primary_button(
                    "preview-bundle",
                    "Preview",
                    cx,
                    |this, _, cx| {
                        this.preview_bundle(cx);
                    },
                ));
        };
        let c = &preview.contents;
        let mut summary = format!(
            "{} · created {}",
            c.project,
            c.created_at.format("%Y-%m-%d %H:%M UTC")
        );
        if preview.target.is_none() {
            summary.push_str(" · new project");
        }
        let mut list = v_flex().gap_2();
        for plan in &preview.plans {
            list = list.child(render_plan(plan, d.remove_missing));
        }
        let remove_missing = d.remove_missing;
        modal
            .child(Label::new(summary))
            .when(!c.note.is_empty(), |m| m.child(muted(format!("Note: {}", c.note))))
            .when(preview.replaces_key, |m| {
                m.child(
                    Label::new("This key differs from the one stored for the project; importing replaces it.")
                        .size(LabelSize::Small)
                        .color(Color::Warning),
                )
            })
            .child(list)
            .child(
                Checkbox::new("remove-missing", ToggleState::from(remove_missing))
                    .label("Also remove keys the bundle doesn't have")
                    .label_size(LabelSize::Small)
                    .on_click(cx.listener(|this, _: &ToggleState, _, cx| {
                        if let Some(Dialog::ImportBundle(d)) = this.dialog_mut() {
                            d.remove_missing = !d.remove_missing;
                        }
                        cx.notify();
                    })),
            )
            .footer(primary_button("import-bundle-ok", "Import", cx, |this, window, cx| {
                this.import_bundle(window, cx);
            }))
    }
}

fn render_plan(plan: &EnvironmentPlan, remove_missing: bool) -> impl IntoElement {
    let title = format!(
        "{}{}: {} added, {} changed, {} {}",
        plan.name,
        if plan.is_new { " (new)" } else { "" },
        plan.count(ChangeKind::Added),
        plan.count(ChangeKind::Changed),
        plan.count(ChangeKind::Removed),
        if remove_missing {
            "removed"
        } else {
            "only here (kept)"
        },
    );
    let mut keys = h_flex().gap_2().flex_wrap();
    for change in plan
        .changes
        .iter()
        .filter(|c| c.kind != ChangeKind::Unchanged)
    {
        let (sign, color) = match change.kind {
            ChangeKind::Added => ("+", Color::Created),
            ChangeKind::Changed => ("~", Color::Modified),
            ChangeKind::Removed if remove_missing => ("−", Color::Deleted),
            _ => ("·", Color::Muted),
        };
        keys = keys.child(
            Label::new(format!("{sign}{}", change.key))
                .size(LabelSize::Small)
                .color(color),
        );
    }
    v_flex()
        .gap_0p5()
        .child(Label::new(title).size(LabelSize::Small))
        .child(keys)
}
