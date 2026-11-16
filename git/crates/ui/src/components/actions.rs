//! Context-menu actions for branches, tags, stashes, remotes and commits.
//! Destructive ones go through a confirmation dialog.

use crate::app::GitView;
use crate::components::menu::MenuItem;
use crate::state::{Dialog, PromptKind};
use ferrumgit_domain::{BisectMark, GitRef, Oid, RefKind, RepoOp};
use gpui::{Context, Window};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Checkout(String),
    /// Local tracking branch for `origin/x` (switches to it if it exists).
    CheckoutRemote(String),
    CheckoutCommit(Oid),
    MergeIntoCurrent(String),
    RebaseCurrentOnto(String),
    BranchFrom(String),
    Rename(String),
    Delete(String),
    DeleteRemote(String, String),
    Pull,
    Push,
    ForcePush,
    PushTag(String),
    DeleteTag(String),
    DeleteRemoteTag(String),
    TagAt(String),
    CherryPick(Oid),
    /// Rewrite the commits after this one (`rebase -i`).
    InteractiveRebase(Oid),
    /// Start bisecting with this commit good (`true`) or bad, HEAD the other.
    BisectStart(Oid, bool),
    BisectMark(BisectMark, Oid),
    Revert(Oid),
    Reset(Oid),
    StashApply(usize),
    StashPop(usize),
    StashDrop(usize),
    FetchRemote(String),
    RemoveRemote(String),
    Copy(String),
}

impl Action {
    #[must_use]
    pub fn item(&self) -> MenuItem {
        match self {
            Self::Checkout(b) => MenuItem::new(format!("Check out {b}")),
            Self::CheckoutRemote(b) => MenuItem::new(format!("Check out {b}")),
            Self::CheckoutCommit(_) => MenuItem::new("Check out this commit (detached)"),
            Self::MergeIntoCurrent(b) => {
                MenuItem::new(format!("Merge {b} into current branch")).sep()
            }
            Self::RebaseCurrentOnto(b) => MenuItem::new(format!("Rebase current branch onto {b}")),
            Self::BranchFrom(_) => MenuItem::new("Create branch here…").sep(),
            Self::Rename(_) => MenuItem::new("Rename…"),
            Self::Delete(b) => MenuItem::new(format!("Delete {b}…")).danger().sep(),
            Self::DeleteRemote(r, b) => MenuItem::new(format!("Delete {r}/{b} on remote…"))
                .danger()
                .sep(),
            Self::Pull => MenuItem::new("Pull"),
            Self::Push => MenuItem::new("Push"),
            Self::ForcePush => MenuItem::new("Force push (with lease)…").danger(),
            Self::PushTag(_) => MenuItem::new("Push tag to origin"),
            Self::DeleteTag(t) => MenuItem::new(format!("Delete tag {t}…")).danger().sep(),
            Self::DeleteRemoteTag(_) => MenuItem::new("Delete tag from origin…").danger(),
            Self::TagAt(_) => MenuItem::new("Create tag here…"),
            Self::CherryPick(_) => MenuItem::new("Cherry-pick onto current branch").sep(),
            Self::InteractiveRebase(_) => MenuItem::new("Interactive rebase onto this commit…"),
            Self::BisectStart(_, true) => {
                MenuItem::new("Bisect: this commit is good, HEAD is bad").sep()
            }
            Self::BisectStart(_, false) => MenuItem::new("Bisect: start with this commit bad"),
            Self::BisectMark(mark, _) => {
                let item = MenuItem::new(format!("Bisect: mark this commit {}", mark.command()));
                if *mark == BisectMark::Good {
                    item.sep()
                } else {
                    item
                }
            }
            Self::Revert(_) => MenuItem::new("Revert commit…"),
            Self::Reset(_) => MenuItem::new("Reset current branch to here…").danger(),
            Self::StashApply(_) => MenuItem::new("Apply stash"),
            Self::StashPop(_) => MenuItem::new("Pop stash"),
            Self::StashDrop(_) => MenuItem::new("Drop stash…").danger().sep(),
            Self::FetchRemote(r) => MenuItem::new(format!("Fetch {r}")),
            Self::RemoveRemote(r) => MenuItem::new(format!("Remove remote {r}…")).danger().sep(),
            Self::Copy(_) => MenuItem::new("Copy name").sep(),
        }
    }
}

/// Menu for a branch or tag row.
#[must_use]
pub fn for_ref(r: &GitRef, refs: &[GitRef]) -> Vec<Action> {
    let n = r.name.clone();
    match r.kind {
        Some(RefKind::LocalBranch) if r.is_head => {
            vec![
                Action::Pull,
                Action::Push,
                Action::ForcePush,
                Action::BranchFrom(n.clone()),
                Action::Rename(n.clone()),
                Action::Copy(n),
            ]
        }
        Some(RefKind::LocalBranch) => vec![
            Action::Checkout(n.clone()),
            Action::MergeIntoCurrent(n.clone()),
            Action::RebaseCurrentOnto(n.clone()),
            Action::BranchFrom(n.clone()),
            Action::Rename(n.clone()),
            Action::Delete(n.clone()),
            Action::Copy(n),
        ],
        Some(RefKind::RemoteBranch) => {
            let (remote, branch) = r
                .remote_parts()
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
                .unwrap_or_default();
            let mut v = vec![
                Action::CheckoutRemote(n.clone()),
                Action::MergeIntoCurrent(n.clone()),
                Action::RebaseCurrentOnto(n.clone()),
                Action::BranchFrom(n.clone()),
                Action::DeleteRemote(remote, branch),
                Action::Copy(n),
            ];
            if refs.iter().any(|l| {
                l.is(RefKind::LocalBranch) && l.upstream.as_deref() == Some(&r.name) && l.is_head
            }) {
                v.remove(0);
            }
            v
        }
        Some(RefKind::Tag) => vec![
            Action::CheckoutCommit(r.target.clone()),
            Action::BranchFrom(n.clone()),
            Action::PushTag(n.clone()),
            Action::DeleteTag(n.clone()),
            Action::DeleteRemoteTag(n.clone()),
            Action::Copy(n),
        ],
        None => Vec::new(),
    }
}

/// Menu for a commit row in the graph (bisect entries depend on whether a
/// bisect session is running).
#[must_use]
pub fn for_commit(oid: &Oid, bisecting: bool) -> Vec<Action> {
    let mut items = vec![
        Action::CheckoutCommit(oid.clone()),
        Action::BranchFrom(oid.0.clone()),
        Action::TagAt(oid.0.clone()),
        Action::CherryPick(oid.clone()),
        Action::InteractiveRebase(oid.clone()),
        Action::Revert(oid.clone()),
        Action::Reset(oid.clone()),
    ];
    if bisecting {
        items.extend(
            [BisectMark::Good, BisectMark::Bad, BisectMark::Skip]
                .map(|m| Action::BisectMark(m, oid.clone())),
        );
    } else {
        items.push(Action::BisectStart(oid.clone(), true));
        items.push(Action::BisectStart(oid.clone(), false));
    }
    items.push(Action::Copy(oid.0.clone()));
    items
}

fn confirm(title: String, body: String, confirm: &str, op: RepoOp) -> Dialog {
    Dialog::Confirm {
        title,
        body,
        confirm: confirm.to_owned(),
        destructive: true,
        op,
    }
}

impl GitView {
    pub fn perform(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.state.active else {
            return;
        };
        let refs = self
            .state
            .view(id)
            .data
            .map(|d| d.snapshot.refs.clone())
            .unwrap_or_default();
        let pull_mode = self.state.settings.pull_mode;
        let prune = self.state.settings.prune_on_fetch;
        let dialog = match action {
            Action::Checkout(b) => return self.op(id, RepoOp::Checkout(b), cx),
            Action::CheckoutRemote(rb) => {
                let local_name = rb.split_once('/').map_or(rb.as_str(), |(_, b)| b).to_owned();
                if refs.iter().any(|r| r.is(RefKind::LocalBranch) && r.name == local_name) {
                    if refs
                        .iter()
                        .any(|r| r.name == local_name && r.upstream.as_deref() == Some(&rb))
                    {
                        return self.op(id, RepoOp::Checkout(local_name), cx);
                    }
                    Dialog::Prompt {
                        kind: PromptKind::CheckoutRemoteAs { remote_branch: rb },
                        initial: format!("{local_name}-2"),
                    }
                } else {
                    return self.op(
                        id,
                        RepoOp::CheckoutRemote {
                            remote_branch: rb,
                            local: local_name,
                        },
                        cx,
                    );
                }
            }
            Action::CheckoutCommit(oid) => return self.op(id, RepoOp::CheckoutDetached(oid), cx),
            Action::MergeIntoCurrent(b) => {
                return self.op(id, RepoOp::Merge { branch: b, no_ff: false }, cx);
            }
            Action::RebaseCurrentOnto(b) => return self.op(id, RepoOp::Rebase { onto: b }, cx),
            Action::BranchFrom(start) => Dialog::Prompt {
                kind: PromptKind::CreateBranch { start: Some(start) },
                initial: String::new(),
            },
            Action::Rename(b) => Dialog::Prompt {
                kind: PromptKind::RenameBranch { old: b.clone() },
                initial: b,
            },
            Action::Delete(b) => {
                let merged_hint =
                    "Unmerged commits are only reachable from this branch and will be lost.";
                confirm(
                    format!("Delete branch {b}?"),
                    format!("The local branch “{b}” will be force-deleted. {merged_hint}"),
                    "Delete",
                    RepoOp::DeleteBranch { name: b, force: true },
                )
            }
            Action::DeleteRemote(r, b) => confirm(
                format!("Delete {r}/{b}?"),
                format!("The branch “{b}” will be deleted on the remote “{r}” for everyone."),
                "Delete on remote",
                RepoOp::DeleteRemoteBranch { remote: r, branch: b },
            ),
            Action::Pull => return self.op(id, RepoOp::Pull(pull_mode), cx),
            Action::Push => {
                return self.op(
                    id,
                    RepoOp::Push {
                        remote: None,
                        force_with_lease: false,
                    },
                    cx,
                );
            }
            Action::ForcePush => confirm(
                "Force push?".into(),
                "Overwrites the remote branch with your local one. --force-with-lease refuses if someone else pushed since your last fetch.".into(),
                "Force push",
                RepoOp::Push {
                    remote: None,
                    force_with_lease: true,
                },
            ),
            Action::PushTag(t) => {
                return self.op(
                    id,
                    RepoOp::PushTag {
                        remote: "origin".into(),
                        tag: t,
                    },
                    cx,
                );
            }
            Action::DeleteTag(t) => confirm(
                format!("Delete tag {t}?"),
                "The local tag is deleted; the remote is not touched.".into(),
                "Delete",
                RepoOp::DeleteTag(t),
            ),
            Action::DeleteRemoteTag(t) => confirm(
                format!("Delete tag {t} from origin?"),
                "The tag is deleted on the remote for everyone.".into(),
                "Delete on remote",
                RepoOp::DeleteRemoteTag {
                    remote: "origin".into(),
                    tag: t,
                },
            ),
            Action::TagAt(target) => Dialog::Prompt {
                kind: PromptKind::CreateTag { target },
                initial: String::new(),
            },
            Action::CherryPick(oid) => return self.op(id, RepoOp::CherryPick(oid), cx),
            Action::InteractiveRebase(oid) => {
                return self.open_interactive_rebase(oid, window, cx);
            }
            Action::BisectStart(oid, good) => {
                let head = self
                    .state
                    .view(id)
                    .data
                    .and_then(|d| d.snapshot.status.branch.oid.clone());
                let (bad, good) = if good {
                    (head, Some(oid))
                } else {
                    (Some(oid), None)
                };
                return self.op(id, RepoOp::BisectStart { bad, good }, cx);
            }
            Action::BisectMark(mark, oid) => {
                return self.op(id, RepoOp::BisectMark { mark, rev: Some(oid) }, cx);
            }
            Action::Revert(oid) => Dialog::Confirm {
                title: "Revert commit?".into(),
                body: format!("Creates a new commit that undoes {}.", oid.short()),
                confirm: "Revert".into(),
                destructive: false,
                op: RepoOp::Revert(oid),
            },
            Action::Reset(oid) => {
                let label = oid.short().to_owned();
                Dialog::Reset {
                    target: oid.0,
                    label,
                }
            }
            Action::StashApply(i) => return self.op(id, RepoOp::StashApply(i), cx),
            Action::StashPop(i) => return self.op(id, RepoOp::StashPop(i), cx),
            Action::StashDrop(i) => confirm(
                "Drop stash?".into(),
                format!("stash@{{{i}}} will be deleted permanently."),
                "Drop",
                RepoOp::StashDrop(i),
            ),
            Action::FetchRemote(r) => {
                return self.op(
                    id,
                    RepoOp::Fetch {
                        remote: Some(r),
                        prune,
                    },
                    cx,
                );
            }
            Action::RemoveRemote(r) => confirm(
                format!("Remove remote {r}?"),
                "The remote and its remote-tracking branches are removed from this repository."
                    .into(),
                "Remove",
                RepoOp::RemoveRemote(r),
            ),
            Action::Copy(text) => {
                self.copy(&text, cx);
                return self.notify(format!("Copied {text}"), false, cx);
            }
        };
        self.open_dialog(dialog, window, cx);
    }

    /// Click on a ref: select and scroll to its commit.
    pub fn reveal(&mut self, target: &Oid, cx: &mut Context<Self>) {
        if let Some(id) = self.state.active {
            self.reveal_commit(id, target, cx);
        }
    }
}
