//! Syncing a project through a shared folder: choose / stop / sync now,
//! and the conflict dialog that shows both versions of each key changed
//! on both sides (masked until revealed) for the user to choose.

use std::path::PathBuf;

use ferrum_ui::gpui::PathPromptOptions;
use ferrum_ui::prelude::*;
use ferrumenv_core::model::{Environment, Variable};
use ferrumenv_core::sync::{self, Merged, SyncState};

use super::Dialog;
use crate::app::EnvApp;
use crate::secrets::{MASK, preview};
use crate::widgets::{field, muted, primary_button, text_button};

pub(crate) struct SyncDialog;

pub(crate) struct SyncConflictsDialog {
    pub(crate) project_id: String,
    /// Who wrote the other version.
    pub(crate) sender: String,
    pub(crate) merged: Merged,
    /// Per conflict: keep theirs (else yours).
    pub(crate) take_theirs: Vec<bool>,
    /// Files merged by this sync, recorded once resolved.
    pub(crate) seen: Vec<(String, String)>,
    /// What the folder's main file holds.
    pub(crate) folder_state: Vec<Environment>,
    /// The conflicting file's hash (put aside on Cancel).
    pub(crate) hash: String,
    pub(crate) revealed: bool,
}

impl EnvApp {
    pub(crate) fn open_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault().and_then(|u| u.project()).is_none() {
            return;
        }
        self.open_dialog(Dialog::Sync(SyncDialog), Vec::new(), None, window, cx);
    }

    fn choose_sync_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Sync through this folder".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                this.set_sync_folder(path, window, cx)
            })
            .ok();
        })
        .detach();
    }

    /// Links the current project to `folder` and syncs at once (a bundle
    /// already there is merged first).
    pub(crate) fn set_sync_folder(
        &mut self,
        folder: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !folder.is_dir() {
            self.set_dialog_error(format!("{} is not a folder.", folder.display()), cx);
            return;
        }
        if self.my_public_key(cx).is_none() {
            return;
        }
        let index = self.change(cx, |u| {
            let index = u.project.ok_or("No project selected")?;
            u.vault.projects[index].sync = Some(SyncState::new(folder));
            Ok(index)
        });
        if let Some(index) = index {
            self.close_dialog(window, cx);
            self.sync_project(index, window, cx);
        }
    }

    fn stop_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let done = self.change(cx, |u| {
            u.project_mut().ok_or("No project selected")?.sync = None;
            Ok(())
        });
        if done.is_some() {
            self.close_dialog(window, cx);
            self.show_info(
                "Sync stopped. The bundle stays in the folder (encrypted).",
                cx,
            );
        }
    }

    /// Syncs the current project now, bringing back conflicts put aside.
    pub(crate) fn sync_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.vault().and_then(|u| u.project) else {
            return;
        };
        if let Some(u) = self.vault_mut() {
            u.sync.snoozed.clear();
        }
        self.close_dialog(window, cx);
        self.sync_project(index, window, cx);
    }

    pub(crate) fn render_sync(&self, cx: &mut Context<Self>) -> Modal {
        let modal = self.modal("Sync through a folder", cx).width(px(560.));
        let Some(u) = self.vault() else {
            return modal;
        };
        let Some(project) = u.project() else {
            return modal;
        };
        let modal = modal.child(muted(format!(
            "Point {} at a folder your team shares (Dropbox, iCloud Drive, a git repository…). Only an encrypted team bundle, {}, is written there, readable by you and the project's team members; their changes are picked up within seconds and merged. Keys changed on both sides are shown to you to choose.",
            project.name,
            sync::file_name(&project.name)
        )));
        let Some(state) = &project.sync else {
            return modal
                .when(project.members.is_empty(), |m| {
                    m.child(
                        Label::new("Add your teammates under Team keys first, or only you can read the folder.")
                            .size(LabelSize::Small)
                            .color(Color::Warning),
                    )
                })
                .footer(primary_button("choose-sync-folder", "Choose folder…", cx, |this, window, cx| {
                    this.choose_sync_folder(window, cx);
                }));
        };
        let status = u
            .sync
            .status
            .get(&project.id)
            .cloned()
            .unwrap_or_else(|| "Not synced yet".into());
        modal
            .child(field(
                "Folder",
                Label::new(state.folder.display().to_string()),
            ))
            .child(muted(status))
            .footer(text_button(
                "stop-sync",
                "Stop syncing",
                cx,
                |this, window, cx| {
                    this.stop_sync(window, cx);
                },
            ))
            .footer(text_button(
                "change-sync-folder",
                "Change folder…",
                cx,
                |this, window, cx| {
                    this.choose_sync_folder(window, cx);
                },
            ))
            .footer(primary_button(
                "sync-now",
                "Sync now",
                cx,
                |this, window, cx| {
                    this.sync_now(window, cx);
                },
            ))
    }

    /// Applies the choices, stores the merge and writes the bundle.
    pub(crate) fn resolve_conflicts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(u) = self.vault_mut() else {
            return;
        };
        let Some(Dialog::SyncConflicts(d)) = u.dialog.take() else {
            return;
        };
        let Some(index) = u.vault.projects.iter().position(|p| p.id == d.project_id) else {
            self.close_dialog(window, cx);
            return;
        };
        let mut merged = d.merged;
        sync::resolve(&mut merged, &d.take_theirs);
        let result = crate::sync::commit(u, index, merged.environments, d.seen, &d.folder_state);
        self.close_dialog(window, cx);
        match result {
            Ok(()) => self.show_info("Conflicts resolved and synced.", cx),
            Err(e) => self.show_error(e, cx),
        }
    }

    /// Cancel: nothing is merged or written until the user syncs again.
    pub(crate) fn snooze_conflicts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(u) = self.vault_mut()
            && let Some(Dialog::SyncConflicts(d)) = &u.dialog
        {
            let hash = d.hash.clone();
            u.sync.snoozed.insert(hash);
        }
        self.close_dialog(window, cx);
        self.show_info(
            "Sync paused for this project until the conflicts are resolved (Sync… › Sync now).",
            cx,
        );
    }

    pub(crate) fn render_sync_conflicts(
        &self,
        d: &SyncConflictsDialog,
        cx: &mut Context<Self>,
    ) -> Modal {
        let mut rows = v_flex().gap_2();
        for (i, conflict) in d.merged.conflicts.iter().enumerate() {
            let theirs = d.take_theirs.get(i).copied().unwrap_or(false);
            rows = rows.child(
                v_flex()
                    .gap_1()
                    .child(Label::new(format!(
                        "{} · {}",
                        conflict.environment, conflict.key
                    )))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(choice_button(
                                i,
                                false,
                                !theirs,
                                &conflict.mine,
                                d.revealed,
                                cx,
                            ))
                            .child(choice_button(
                                i,
                                true,
                                theirs,
                                &conflict.theirs,
                                d.revealed,
                                cx,
                            )),
                    ),
            );
        }
        self.modal(format!("Sync conflicts with {}", d.sender), cx)
            .width(px(640.))
            .on_dismiss(cx.listener(|this, _, window, cx| this.snooze_conflicts(window, cx)))
            .child(muted(
                "These keys changed both here and in the shared folder since the last sync. Choose which version to keep for each; nothing is written until you do.",
            ))
            .child(rows)
            .footer(text_button(
                "reveal-conflicts",
                if d.revealed { "Hide values" } else { "Reveal values" },
                cx,
                |this, _, cx| {
                    if let Some(Dialog::SyncConflicts(d)) = this.dialog_mut() {
                        d.revealed = !d.revealed;
                    }
                    cx.notify();
                },
            ))
            .footer(text_button("snooze-conflicts", "Later", cx, |this, window, cx| {
                this.snooze_conflicts(window, cx);
            }))
            .footer(primary_button("resolve-conflicts", "Keep chosen versions", cx, |this, window, cx| {
                this.resolve_conflicts(window, cx);
            }))
    }
}

/// "Yours: ●●●●" / "Theirs: sk_li…" as a toggle.
fn choice_button(
    index: usize,
    theirs: bool,
    selected: bool,
    value: &Option<Variable>,
    revealed: bool,
    cx: &mut Context<EnvApp>,
) -> Button {
    let side = if theirs { "Theirs" } else { "Yours" };
    let shown = match value {
        None => "(deleted)".to_string(),
        Some(var) if revealed || !var.secret => preview(&var.value),
        Some(_) => MASK.to_string(),
    };
    Button::new(
        SharedString::from(format!("conflict-{index}-{side}")),
        format!("{side}: {shown}"),
    )
    .size(ButtonSize::Compact)
    .label_size(LabelSize::Small)
    .toggle_state(selected)
    .on_click(cx.listener(move |this, _, _, cx| {
        if let Some(Dialog::SyncConflicts(d)) = this.dialog_mut()
            && let Some(slot) = d.take_theirs.get_mut(index)
        {
            *slot = theirs;
        }
        cx.notify();
    }))
}
