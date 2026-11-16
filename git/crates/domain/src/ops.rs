//! Every repository action the UI can request. The application layer turns
//! each into one or more git invocations, using the repo's profile for
//! identity and SSH.

use crate::bisect::BisectMark;
use crate::commit::Oid;
use crate::rebase::RebasePlan;
use crate::undo::Undo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullMode {
    #[default]
    FastForwardOnly,
    Merge,
    Rebase,
}

impl PullMode {
    pub const ALL: [Self; 3] = [Self::FastForwardOnly, Self::Merge, Self::Rebase];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::FastForwardOnly => "Fast-forward only",
            Self::Merge => "Merge",
            Self::Rebase => "Rebase",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

impl ResetMode {
    #[must_use]
    pub const fn flag(self) -> &'static str {
        match self {
            Self::Soft => "--soft",
            Self::Mixed => "--mixed",
            Self::Hard => "--hard",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoOp {
    // Working copy
    Stage(Vec<String>),
    Unstage(Vec<String>),
    StageAll,
    UnstageAll,
    /// Discard working-tree changes; untracked files are deleted.
    Discard(Vec<String>),
    /// Apply a single-hunk patch (built by the git layer).
    ApplyPatch {
        patch: String,
        cached: bool,
        reverse: bool,
    },
    Commit {
        message: String,
        amend: bool,
    },
    // Branches
    Checkout(String),
    /// Create a local tracking branch for `origin/x` and switch to it.
    CheckoutRemote {
        remote_branch: String,
        local: String,
    },
    CheckoutDetached(Oid),
    CreateBranch {
        name: String,
        start: Option<String>,
        checkout: bool,
    },
    RenameBranch {
        old: String,
        new: String,
    },
    DeleteBranch {
        name: String,
        force: bool,
    },
    DeleteRemoteBranch {
        remote: String,
        branch: String,
    },
    SetUpstream {
        branch: String,
        upstream: String,
    },
    Merge {
        branch: String,
        no_ff: bool,
    },
    Rebase {
        onto: String,
    },
    /// Rewrites `base..HEAD` as planned (git's own `rebase -i` with a
    /// generated todo list).
    InteractiveRebase(RebasePlan),
    // Commits
    CherryPick(Oid),
    Revert(Oid),
    Reset {
        target: String,
        mode: ResetMode,
    },
    // Multi-step operations
    Continue,
    /// Skips the commit a rebase / cherry-pick / revert stopped at.
    Skip,
    Abort,
    TakeOurs(Vec<String>),
    TakeTheirs(Vec<String>),
    /// Writes the resolved `content` to a conflicted file and stages it.
    Resolve {
        path: String,
        content: String,
    },
    // Bisect
    /// Starts bisecting, optionally marking a bad and a good commit right away.
    BisectStart {
        bad: Option<Oid>,
        good: Option<Oid>,
    },
    /// Marks `rev` (default: the checked-out commit).
    BisectMark {
        mark: BisectMark,
        rev: Option<Oid>,
    },
    /// Ends bisecting and checks out where it started.
    BisectReset,
    // Tags
    CreateTag {
        name: String,
        target: String,
        message: Option<String>,
    },
    DeleteTag(String),
    PushTag {
        remote: String,
        tag: String,
    },
    DeleteRemoteTag {
        remote: String,
        tag: String,
    },
    // Stash
    StashSave {
        message: String,
        include_untracked: bool,
    },
    StashApply(usize),
    StashPop(usize),
    StashDrop(usize),
    // Remotes & network
    AddRemote {
        name: String,
        url: String,
    },
    RemoveRemote(String),
    Fetch {
        remote: Option<String>,
        prune: bool,
    },
    Pull(PullMode),
    /// Push the current branch (sets upstream on `remote` when it has none).
    Push {
        remote: Option<String>,
        force_with_lease: bool,
    },
    /// Reverse the last local commit / amend / checkout (see [`Undo`]).
    Undo(Undo),
}

impl RepoOp {
    /// Network operations need the profile's SSH environment.
    #[must_use]
    pub const fn is_network(&self) -> bool {
        matches!(
            self,
            Self::Fetch { .. }
                | Self::Pull(_)
                | Self::Push { .. }
                | Self::PushTag { .. }
                | Self::DeleteRemoteBranch { .. }
                | Self::DeleteRemoteTag { .. }
        )
    }

    /// Present-progressive label for the status bar.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Stage(_) | Self::StageAll => "Staging".into(),
            Self::Unstage(_) | Self::UnstageAll => "Unstaging".into(),
            Self::Discard(_) => "Discarding changes".into(),
            Self::ApplyPatch { .. } => "Applying hunk".into(),
            Self::Commit { amend: true, .. } => "Amending commit".into(),
            Self::Commit { .. } => "Committing".into(),
            Self::Checkout(b) => format!("Checking out {b}"),
            Self::CheckoutRemote { local, .. } => format!("Checking out {local}"),
            Self::CheckoutDetached(o) => format!("Checking out {}", o.short()),
            Self::CreateBranch { name, .. } => format!("Creating branch {name}"),
            Self::RenameBranch { new, .. } => format!("Renaming to {new}"),
            Self::DeleteBranch { name, .. } => format!("Deleting {name}"),
            Self::DeleteRemoteBranch { remote, branch } => format!("Deleting {remote}/{branch}"),
            Self::SetUpstream { upstream, .. } => format!("Tracking {upstream}"),
            Self::Merge { branch, .. } => format!("Merging {branch}"),
            Self::Rebase { onto } => format!("Rebasing onto {onto}"),
            Self::InteractiveRebase(plan) => format!("Rebasing onto {}", plan.base.short()),
            Self::CherryPick(o) => format!("Cherry-picking {}", o.short()),
            Self::Revert(o) => format!("Reverting {}", o.short()),
            Self::Reset { target, .. } => format!("Resetting to {}", short_target(target)),
            Self::Continue => "Continuing".into(),
            Self::Skip => "Skipping commit".into(),
            Self::Abort => "Aborting".into(),
            Self::TakeOurs(_) => "Taking ours".into(),
            Self::TakeTheirs(_) => "Taking theirs".into(),
            Self::Resolve { path, .. } => format!("Resolving {path}"),
            Self::BisectStart { .. } => "Starting bisect".into(),
            Self::BisectMark { mark, .. } => format!("Marking {}", mark.label().to_lowercase()),
            Self::BisectReset => "Ending bisect".into(),
            Self::CreateTag { name, .. } => format!("Creating tag {name}"),
            Self::DeleteTag(t) => format!("Deleting tag {t}"),
            Self::PushTag { tag, .. } => format!("Pushing tag {tag}"),
            Self::DeleteRemoteTag { tag, .. } => format!("Deleting remote tag {tag}"),
            Self::StashSave { .. } => "Stashing".into(),
            Self::StashApply(_) => "Applying stash".into(),
            Self::StashPop(_) => "Popping stash".into(),
            Self::StashDrop(_) => "Dropping stash".into(),
            Self::AddRemote { name, .. } => format!("Adding remote {name}"),
            Self::RemoveRemote(n) => format!("Removing remote {n}"),
            Self::Fetch { .. } => "Fetching".into(),
            Self::Pull(_) => "Pulling".into(),
            Self::Push { .. } => "Pushing".into(),
            Self::Undo(_) => "Undoing".into(),
        }
    }
}

fn short_target(t: &str) -> &str {
    if t.len() == 40 && t.bytes().all(|b| b.is_ascii_hexdigit()) {
        &t[..7]
    } else {
        t
    }
}
