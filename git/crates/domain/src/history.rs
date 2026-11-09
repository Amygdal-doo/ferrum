//! Per-file history and blame.

use crate::commit::{Commit, Oid};
use crate::status::ChangeKind;
use std::collections::HashMap;

/// One commit that touched a file (`git log --follow`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileCommit {
    pub commit: Commit,
    /// The file's path in this commit (differs from today's after a rename).
    pub path: String,
    /// Path before this commit renamed / copied the file.
    pub old_path: Option<String>,
    /// `Unmodified` for merges, which list no changes.
    pub change: ChangeKind,
}

/// The commit a blamed line comes from (shared by all its lines).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlameCommit {
    pub author_name: String,
    pub author_email: String,
    /// Unix seconds.
    pub author_time: i64,
    pub summary: String,
    /// Path of the file in that commit.
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlameLine {
    pub oid: Oid,
    /// 1-based line number in the blamed revision.
    pub line_no: u32,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Blame {
    pub path: String,
    pub lines: Vec<BlameLine>,
    pub commits: HashMap<Oid, BlameCommit>,
}

impl Blame {
    #[must_use]
    pub fn commit_of(&self, line: &BlameLine) -> Option<&BlameCommit> {
        self.commits.get(&line.oid)
    }

    /// `true` when line `i` starts a run of lines from one commit; only those
    /// show the annotation.
    #[must_use]
    pub fn starts_group(&self, i: usize) -> bool {
        match (
            i.checked_sub(1).and_then(|p| self.lines.get(p)),
            self.lines.get(i),
        ) {
            (Some(prev), Some(line)) => prev.oid != line.oid,
            (None, Some(_)) => true,
            _ => false,
        }
    }
}
