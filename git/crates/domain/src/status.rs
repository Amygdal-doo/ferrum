//! Working tree / index status.

use crate::commit::Oid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChangeKind {
    #[default]
    Unmodified,
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Conflicted,
}

impl ChangeKind {
    /// One-letter code shown next to files.
    #[must_use]
    pub const fn letter(self) -> &'static str {
        match self {
            Self::Unmodified => " ",
            Self::Modified => "M",
            Self::Added => "A",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Copied => "C",
            Self::TypeChanged => "T",
            Self::Untracked => "U",
            Self::Conflicted => "!",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileStatus {
    pub path: String,
    /// Source path of a rename/copy.
    pub orig_path: Option<String>,
    /// Change staged in the index (vs HEAD).
    pub index: ChangeKind,
    /// Change in the working tree (vs the index).
    pub worktree: ChangeKind,
}

impl FileStatus {
    #[must_use]
    pub fn is_conflicted(&self) -> bool {
        self.index == ChangeKind::Conflicted
    }

    #[must_use]
    pub fn is_staged(&self) -> bool {
        !self.is_conflicted()
            && !matches!(self.index, ChangeKind::Unmodified | ChangeKind::Untracked)
    }

    #[must_use]
    pub fn is_unstaged(&self) -> bool {
        self.is_conflicted() || self.worktree != ChangeKind::Unmodified
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BranchInfo {
    /// Checked-out branch; `None` when HEAD is detached.
    pub head: Option<String>,
    /// HEAD commit; `None` in a repository without commits.
    pub oid: Option<Oid>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

/// A multi-step operation in progress (needs continue / abort).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoOperation {
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

impl RepoOperation {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Merge => "Merge",
            Self::Rebase => "Rebase",
            Self::CherryPick => "Cherry-pick",
            Self::Revert => "Revert",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkingStatus {
    pub branch: BranchInfo,
    pub files: Vec<FileStatus>,
    pub operation: Option<RepoOperation>,
}

impl WorkingStatus {
    #[must_use]
    pub fn staged(&self) -> Vec<&FileStatus> {
        self.files.iter().filter(|f| f.is_staged()).collect()
    }

    #[must_use]
    pub fn unstaged(&self) -> Vec<&FileStatus> {
        self.files.iter().filter(|f| f.is_unstaged()).collect()
    }

    #[must_use]
    pub fn conflicted(&self) -> Vec<&FileStatus> {
        self.files.iter().filter(|f| f.is_conflicted()).collect()
    }

    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.files.is_empty()
    }
}
