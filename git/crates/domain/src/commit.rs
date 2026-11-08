//! Commits and stashes.

use crate::diff::FileDiff;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A full hex object id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Oid(pub String);

impl Oid {
    #[must_use]
    pub fn new(hex: impl Into<String>) -> Self {
        Self(hex.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The 7-character abbreviation shown in lists.
    #[must_use]
    pub fn short(&self) -> &str {
        self.0.get(..7).unwrap_or(&self.0)
    }
}

impl fmt::Display for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One row of the log.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Commit {
    pub oid: Oid,
    pub parents: Vec<Oid>,
    pub author_name: String,
    pub author_email: String,
    /// Unix seconds.
    pub author_time: i64,
    pub summary: String,
}

impl Commit {
    #[must_use]
    pub fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }
}

/// A commit (or stash) opened in the detail panel.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommitDetails {
    pub commit: Commit,
    /// Message body after the summary line (may be empty).
    pub body: String,
    pub committer_name: String,
    pub committer_email: String,
    pub commit_time: i64,
    /// Changes against the first parent.
    pub files: Vec<FileDiff>,
    /// The patch was larger than the display limit and was cut.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stash {
    /// `n` in `stash@{n}`.
    pub index: usize,
    pub oid: Oid,
    pub message: String,
    /// Unix seconds.
    pub time: i64,
}

impl Stash {
    #[must_use]
    pub fn refname(&self) -> String {
        format!("stash@{{{}}}", self.index)
    }
}
