//! Keeping synced projects in step with their shared folder. A timer polls
//! every few seconds while the vault is unlocked and no dialog is open:
//! changed bundle files are merged (three-way, see `ferrumenv_core::sync`)
//! and the project's own bundle is rewritten whenever the vault differs
//! from what the folder holds, so local edits go out within a poll.
//! Conflicts open a dialog showing both versions; until they are resolved
//! nothing is written to the folder.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use ferrum_ui::prelude::*;
use ferrumenv_core::model::Environment;
use ferrumenv_core::sync;

use crate::app::EnvApp;
use crate::dialogs::{Dialog, SyncConflictsDialog};
use crate::unlocked::Unlocked;

/// How often synced folders are checked.
pub(crate) const SYNC_POLL: Duration = Duration::from_secs(2);

/// Per-unlock sync bookkeeping (dropped on lock).
#[derive(Default)]
pub(crate) struct SyncMemo {
    /// Hashes of bundle files whose conflicts the user put aside: the
    /// project is not written until they are resolved.
    pub snoozed: HashSet<String>,
    /// Hashes of rejected files already reported.
    pub reported: HashSet<String>,
    /// The last status per project id ("Synced 14:02", or an error).
    pub status: HashMap<String, String>,
}

/// What one sync of a project did.
pub(crate) enum Step {
    Idle,
    Synced { taken: usize, from: Vec<String> },
    Blocked,
    Conflicts(SyncConflictsDialog),
}

/// Merges the folder's new files into project `index` and writes its
/// bundle when needed. Rejected files are returned as notes.
pub(crate) fn run(u: &mut Unlocked, index: usize) -> Result<(Step, Vec<String>), String> {
    let project = u.vault.projects.get(index).ok_or("No such project")?;
    let state = project.sync.clone().ok_or("This project is not synced")?;
    let identity = u
        .vault
        .identity
        .clone()
        .ok_or("You have no key pair yet (Team keys)")?;
    let (incoming, rejected) = sync::read_incoming(&state, project, &identity)
        .map_err(|e| format!("Could not read {}: {e}", state.folder.display()))?;
    let mut notes = Vec::new();
    for r in rejected {
        if u.sync.reported.insert(r.hash) {
            notes.push(format!(
                "Ignored {} in the sync folder: {}",
                r.file, r.reason
            ));
        }
    }
    if incoming.iter().any(|i| u.sync.snoozed.contains(&i.hash)) {
        return Ok((Step::Blocked, notes));
    }
    let main = sync::file_name(&project.name);
    let me = identity.public_key().map_err(|e| e.to_string())?;
    let mut local = project.environments.clone();
    let mut folder_state = state.base.clone();
    let (mut seen, mut taken, mut from) = (Vec::new(), 0, Vec::new());
    for inc in incoming {
        // A sync service's conflict copy has an unknown base: merging it
        // against nothing turns every difference into a choice.
        let base: &[Environment] = if inc.file == main { &state.base } else { &[] };
        let merged = sync::merge(base, &local, &inc.contents.environments);
        if inc.file == main {
            folder_state = inc.contents.environments.clone();
        }
        let sender = if inc.sender == me {
            "you (another device)".to_string()
        } else {
            project
                .member(&inc.sender)
                .map_or_else(|| inc.sender.fingerprint(), |m| m.name.clone())
        };
        seen.push((inc.file.clone(), inc.hash.clone()));
        if !merged.conflicts.is_empty() {
            let choices = vec![false; merged.conflicts.len()];
            return Ok((
                Step::Conflicts(SyncConflictsDialog {
                    project_id: project.id.clone(),
                    sender,
                    merged,
                    take_theirs: choices,
                    seen,
                    folder_state,
                    hash: inc.hash,
                    revealed: false,
                }),
                notes,
            ));
        }
        taken += merged.taken;
        if merged.taken > 0 && !from.contains(&sender) {
            from.push(sender);
        }
        local = merged.environments;
    }
    let changed = !seen.is_empty();
    commit(u, index, local, seen, &folder_state)?;
    let step = if changed || taken > 0 {
        Step::Synced { taken, from }
    } else {
        Step::Idle
    };
    Ok((step, notes))
}

/// Stores the merged environments and the files merged, then writes the
/// project's bundle if the folder doesn't hold exactly that yet.
pub(crate) fn commit(
    u: &mut Unlocked,
    index: usize,
    environments: Vec<Environment>,
    seen: Vec<(String, String)>,
    folder_state: &[Environment],
) -> Result<(), String> {
    let identity = u.vault.identity.clone().ok_or("No key pair")?;
    let project = u.vault.projects.get_mut(index).ok_or("No such project")?;
    let Some(state) = project.sync.as_mut() else {
        return Err("This project is not synced".into());
    };
    let changed = !sync::same_content(&project.environments, &environments) || !seen.is_empty();
    for (file, hash) in seen {
        state.seen.insert(file, hash);
    }
    let needs_write = !sync::same_content(&environments, folder_state)
        || !sync::main_file_exists(state, &project.name);
    if !sync::same_content(&project.environments, &environments) {
        project.environments = environments;
        project.touch();
    }
    let mut result = Ok(());
    if needs_write {
        let state = project.sync.clone().ok_or("not synced")?;
        match sync::write(&state, project, &identity) {
            Ok((file, hash)) => {
                let state = project.sync.as_mut().ok_or("not synced")?;
                state.seen.insert(file, hash);
                state.base = project.environments.clone();
            }
            Err(e) => result = Err(format!("Could not write to the sync folder: {e}")),
        }
    } else if let Some(state) = project.sync.as_mut() {
        state.base = project.environments.clone();
    }
    if changed || needs_write {
        u.session
            .save(&u.vault)
            .map_err(|e| format!("Could not save the vault: {e}"))?;
    }
    result
}

impl EnvApp {
    /// One poll: syncs every synced project, unless a dialog is open.
    pub(crate) fn poll_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(u) = self.vault() else {
            return;
        };
        if u.dialog.is_some() {
            return;
        }
        let synced: Vec<usize> = (0..u.vault.projects.len())
            .filter(|&i| u.vault.projects[i].sync.is_some())
            .collect();
        for index in synced {
            self.sync_project(index, window, cx);
            if self.vault().is_some_and(|u| u.dialog.is_some()) {
                break;
            }
        }
    }

    pub(crate) fn sync_project(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(u) = self.vault_mut() else {
            return;
        };
        let Some(project) = u.vault.projects.get(index) else {
            return;
        };
        let (id, name) = (project.id.clone(), project.name.clone());
        let result = run(u, index);
        let now = chrono::Local::now().format("%H:%M");
        let (status, message) = match result {
            Ok((step, notes)) => {
                for note in notes {
                    self.show_error(note, cx);
                }
                match step {
                    Step::Idle => (format!("In sync · checked {now}"), None),
                    Step::Synced { taken, from } if taken > 0 => (
                        format!("Synced {now}"),
                        Some(format!(
                            "{name}: {taken} change(s) synced from {}.",
                            from.join(", ")
                        )),
                    ),
                    Step::Synced { .. } => (format!("Synced {now}"), None),
                    Step::Blocked => (
                        "Waiting: resolve the conflicts (Sync… › Sync now)".to_string(),
                        None,
                    ),
                    Step::Conflicts(dialog) => {
                        self.open_dialog(
                            Dialog::SyncConflicts(dialog),
                            Vec::new(),
                            None,
                            window,
                            cx,
                        );
                        ("Conflicts to resolve".to_string(), None)
                    }
                }
            }
            Err(e) => (e, None),
        };
        let Some(u) = self.vault_mut() else {
            return;
        };
        let previous = u.sync.status.insert(id, status.clone());
        if status.starts_with("Could not") && previous.as_deref() != Some(status.as_str()) {
            self.show_error(status, cx);
        } else if let Some(message) = message {
            self.show_info(message, cx);
        }
        cx.notify();
    }
}
