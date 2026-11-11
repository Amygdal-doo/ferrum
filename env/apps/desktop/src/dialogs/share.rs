//! The project share key (show / copy / generate / rotate / set a key
//! received from a teammate) and exporting an encrypted bundle.

use chrono::Utc;
use ferrum_ui::prelude::*;
use ferrum_ui::ui::Checkbox;
use ferrumenv_core::ShareKey;
use ferrumenv_core::bundle::{self, BundleContents};

use super::{ConfirmAction, Dialog, input};
use crate::app::EnvApp;
use crate::secrets::MASK;
use crate::widgets::{field, muted, primary_button, text_button};

pub(crate) struct ShareKeyDialog {
    pub(crate) revealed: bool,
    /// A key pasted from a teammate.
    pub(crate) received: Entity<TextInput>,
}

pub(crate) struct ShareDialog {
    /// One flag per environment of the project.
    pub(crate) selected: Vec<bool>,
    pub(crate) note: Entity<TextInput>,
    /// The armored bundle once created (encrypted: safe to show).
    pub(crate) output: Option<String>,
}

impl EnvApp {
    pub(crate) fn open_share_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault().and_then(|u| u.project()).is_none() {
            return;
        }
        let mut subscriptions = Vec::new();
        let received = input(window, cx, &mut subscriptions, |t| {
            t.monospace().masked(true).placeholder("fek1_…")
        });
        let dialog = ShareKeyDialog {
            revealed: false,
            received,
        };
        self.open_dialog(Dialog::ShareKey(dialog), subscriptions, None, window, cx);
    }

    /// Generates (or rotates) the current project's key and shows it.
    pub(crate) fn generate_share_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let done = self.change(cx, |u| {
            let key = ShareKey::generate().map_err(|e| e.to_string())?;
            u.project_mut().ok_or("No project selected")?.share_key = Some(key);
            Ok(())
        });
        if done.is_some() {
            self.open_share_key(window, cx);
            self.show_info(
                "New share key generated. Send it to your team once, privately.",
                cx,
            );
        }
    }

    pub(crate) fn submit_received_key(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::ShareKey(d)) = self.dialog_mut() else {
            return;
        };
        let text = zeroize::Zeroizing::new(d.received.read(cx).text().to_string());
        let received = d.received.clone();
        if text.trim().is_empty() {
            self.set_dialog_error("Paste the key your teammate sent you.", cx);
            return;
        }
        let key = match ShareKey::parse(&text) {
            Ok(key) => key,
            Err(e) => {
                self.set_dialog_error(e.to_string(), cx);
                return;
            }
        };
        let fingerprint = key.fingerprint();
        let saved = self.change(cx, |u| {
            u.project_mut().ok_or("No project selected")?.share_key = Some(key);
            Ok(())
        });
        if saved.is_some() {
            received.update(cx, |input, cx| input.set_text("", cx));
            if let Some(u) = self.vault_mut() {
                u.dialog_error = None;
            }
            self.show_info(
                format!("Share key {fingerprint} saved for this project."),
                cx,
            );
        }
    }

    pub(crate) fn render_share_key(&self, d: &ShareKeyDialog, cx: &mut Context<Self>) -> Modal {
        let colors = cx.theme().colors().clone();
        let key = self
            .vault()
            .and_then(|u| u.project())
            .and_then(|p| p.share_key.as_ref());
        let mut modal = self.modal("Project share key", cx).width(px(560.)).child(muted(
            "Anyone with this key can open bundles of this project, and create them. Exchange it once, privately (in person, a password manager), never next to a bundle.",
        ));
        modal = match key {
            Some(key) => {
                let shown: SharedString = if d.revealed {
                    key.encode().as_str().to_string().into()
                } else {
                    format!("fek1_{MASK}").into()
                };
                modal
                    .child(
                        h_flex()
                            .gap_2()
                            .p_2()
                            .rounded_md()
                            .border_1()
                            .border_color(colors.border)
                            .bg(colors.editor_background)
                            .child(div().flex_1().min_w_0().font_buffer(cx).text_ui_sm(cx).child(shown))
                            .child(text_button(
                                "reveal-key",
                                if d.revealed { "Hide" } else { "Reveal" },
                                cx,
                                |this, _, cx| {
                                    if let Some(Dialog::ShareKey(d)) = this.dialog_mut() {
                                        d.revealed = !d.revealed;
                                    }
                                    cx.notify();
                                },
                            ))
                            .child(text_button("copy-key", "Copy", cx, |this, _, cx| {
                                let text = this
                                    .vault()
                                    .and_then(|u| u.project())
                                    .and_then(|p| p.share_key.as_ref())
                                    .map(ShareKey::encode);
                                if let Some(text) = text {
                                    this.copy_secret(&text, "Share key", cx);
                                }
                            })),
                    )
                    .child(muted(format!(
                        "Fingerprint {}: compare it with your teammates to check you hold the same key.",
                        key.fingerprint()
                    )))
                    .footer(text_button("rotate-key", "Generate new key…", cx, |this, window, cx| {
                        this.confirm(ConfirmAction::RotateKey, window, cx);
                    }))
            }
            None => modal
                .child(Label::new("This project has no share key yet."))
                .footer(primary_button(
                    "generate-key",
                    "Generate key",
                    cx,
                    |this, window, cx| {
                        this.generate_share_key(window, cx);
                    },
                )),
        };
        modal.child(field(
            "Set the key received from a teammate",
            h_flex()
                .gap_2()
                .child(div().flex_1().child(d.received.clone()))
                .child(text_button("use-key", "Use this key", cx, |this, _, cx| {
                    this.submit_received_key(cx);
                })),
        ))
    }

    pub(crate) fn open_share(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.vault().and_then(|u| u.project()) else {
            return;
        };
        let selected = vec![true; project.environments.len()];
        let mut subscriptions = Vec::new();
        let note = input(window, cx, &mut subscriptions, |t| {
            t.placeholder("e.g. rotated the Stripe keys")
        });
        let dialog = ShareDialog {
            selected,
            note: note.clone(),
            output: None,
        };
        self.open_dialog(Dialog::Share(dialog), subscriptions, Some(note), window, cx);
    }

    pub(crate) fn create_bundle(&mut self, cx: &mut Context<Self>) {
        let sealed = (|| -> Result<String, String> {
            let u = self.vault().ok_or("Locked")?;
            let (Some(project), Some(Dialog::Share(d))) = (u.project(), u.dialog.as_ref()) else {
                return Err("No project selected".into());
            };
            let key = project
                .share_key
                .as_ref()
                .ok_or("Generate or set a share key first.")?;
            let environments: Vec<_> = project
                .environments
                .iter()
                .zip(&d.selected)
                .filter(|(_, on)| **on)
                .map(|(e, _)| e.clone())
                .collect();
            if environments.is_empty() {
                return Err("Choose at least one environment.".into());
            }
            let contents = BundleContents {
                project: project.name.clone(),
                environments,
                created_at: Utc::now(),
                note: d.note.read(cx).text().trim().to_string(),
            };
            bundle::seal(&contents, key).map_err(|e| e.to_string())
        })();
        match sealed {
            Ok(armored) => {
                if let Some(Dialog::Share(d)) = self.dialog_mut() {
                    d.output = Some(armored);
                }
                if let Some(u) = self.vault_mut() {
                    u.dialog_error = None;
                }
                cx.notify();
            }
            Err(message) => self.set_dialog_error(message, cx),
        }
    }

    fn save_bundle_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(u) = self.vault() else {
            return;
        };
        let (Some(project), Some(Dialog::Share(d))) = (u.project(), u.dialog.as_ref()) else {
            return;
        };
        let Some(armored) = d.output.clone() else {
            return;
        };
        let dir = project
            .path
            .clone()
            .or_else(ferrum_ui::project::home_dir)
            .unwrap_or_default();
        let name = format!("{}.{}", project.name, bundle::FILE_EXTENSION);
        let receiver = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let result = std::fs::write(&path, armored);
            this.update(cx, |this, cx| match result {
                Ok(()) => this.show_info(format!("Bundle saved to {}", path.display()), cx),
                Err(e) => this.show_error(format!("Could not save the bundle: {e}"), cx),
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn render_share(&self, d: &ShareDialog, cx: &mut Context<Self>) -> Modal {
        let colors = cx.theme().colors().clone();
        let Some(project) = self.vault().and_then(|u| u.project()) else {
            return self.modal("Share", cx);
        };
        let mut modal = self
            .modal(format!("Share {}", project.name), cx)
            .width(px(600.));
        let Some(key) = project.share_key.as_ref() else {
            return modal
                .child(Label::new(
                    "Bundles are encrypted with the project's share key, and this project has none yet.",
                ))
                .footer(primary_button("share-generate", "Generate key", cx, |this, window, cx| {
                    this.generate_share_key(window, cx);
                }));
        };
        let mut envs = h_flex().gap_3().flex_wrap();
        for (i, env) in project.environments.iter().enumerate() {
            let on = d.selected.get(i).copied().unwrap_or(false);
            envs = envs.child(
                Checkbox::new(
                    SharedString::from(format!("share-env-{i}")),
                    ToggleState::from(on),
                )
                .label(format!("{} ({})", env.name, env.variables.len()))
                .label_size(LabelSize::Small)
                .on_click(cx.listener(move |this, _: &ToggleState, _, cx| {
                    if let Some(Dialog::Share(d)) = this.dialog_mut()
                        && let Some(flag) = d.selected.get_mut(i)
                    {
                        *flag = !*flag;
                        d.output = None;
                    }
                    cx.notify();
                })),
            );
        }
        modal = modal
            .child(muted(format!(
                "Encrypted with the share key {}. Teammates who hold it can import the bundle; the key is never included.",
                key.fingerprint()
            )))
            .child(field("Environments", envs))
            .child(field("Note for the receiver", d.note.clone()));
        match &d.output {
            None => modal.footer(primary_button(
                "create-bundle",
                "Create bundle",
                cx,
                |this, _, cx| {
                    this.create_bundle(cx);
                },
            )),
            Some(armored) => modal
                .child(
                    div()
                        .id("bundle-output")
                        .max_h(px(180.))
                        .overflow_y_scroll()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_color(colors.border)
                        .bg(colors.editor_background)
                        .font_buffer(cx)
                        .text_ui_xs(cx)
                        .child(armored.clone()),
                )
                .footer(text_button(
                    "save-bundle",
                    "Save .fenv…",
                    cx,
                    |this, window, cx| {
                        this.save_bundle_file(window, cx);
                    },
                ))
                .footer(primary_button(
                    "copy-bundle",
                    "Copy bundle",
                    cx,
                    |this, _, cx| {
                        let text = match this.dialog_mut() {
                            Some(Dialog::Share(d)) => d.output.clone(),
                            _ => None,
                        };
                        if let Some(text) = text {
                            this.copy_plain(text, "Bundle", cx);
                        }
                    },
                )),
        }
    }
}
