//! Compose tab state: the compose file in the editor, and the output of the
//! last `docker compose` command run for each project.

use std::collections::VecDeque;
use std::path::PathBuf;

/// Output lines kept per run.
pub const RUN_LINES: usize = 5_000;

/// The compose file open in a project panel's editor.
#[derive(Debug, Default)]
pub struct ComposeEditor {
    pub file: Option<PathBuf>,
    /// The text on disk, to tell edits apart.
    pub saved: Option<String>,
    /// The file could not be found or read.
    pub error: Option<String>,
    /// The editor's text differs from `saved`.
    pub dirty: bool,
}

/// One `docker compose …` run and what it printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeRun {
    /// e.g. `docker compose -p shop pull`.
    pub command: String,
    pub lines: VecDeque<String>,
    pub running: bool,
    /// `Some(Ok(summary))` or `Some(Err(error))` once finished.
    pub outcome: Option<Result<String, String>>,
}

impl ComposeRun {
    pub fn new(command: String) -> Self {
        Self {
            command,
            lines: VecDeque::new(),
            running: true,
            outcome: None,
        }
    }

    /// Appends a line, dropping the oldest past [`RUN_LINES`].
    pub fn push(&mut self, line: String) {
        if self.lines.len() == RUN_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    pub fn finish(&mut self, outcome: Result<String, String>) {
        self.running = false;
        self.outcome = Some(outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_newest_lines() {
        let mut run = ComposeRun::new("docker compose -p shop pull".into());
        for i in 0..=RUN_LINES {
            run.push(format!("line {i}"));
        }
        assert_eq!(run.lines.len(), RUN_LINES);
        assert_eq!(run.lines.front().map(String::as_str), Some("line 1"));
        assert!(run.running && run.outcome.is_none());
        run.finish(Err("exit 1".into()));
        assert!(!run.running);
        assert_eq!(run.outcome, Some(Err("exit 1".into())));
    }
}
