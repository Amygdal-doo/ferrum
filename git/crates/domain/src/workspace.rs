//! Open repository tabs and app settings, restored after restart.

use crate::ids::RepoId;
use crate::ops::PullMode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceState {
    pub open: Vec<RepoId>,
    pub active: Option<RepoId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub theme: Theme,
    /// Sidebar width in px; 0 = default.
    pub sidebar_width: f64,
    /// Detail panel width in px; 0 = default.
    pub detail_width: f64,
    /// Commits loaded into the graph (0 = default of 2000).
    pub log_limit: u32,
    pub pull_mode: PullMode,
    /// Fetch with `--prune`.
    pub prune_on_fetch: bool,
    /// Hide `·` / `¬` whitespace markers in diffs.
    pub hide_invisibles: bool,
}
