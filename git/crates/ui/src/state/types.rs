//! Small UI-state value types.

use ferrumgit_application::forge::{PrDetail, PrFilter, PrList, ReviewEvent};
use ferrumgit_application::{CancellationToken, ForgeInfo, PrChanges, RepoData};
use ferrumgit_domain::{
    Blame, CommitDetails, ConflictChoice, ConflictFile, DiffSource, FileCommit, FileDiff, Oid,
    ProfileId, RepoId, RepoOp,
};
use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub message: String,
    pub error: bool,
}

/// Workspace page shown for a repository (sidebar "Workspace" section).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    WorkingCopy,
    #[default]
    History,
    Stashes,
    /// GitHub pull requests / GitLab merge requests.
    PullRequests,
}

/// Tab of the pull request detail panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrTab {
    #[default]
    Conversation,
    Commits,
    Files,
    Checks,
}

impl PrTab {
    pub const ALL: [Self; 4] = [Self::Conversation, Self::Commits, Self::Files, Self::Checks];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Conversation => "Conversation",
            Self::Commits => "Commits",
            Self::Files => "Files",
            Self::Checks => "Checks",
        }
    }
}

/// A value being loaded from the forge.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Load<T> {
    #[default]
    Idle,
    Loading,
    Ready(Arc<T>),
    Failed(String),
}

impl<T> Load<T> {
    #[must_use]
    pub fn ready(&self) -> Option<&Arc<T>> {
        match self {
            Self::Ready(v) => Some(v),
            _ => None,
        }
    }
}

/// Pull requests page state for one repository.
#[derive(Debug, Clone, Default)]
pub struct PrPanel {
    /// Forge, token state and onboarding text.
    pub info: Load<ForgeInfo>,
    pub filter: PrFilter,
    pub list: Load<PrList>,
    pub selected: Option<u64>,
    pub detail: Load<PrDetail>,
    pub tab: PrTab,
    /// Changed files, diffed locally after fetching the PR ref.
    pub changes: Load<PrChanges>,
}

/// What the detail panel shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Selection {
    #[default]
    Wip,
    Commit(Oid),
    Stash(usize),
}

/// A diff open in the center pane.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffView {
    pub file: Arc<FileDiff>,
    pub source: DiffSource,
    /// Rename source, so staged renames diff correctly on reload.
    pub orig: Option<String>,
    /// Selected `(hunk, line)` pairs for line staging.
    pub lines: BTreeSet<(usize, usize)>,
}

/// Three-way view of a conflicted file (ours / theirs / result); replaces
/// the diff while open.
#[derive(Debug, Clone, PartialEq)]
pub struct ConflictView {
    pub file: Arc<ConflictFile>,
    /// One per conflict, in order.
    pub choices: Vec<ConflictChoice>,
}

impl ConflictView {
    #[must_use]
    pub fn new(file: ConflictFile) -> Self {
        let choices = vec![ConflictChoice::Unresolved; file.conflicts()];
        Self {
            file: Arc::new(file),
            choices,
        }
    }

    #[must_use]
    pub fn resolved(&self) -> usize {
        self.choices.iter().filter(|c| c.is_resolved()).count()
    }
}

/// Tab of the file panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileTab {
    #[default]
    History,
    Blame,
}

/// History and blame of one file; replaces the right panel while open.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FileView {
    pub path: String,
    /// Where history starts and blame is taken; `None` = HEAD / working tree.
    pub rev: Option<Oid>,
    pub tab: FileTab,
    pub history: Option<Arc<Vec<FileCommit>>>,
    pub blame: Option<Arc<Blame>>,
    pub error: Option<String>,
}

/// Commits whose full message matched a search (from `git log --grep`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchHits {
    pub query: String,
    pub oids: Arc<HashSet<String>>,
}

/// Per-repository UI state.
#[derive(Debug, Clone, Default)]
pub struct RepoView {
    pub data: Option<Arc<RepoData>>,
    pub error: Option<String>,
    pub loading: bool,
    pub selection: Selection,
    pub details: Option<Arc<CommitDetails>>,
    pub diff: Option<DiffView>,
    /// Conflicted file open in the three-way view.
    pub conflict: Option<ConflictView>,
    /// Last change fingerprint seen by the poller.
    pub fingerprint: String,
    pub summary: String,
    pub description: String,
    pub amend: bool,
    /// An AI draft of the commit message is being written.
    pub drafting: bool,
    /// Bumped to remount the commit inputs after a programmatic change.
    pub draft_rev: u64,
    pub page: Page,
    /// Back / forward navigation over selections (toolbar Prev / Next).
    pub back: Vec<Selection>,
    pub forward: Vec<Selection>,
    /// History search (message, author, SHA).
    pub search: String,
    /// Message-body matches for `search`, once `git log --grep` answered.
    pub search_hits: SearchHits,
    /// Show only matching commits instead of highlighting them in the graph.
    pub search_filter: bool,
    /// File history / blame panel.
    pub file: Option<FileView>,
    /// Files expanded in the commit details.
    pub expanded: std::collections::HashSet<String>,
    /// Details show a folder tree instead of the changeset.
    pub tree: bool,
    /// Commit details show the whole message, not just its first line.
    pub message_expanded: bool,
    /// Scroll the graph to this row once rendered.
    pub scroll_to: Option<usize>,
    /// Pull requests page.
    pub prs: PrPanel,
}

/// A running operation (one per repository at a time).
#[derive(Debug, Clone)]
pub struct Busy {
    pub label: String,
    pub cancel: Option<CancellationToken>,
}

/// Single-field (+ optional message / checkbox) prompts that build a [`RepoOp`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptKind {
    CreateBranch { start: Option<String> },
    RenameBranch { old: String },
    CreateTag { target: String },
    StashSave,
    CheckoutRemoteAs { remote_branch: String },
}

impl PromptKind {
    /// `(title, field label, message label, checkbox label + default)`.
    #[must_use]
    pub fn labels(
        &self,
    ) -> (
        &'static str,
        &'static str,
        Option<&'static str>,
        Option<(&'static str, bool)>,
    ) {
        match self {
            Self::CreateBranch { .. } => (
                "Create branch",
                "Branch name",
                None,
                Some(("Check out after creating", true)),
            ),
            Self::RenameBranch { .. } => ("Rename branch", "New name", None, None),
            Self::CreateTag { .. } => (
                "Create tag",
                "Tag name",
                Some("Message (makes an annotated tag)"),
                None,
            ),
            Self::StashSave => (
                "Stash changes",
                "Message (optional)",
                None,
                Some(("Include untracked files", true)),
            ),
            Self::CheckoutRemoteAs { .. } => {
                ("Check out remote branch", "Local branch name", None, None)
            }
        }
    }

    #[must_use]
    pub fn build(&self, text: &str, message: &str, checked: bool) -> Option<RepoOp> {
        let t = text.trim().to_owned();
        if t.is_empty() && *self != Self::StashSave {
            return None;
        }
        Some(match self {
            Self::CreateBranch { start } => RepoOp::CreateBranch {
                name: t,
                start: start.clone(),
                checkout: checked,
            },
            Self::RenameBranch { old } => RepoOp::RenameBranch {
                old: old.clone(),
                new: t,
            },
            Self::CreateTag { target } => RepoOp::CreateTag {
                name: t,
                target: target.clone(),
                message: (!message.trim().is_empty()).then(|| message.trim().to_owned()),
            },
            Self::StashSave => RepoOp::StashSave {
                message: t,
                include_untracked: checked,
            },
            Self::CheckoutRemoteAs { remote_branch } => RepoOp::CheckoutRemote {
                remote_branch: remote_branch.clone(),
                local: t,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Dialog {
    /// Confirm, then run `op` in the active repository.
    Confirm {
        title: String,
        body: String,
        confirm: String,
        destructive: bool,
        op: RepoOp,
    },
    Prompt {
        kind: PromptKind,
        initial: String,
    },
    Reset {
        target: String,
        label: String,
    },
    Clone,
    AddRemote,
    QuickOpen,
    /// Choose a branch to merge into / rebase the current branch onto.
    PickBranch {
        rebase: bool,
    },
    RepoSettings(RepoId),
    Profiles(Option<ProfileId>),
    /// Forge detection overrides and access token for a repository.
    ForgeSettings(RepoId),
    /// New pull / merge request from the current branch.
    CreatePr,
    /// Comment or review the selected pull request.
    PrReview {
        number: u64,
        event: ReviewEvent,
    },
    /// Merge the selected pull request on the forge.
    PrMerge {
        number: u64,
    },
}
