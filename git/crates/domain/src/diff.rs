//! Parsed unified diffs.

use crate::status::ChangeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
    /// `\ No newline at end of file`
    NoNewline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    /// Without the leading `+`/`-`/` ` marker.
    pub content: String,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Hunk {
    /// The full `@@ -a,b +c,d @@ context` line.
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileDiff {
    /// New path (old path for deletions).
    pub path: String,
    /// Previous path of a rename/copy.
    pub old_path: Option<String>,
    pub change: ChangeKind,
    pub binary: bool,
    /// Raw header lines (`diff --git`, `index`, `---`, `+++` …), needed to
    /// rebuild a patch for one hunk.
    pub header: Vec<String>,
    pub hunks: Vec<Hunk>,
    pub additions: u32,
    pub deletions: u32,
}

/// Which side of the repository a diff was taken from; decides the hunk actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffSource {
    /// Working tree vs index: stage / discard hunks.
    Unstaged,
    /// Index vs HEAD: unstage hunks.
    Staged,
    /// An untracked file shown as all-added.
    Untracked,
    /// A historical commit or stash (read-only).
    Commit,
}
