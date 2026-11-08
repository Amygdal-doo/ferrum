//! Context-menu actions for branches, tags, stashes, remotes and commits.
//! Destructive ones go through a confirmation dialog.

use crate::commands::{self, repo};
use crate::components::menu::MenuItem;
use crate::state::{AppState, Dialog, PromptKind};
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use ferrumgit_domain::{GitRef, Oid, RefKind, RepoOp};

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

/// Menu for a commit row in the graph.
#[must_use]
pub fn for_commit(oid: &Oid) -> Vec<Action> {
    vec![
        Action::CheckoutCommit(oid.clone()),
        Action::BranchFrom(oid.0.clone()),
        Action::TagAt(oid.0.clone()),
        Action::CherryPick(oid.clone()),
        Action::Revert(oid.clone()),
        Action::Reset(oid.clone()),
        Action::Copy(oid.0.clone()),
    ]
}

fn confirm(mut state: AppState, title: String, body: String, confirm: &str, op: RepoOp) {
    state.dialog.set(Some(Dialog::Confirm {
        title,
        body,
        confirm: confirm.to_owned(),
        destructive: true,
        op,
    }));
}

pub fn perform(mut state: AppState, services: AppServices, action: Action) {
    let Some(id) = *state.active.peek() else {
        return;
    };
    let run = |op: RepoOp| repo::op(state, services.clone(), id, op);
    let refs = state
        .view(id)
        .data
        .map(|d| d.snapshot.refs.clone())
        .unwrap_or_default();
    let prompt = |kind: PromptKind, initial: String| {
        let mut st = state;
        st.dialog.set(Some(Dialog::Prompt { kind, initial }));
    };
    match action {
        Action::Checkout(b) => run(RepoOp::Checkout(b)),
        Action::CheckoutRemote(rb) => {
            let local_name = rb.split_once('/').map_or(rb.as_str(), |(_, b)| b).to_owned();
            if refs.iter().any(|r| r.is(RefKind::LocalBranch) && r.name == local_name) {
                if refs.iter().any(|r| r.name == local_name && r.upstream.as_deref() == Some(&rb)) {
                    run(RepoOp::Checkout(local_name));
                } else {
                    prompt(PromptKind::CheckoutRemoteAs { remote_branch: rb }, format!("{local_name}-2"));
                }
            } else {
                run(RepoOp::CheckoutRemote { remote_branch: rb, local: local_name });
            }
        }
        Action::CheckoutCommit(oid) => run(RepoOp::CheckoutDetached(oid)),
        Action::MergeIntoCurrent(b) => run(RepoOp::Merge { branch: b, no_ff: false }),
        Action::RebaseCurrentOnto(b) => run(RepoOp::Rebase { onto: b }),
        Action::BranchFrom(start) => prompt(PromptKind::CreateBranch { start: Some(start) }, String::new()),
        Action::Rename(b) => prompt(PromptKind::RenameBranch { old: b.clone() }, b),
        Action::Delete(b) => {
            let merged_hint = "Unmerged commits are only reachable from this branch and will be lost.";
            confirm(state, format!("Delete branch {b}?"), format!("The local branch “{b}” will be force-deleted. {merged_hint}"), "Delete", RepoOp::DeleteBranch { name: b, force: true });
        }
        Action::DeleteRemote(r, b) => confirm(state, format!("Delete {r}/{b}?"), format!("The branch “{b}” will be deleted on the remote “{r}” for everyone."), "Delete on remote", RepoOp::DeleteRemoteBranch { remote: r, branch: b }),
        Action::Pull => run(RepoOp::Pull(state.settings.peek().pull_mode)),
        Action::Push => run(RepoOp::Push { remote: None, force_with_lease: false }),
        Action::ForcePush => confirm(state, "Force push?".into(), "Overwrites the remote branch with your local one. --force-with-lease refuses if someone else pushed since your last fetch.".into(), "Force push", RepoOp::Push { remote: None, force_with_lease: true }),
        Action::PushTag(t) => run(RepoOp::PushTag { remote: "origin".into(), tag: t }),
        Action::DeleteTag(t) => confirm(state, format!("Delete tag {t}?"), "The local tag is deleted; the remote is not touched.".into(), "Delete", RepoOp::DeleteTag(t)),
        Action::DeleteRemoteTag(t) => confirm(state, format!("Delete tag {t} from origin?"), "The tag is deleted on the remote for everyone.".into(), "Delete on remote", RepoOp::DeleteRemoteTag { remote: "origin".into(), tag: t }),
        Action::TagAt(target) => prompt(PromptKind::CreateTag { target }, String::new()),
        Action::CherryPick(oid) => run(RepoOp::CherryPick(oid)),
        Action::Revert(oid) => {
            let mut st = state;
            st.dialog.set(Some(Dialog::Confirm { title: "Revert commit?".into(), body: format!("Creates a new commit that undoes {}.", oid.short()), confirm: "Revert".into(), destructive: false, op: RepoOp::Revert(oid) }));
        }
        Action::Reset(oid) => {
            let label = oid.short().to_owned();
            state.dialog.set(Some(Dialog::Reset { target: oid.0, label }));
        }
        Action::StashApply(i) => run(RepoOp::StashApply(i)),
        Action::StashPop(i) => run(RepoOp::StashPop(i)),
        Action::StashDrop(i) => confirm(state, "Drop stash?".into(), format!("stash@{{{i}}} will be deleted permanently."), "Drop", RepoOp::StashDrop(i)),
        Action::FetchRemote(r) => run(RepoOp::Fetch { remote: Some(r), prune: state.settings.peek().prune_on_fetch }),
        Action::RemoveRemote(r) => confirm(state, format!("Remove remote {r}?"), "The remote and its remote-tracking branches are removed from this repository.".into(), "Remove", RepoOp::RemoveRemote(r)),
        Action::Copy(text) => {
            commands::copy(&text);
            state.notify(format!("Copied {text}"), false);
        }
    }
}

/// Click on a ref: select and scroll to its commit.
pub fn reveal(state: AppState, services: AppServices, target: &Oid) {
    if let Some(id) = *state.active.peek() {
        repo::reveal_commit(state, services, id, target);
    }
}
