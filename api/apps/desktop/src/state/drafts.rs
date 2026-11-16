//! The "Draft tests with agent" review: one row per request, each drafted
//! in turn, reviewed (and editable) before anything is saved.

use ferrumapi_application::agent_tests::{DraftInput, DraftModel};
use ferrumapi_domain::{RequestOrigin, TabId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftStatus {
    Waiting,
    Drafting,
    /// The script (editable) and the lines that do not parse.
    Ready {
        script: String,
        problems: Vec<(usize, String)>,
    },
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftRow {
    pub name: String,
    /// Where an accepted script goes: an open tab, or a saved request.
    pub tab: Option<TabId>,
    pub origin: Option<RequestOrigin>,
    pub input: DraftInput,
    pub status: DraftStatus,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftState {
    pub rows: Vec<DraftRow>,
    pub models: Vec<DraftModel>,
    pub model: Option<DraftModel>,
    pub running: bool,
    /// Bumped per drafting pass; results of an older pass are dropped.
    pub run: u64,
}

impl DraftState {
    #[must_use]
    pub fn new(rows: Vec<DraftRow>, models: Vec<DraftModel>, model: Option<DraftModel>) -> Self {
        Self {
            rows,
            models,
            model,
            running: false,
            run: 0,
        }
    }

    /// Rows whose script is ready and ticked.
    #[must_use]
    pub fn accepted(&self) -> Vec<&DraftRow> {
        self.rows
            .iter()
            .filter(|r| r.selected && matches!(r.status, DraftStatus::Ready { .. }))
            .collect()
    }
}

/// `existing` tests with `drafted` added: a blank script is replaced, else
/// the draft goes below the user's own lines.
#[must_use]
pub fn merge_tests(existing: &str, drafted: &str) -> String {
    if existing.trim().is_empty() {
        drafted.trim().to_owned()
    } else {
        format!(
            "{}\n\n# Drafted by the agent\n{}",
            existing.trim_end(),
            drafted.trim()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drafts_replace_blank_tests_and_append_to_others() {
        assert_eq!(
            merge_tests("  \n", "assert status == 200\n"),
            "assert status == 200"
        );
        assert_eq!(
            merge_tests("assert time < 500\n", "assert status == 200"),
            "assert time < 500\n\n# Drafted by the agent\nassert status == 200"
        );
    }
}
