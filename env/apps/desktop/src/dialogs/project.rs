//! New project (free-named, or linked to a folder from the recent
//! projects list / a folder dialog) and new/rename environment.

use std::path::PathBuf;

use ferrum_ui::gpui::PathPromptOptions;
use ferrum_ui::prelude::*;
use ferrum_ui::project;

use super::{Dialog, input};
use crate::app::EnvApp;
use crate::widgets::{field, muted, primary_button, text_button};

/// How many recent folders the new-project dialog offers.
const RECENT_SHOWN: usize = 6;

pub(crate) struct NewProjectDialog {
    pub(crate) name: Entity<TextInput>,
    pub(crate) path: Option<PathBuf>,
    /// Recent Ferrum projects (`~/.ferrum/projects.json`) without a vault
    /// project yet.
    pub(crate) recent: Vec<PathBuf>,
}

pub(crate) struct EnvironmentNameDialog {
    /// `None` adds an environment; `Some(i)` renames environment `i`.
    pub(crate) rename: Option<usize>,
    pub(crate) name: Entity<TextInput>,
}

impl EnvApp {
    pub(crate) fn open_new_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(unlocked) = self.vault() else {
            return;
        };
        let recent: Vec<PathBuf> = project::projects_file()
            .map(|file| project::load(&file))
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.path)
            .filter(|p| unlocked.vault.project_by_path(p).is_none())
            .take(RECENT_SHOWN)
            .collect();
        let mut subscriptions = Vec::new();
        let name = input(window, cx, &mut subscriptions, |t| {
            t.placeholder("e.g. shop-backend")
        });
        let dialog = NewProjectDialog {
            name: name.clone(),
            path: None,
            recent,
        };
        self.open_dialog(
            Dialog::NewProject(dialog),
            subscriptions,
            Some(name),
            window,
            cx,
        );
    }

    fn set_new_project_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(Dialog::NewProject(d)) = self.dialog_mut() {
            if d.name.read(cx).text().trim().is_empty() {
                let name = project::project_name(&path);
                d.name.update(cx, |input, cx| input.set_text(name, cx));
            }
            d.path = Some(path);
        }
        cx.notify();
    }

    fn choose_project_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Link".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.into_iter().next()
            {
                this.update(cx, |this, cx| this.set_new_project_path(path, cx))
                    .ok();
            }
        })
        .detach();
    }

    pub(crate) fn submit_new_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::NewProject(d)) = self.dialog_mut() else {
            return;
        };
        let name = d.name.read(cx).text().trim().to_string();
        let path = d.path.clone();
        let added = self.change(cx, |u| {
            u.vault.add_project(&name, path).map_err(|e| e.to_string())
        });
        if let Some(index) = added {
            self.close_dialog(window, cx);
            self.select_project(index, window, cx);
        }
    }

    pub(crate) fn render_new_project(&self, d: &NewProjectDialog, cx: &mut Context<Self>) -> Modal {
        let colors = cx.theme().colors().clone();
        let folder = d.path.as_ref().map_or_else(
            || "None: a free-named project".to_string(),
            |p| p.display().to_string(),
        );
        let mut recent = v_flex().gap_0p5();
        for (i, path) in d.recent.iter().enumerate() {
            let pick = path.clone();
            recent = recent.child(
                ListItem::new(SharedString::from(format!("recent-{i}")))
                    .rounded()
                    .toggle_state(d.path.as_ref() == Some(path))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(Label::new(project::project_name(path)).size(LabelSize::Small))
                            .child(muted(path.display().to_string()).size(LabelSize::XSmall)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_new_project_path(pick.clone(), cx);
                    })),
            );
        }
        self.modal("New project", cx)
            .child(field("Name", d.name.clone()))
            .child(field(
                "Folder",
                h_flex()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(muted(folder)))
                    .child(text_button(
                        "choose-folder",
                        "Choose…",
                        cx,
                        |this, window, cx| {
                            this.choose_project_folder(window, cx);
                        },
                    ))
                    .when(d.path.is_some(), |row| {
                        row.child(text_button("clear-folder", "Clear", cx, |this, _, cx| {
                            if let Some(Dialog::NewProject(d)) = this.dialog_mut() {
                                d.path = None;
                            }
                            cx.notify();
                        }))
                    }),
            ))
            .when(!d.recent.is_empty(), |modal| {
                modal.child(
                    v_flex()
                        .gap_1()
                        .child(muted("Recent Ferrum projects"))
                        .child(
                            div()
                                .rounded_md()
                                .border_1()
                                .border_color(colors.border_variant)
                                .p_1()
                                .child(recent),
                        ),
                )
            })
            .child(muted(
                "New projects start with development, staging and production environments.",
            ))
            .footer(primary_button(
                "create-project",
                "Create project",
                cx,
                |this, window, cx| {
                    this.submit_new_project(window, cx);
                },
            ))
    }

    pub(crate) fn open_environment_name(
        &mut self,
        rename: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = rename.and_then(|i| {
            self.vault()
                .and_then(|u| u.project())
                .and_then(|p| p.environments.get(i))
                .map(|e| e.name.clone())
        });
        if self.vault().and_then(|u| u.project()).is_none() {
            return;
        }
        let mut subscriptions = Vec::new();
        let name = input(window, cx, &mut subscriptions, |t| {
            t.placeholder("e.g. preview")
                .with_text(current.unwrap_or_default())
        });
        let dialog = EnvironmentNameDialog {
            rename,
            name: name.clone(),
        };
        self.open_dialog(
            Dialog::EnvironmentName(dialog),
            subscriptions,
            Some(name),
            window,
            cx,
        );
    }

    pub(crate) fn submit_environment_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::EnvironmentName(d)) = self.dialog_mut() else {
            return;
        };
        let name = d.name.read(cx).text().to_string();
        let rename = d.rename;
        let done = self.change(cx, |u| {
            let project = u.project_mut().ok_or("No project selected")?;
            let index = match rename {
                Some(i) => project.rename_environment(i, &name).map(|()| i),
                None => project.add_environment(&name),
            }
            .map_err(|e| e.to_string())?;
            Ok(index)
        });
        if let Some(index) = done {
            self.close_dialog(window, cx);
            self.select_environment(index, cx);
        }
    }

    pub(crate) fn render_environment_name(
        &self,
        d: &EnvironmentNameDialog,
        cx: &mut Context<Self>,
    ) -> Modal {
        let (title, button) = match d.rename {
            Some(_) => ("Rename environment", "Rename"),
            None => ("New environment", "Add environment"),
        };
        self.modal(title, cx)
            .width(px(380.))
            .child(field("Name", d.name.clone()))
            .footer(primary_button(
                "save-env",
                button,
                cx,
                |this, window, cx| {
                    this.submit_environment_name(window, cx);
                },
            ))
    }
}
