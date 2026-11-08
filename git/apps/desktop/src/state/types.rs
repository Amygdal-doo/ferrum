//! Small UI-state value types.

use ferrumgit_application::{CancellationToken, RepoData};
use ferrumgit_domain::{CommitDetails, DiffSource, FileDiff, Oid, ProfileId, RepoId, RepoOp};
use std::collections::BTreeSet;
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

/// Per-repository UI state.
#[derive(Debug, Clone, Default)]
pub struct RepoView {
    pub data: Option<Arc<RepoData>>,
    pub error: Option<String>,
    pub loading: bool,
    pub selection: Selection,
    pub details: Option<Arc<CommitDetails>>,
    pub diff: Option<DiffView>,
    /// Last change fingerprint seen by the poller.
    pub fingerprint: String,
    pub summary: String,
    pub description: String,
    pub amend: bool,
    /// Bumped to remount the commit inputs after a programmatic change.
    pub draft_rev: u64,
    pub page: Page,
    /// Back / forward navigation over selections (toolbar Prev / Next).
    pub back: Vec<Selection>,
    pub forward: Vec<Selection>,
    /// History search (message, author, SHA).
    pub search: String,
    /// Files expanded in the commit details.
    pub expanded: std::collections::HashSet<String>,
    /// Details show a folder tree instead of the changeset.
    pub tree: bool,
    /// Scroll the graph to this row once rendered.
    pub scroll_to: Option<usize>,
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
}
