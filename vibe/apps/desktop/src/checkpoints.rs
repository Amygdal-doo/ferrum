//! The agent's checkpoints in the git column: every turn that wrote files
//! starts one (FerrumChat's change set), shown newest first above the
//! history's commits as "Before: <the turn's prompt>". "Restore to here…"
//! asks first, then puts the files back as they were before that turn and
//! drops the later checkpoints; the chat's review offers "Undo restore"
//! until the next turn starts.

use std::path::PathBuf;

use chrono::Local;
use ferrum_ui::components::TintColor;
use ferrum_ui::prelude::*;
use ferrumagent_ui::core::changeset::CheckpointInfo;
use ferrumgit_ui::HistoryMarker;

use crate::app::FerrumEngineer;

/// The section title above the markers.
pub const TITLE: &str = "AGENT CHECKPOINTS · NOT COMMITTED";

/// A restore waiting for confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePrompt {
    pub id: u64,
    pub label: String,
    /// The files written since, which go back.
    pub files: Vec<PathBuf>,
}

/// Markers for the checkpoints that wrote files, newest first.
#[must_use]
pub fn markers(checkpoints: &[CheckpointInfo]) -> Vec<HistoryMarker> {
    checkpoints
        .iter()
        .rev()
        .filter(|c| !c.files.is_empty())
        .map(|c| HistoryMarker {
            id: c.id,
            title: format!("Before: {}", c.label),
            detail: format!(
                "{} file{} · {}",
                c.files.len(),
                if c.files.len() == 1 { "" } else { "s" },
                c.created.with_timezone(&Local).format("%H:%M")
            ),
        })
        .collect()
}

/// Files a restore to `id` writes: those of `id` and every later one.
#[must_use]
pub fn files_restored(checkpoints: &[CheckpointInfo], id: u64) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = checkpoints
        .iter()
        .skip_while(|c| c.id != id)
        .flat_map(|c| c.files.iter().cloned())
        .collect();
    files.sort();
    files.dedup();
    files
}

impl FerrumEngineer {
    /// Shows the current project's chat checkpoints in the git column.
    pub(crate) fn sync_checkpoints(&mut self, cx: &mut Context<Self>) {
        let Some(git) = self.git.clone() else {
            return;
        };
        let checkpoints = self
            .chat()
            .map(|chat| chat.read(cx).checkpoints())
            .unwrap_or_default();
        git.update(cx, |git, cx| {
            git.set_history_markers(TITLE, markers(&checkpoints), cx);
        });
    }

    /// "Restore to here…" on checkpoint `id`: asks first.
    pub(crate) fn ask_restore(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(chat) = self.chat() else {
            return;
        };
        let checkpoints = chat.read(cx).checkpoints();
        let Some(checkpoint) = checkpoints.iter().find(|c| c.id == id) else {
            // Restored or replaced meanwhile.
            self.sync_checkpoints(cx);
            return;
        };
        self.restore_prompt = Some(RestorePrompt {
            id,
            label: checkpoint.label.clone(),
            files: files_restored(&checkpoints, id),
        });
        cx.notify();
    }

    /// The answer: restore (`true`) or not.
    pub(crate) fn answer_restore(&mut self, restore: bool, cx: &mut Context<Self>) {
        let Some(prompt) = self.restore_prompt.take() else {
            return;
        };
        if restore && let Some(chat) = self.chat() {
            chat.update(cx, |chat, cx| chat.restore_to_checkpoint(prompt.id, cx));
        }
        cx.notify();
    }

    pub(crate) fn render_restore_prompt(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let prompt = self.restore_prompt.as_ref()?;
        const LISTED: usize = 8;
        let mut files = prompt
            .files
            .iter()
            .take(LISTED)
            .map(|f| format!("• {}", f.display()))
            .collect::<Vec<_>>()
            .join("\n");
        if prompt.files.len() > LISTED {
            files.push_str(&format!("\n… and {} more", prompt.files.len() - LISTED));
        }
        let modal = Modal::new(format!("Restore to before “{}”?", prompt.label))
            .on_dismiss(cx.listener(|this, _, _, cx| this.answer_restore(false, cx)))
            .child(Label::new(
                "These files go back to how they were before that turn; later checkpoints are dropped:",
            ))
            .child(
                Label::new(files)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                Label::new(
                    "The conversation is kept. Until the next turn starts, the chat's review offers Undo restore. Edits you made to these files since then are replaced too.",
                )
                .size(LabelSize::Small)
                .color(Color::Muted),
            )
            .footer(
                Button::new("restore-cancel", "Cancel")
                    .on_click(cx.listener(|this, _, _, cx| this.answer_restore(false, cx))),
            )
            .footer(
                Button::new("restore-confirm", "Restore")
                    .style(ButtonStyle::Tinted(TintColor::Warning))
                    .on_click(cx.listener(|this, _, _, cx| this.answer_restore(true, cx))),
            );
        Some(modal.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn checkpoint(id: u64, label: &str, files: &[&str]) -> CheckpointInfo {
        CheckpointInfo {
            id,
            label: label.into(),
            created: Utc::now(),
            files: files.iter().map(PathBuf::from).collect(),
        }
    }

    #[test]
    fn turns_that_wrote_files_become_markers_newest_first() {
        let all = [
            checkpoint(1, "Add a test", &["a.rs"]),
            checkpoint(2, "Explain this", &[]),
            checkpoint(3, "Fix it", &["b.rs", "a.rs"]),
        ];
        let shown = markers(&all);
        let titles: Vec<&str> = shown.iter().map(|m| m.title.as_str()).collect();
        assert_eq!(titles, ["Before: Fix it", "Before: Add a test"]);
        assert_eq!(shown[0].id, 3);
        assert!(shown[0].detail.starts_with("2 files · "));
        assert!(shown[1].detail.starts_with("1 file · "));

        assert_eq!(
            files_restored(&all, 1),
            [PathBuf::from("a.rs"), PathBuf::from("b.rs")]
        );
        assert_eq!(files_restored(&all, 3).len(), 2);
        assert!(files_restored(&all, 9).is_empty());
    }
}
