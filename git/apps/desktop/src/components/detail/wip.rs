//! Working copy: operation banner, conflicts, unstaged / staged lists, commit box.

use super::files::{FileRow, ListHeader, RowAction};
use crate::commands::repo;
use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::state::{AppState, Dialog};
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use ferrumgit_domain::{DiffSource, FileStatus, RepoId, RepoOp};

const STAGE: RowAction = RowAction {
    label: "Stage",
    title: "Stage file",
    danger: false,
};
const DISCARD: RowAction = RowAction {
    label: "Discard",
    title: "Discard changes in this file",
    danger: true,
};
const UNSTAGE: RowAction = RowAction {
    label: "Unstage",
    title: "Unstage file",
    danger: false,
};
const OURS: RowAction = RowAction {
    label: "Ours",
    title: "Resolve with our version",
    danger: false,
};
const THEIRS: RowAction = RowAction {
    label: "Theirs",
    title: "Resolve with their version",
    danger: false,
};
const RESOLVED: RowAction = RowAction {
    label: "Resolved",
    title: "Mark as resolved (stage the file as edited)",
    danger: false,
};

fn discard_dialog(paths: Vec<String>) -> Dialog {
    let body = if paths.len() == 1 {
        format!(
            "Changes to “{}” will be lost. Untracked files are deleted.",
            paths[0]
        )
    } else {
        format!(
            "Changes to {} files will be lost. Untracked files are deleted.",
            paths.len()
        )
    };
    Dialog::Confirm {
        title: "Discard changes?".into(),
        body,
        confirm: "Discard".into(),
        destructive: true,
        op: RepoOp::Discard(paths),
    }
}

#[component]
pub fn WipPanel(id: RepoId) -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let view = state.view(id);
    let Some(data) = view.data.clone() else {
        return rsx! {};
    };
    let status = data.snapshot.status.clone();
    let conflicted: Vec<FileStatus> = status.conflicted().into_iter().cloned().collect();
    let unstaged: Vec<FileStatus> = status
        .unstaged()
        .into_iter()
        .filter(|f| !f.is_conflicted())
        .cloned()
        .collect();
    let staged: Vec<FileStatus> = status.staged().into_iter().cloned().collect();
    let open_diff = view
        .diff
        .as_ref()
        .map(|d| (d.file.path.clone(), d.source.clone()));
    let is_open = move |p: &str, staged: bool| {
        open_diff
            .as_ref()
            .is_some_and(|(path, src)| path == p && (*src == DiffSource::Staged) == staged)
    };
    let busy = state.is_busy(id);
    let run = {
        let s = services.clone();
        move |op: RepoOp| repo::op(state, s.clone(), id, op)
    };
    let unstaged_paths: Vec<String> = unstaged.iter().map(|f| f.path.clone()).collect();
    let (r_stage_all, r_unstage_all, r_cont, r_abort) =
        (run.clone(), run.clone(), run.clone(), run.clone());

    rsx! {
        div { class: "flex min-h-0 flex-1 flex-col",
            if let Some(op) = status.operation {
                div { class: "flex shrink-0 items-center gap-2 border-b bg-warning-light px-3 py-2 text-xs text-warning-dark",
                    span { class: "flex-1 font-medium",
                        if conflicted.is_empty() { "{op.label()} in progress — ready to continue" } else { "{op.label()} in progress — resolve {conflicted.len()} conflict(s)" }
                    }
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline, class: "h-6 text-xs", disabled: busy, onclick: move |_| r_abort(RepoOp::Abort), "Abort" }
                    Button { size: ButtonSize::Sm, class: "h-6 text-xs", disabled: busy || !conflicted.is_empty(), onclick: move |_| r_cont(RepoOp::Continue), "Continue" }
                }
            }
            div { class: "flex min-h-0 flex-1 flex-col overflow-y-auto",
                if !conflicted.is_empty() {
                    ListHeader { title: "Conflicts", count: conflicted.len() }
                    div { class: "p-1",
                        for f in conflicted {
                            {
                                let (p, s1, run) = (f.path.clone(), services.clone(), run.clone());
                                let file = f.clone();
                                rsx! {
                                    FileRow { key: "c{f.path}", path: f.path.clone(), kind: f.index, active: is_open(&f.path, false),
                                        actions: vec![OURS, THEIRS, RESOLVED],
                                        on_open: move |_| repo::open_file(state, s1.clone(), id, &file, false),
                                        on_action: move |i: usize| run(match i { 0 => RepoOp::TakeOurs(vec![p.clone()]), 1 => RepoOp::TakeTheirs(vec![p.clone()]), _ => RepoOp::Stage(vec![p.clone()]) }) }
                                }
                            }
                        }
                    }
                }
                ListHeader { title: "Unstaged", count: unstaged.len(),
                    if !unstaged.is_empty() {
                        button { class: "rounded px-1.5 text-[11px] text-destructive hover:bg-accent", disabled: busy,
                            onclick: move |_| state.dialog.set(Some(discard_dialog(unstaged_paths.clone()))), "Discard all" }
                        button { class: "rounded px-1.5 text-[11px] font-medium hover:bg-accent", disabled: busy,
                            onclick: move |_| r_stage_all(RepoOp::StageAll), "Stage all" }
                    }
                }
                div { class: "p-1",
                    if unstaged.is_empty() { p { class: "px-2 py-3 text-xs text-muted-foreground", "No unstaged changes" } }
                    for f in unstaged {
                        {
                            let (p, s1, run) = (f.path.clone(), services.clone(), run.clone());
                            let file = f.clone();
                            rsx! {
                                FileRow { key: "u{f.path}", path: f.path.clone(), kind: f.worktree, active: is_open(&f.path, false),
                                    actions: vec![DISCARD, STAGE],
                                    on_open: move |_| repo::open_file(state, s1.clone(), id, &file, false),
                                    on_action: move |i: usize| if i == 1 { run(RepoOp::Stage(vec![p.clone()])) } else { state.dialog.set(Some(discard_dialog(vec![p.clone()]))) } }
                            }
                        }
                    }
                }
                ListHeader { title: "Staged", count: staged.len(),
                    if !staged.is_empty() {
                        button { class: "rounded px-1.5 text-[11px] font-medium hover:bg-accent", disabled: busy,
                            onclick: move |_| r_unstage_all(RepoOp::UnstageAll), "Unstage all" }
                    }
                }
                div { class: "p-1",
                    if staged.is_empty() { p { class: "px-2 py-3 text-xs text-muted-foreground", "Stage files or hunks to commit them" } }
                    for f in staged.clone() {
                        {
                            let (p, s1, run) = (f.path.clone(), services.clone(), run.clone());
                            let file = f.clone();
                            rsx! {
                                FileRow { key: "s{f.path}", path: f.path.clone(), kind: f.index, active: is_open(&f.path, true),
                                    actions: vec![UNSTAGE],
                                    on_open: move |_| repo::open_file(state, s1.clone(), id, &file, true),
                                    on_action: move |_| run(RepoOp::Unstage(vec![p.clone()])) }
                            }
                        }
                    }
                }
            }
            CommitBox { id, staged: staged.len() }
        }
    }
}

#[component]
fn CommitBox(id: RepoId, staged: usize) -> Element {
    let state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let view = state.view(id);
    let entry = state.entry(id);
    let profile = entry.as_ref().and_then(|e| state.profile_of(e));
    let who = profile
        .as_ref()
        .and_then(|p| p.identity().map(|(n, e)| format!("{n} <{e}>")));
    let amend = view.amend;
    let busy = state.is_busy(id);
    let summary_len = view.summary.chars().count();
    let can_commit = !busy && !view.summary.trim().is_empty() && (staged > 0 || amend);
    let label = match (amend, staged) {
        (true, _) => "Amend previous commit".to_owned(),
        (false, 0) => "Stage changes to commit".to_owned(),
        (false, n) => format!("Commit {n} file{}", if n == 1 { "" } else { "s" }),
    };
    let rev = view.draft_rev;
    let (summary0, desc0) = (view.summary.clone(), view.description.clone());
    let (s_amend, s_commit) = (services.clone(), services.clone());
    let commit = move || {
        let v = state.view(id);
        let msg = if v.description.trim().is_empty() {
            v.summary.trim().to_owned()
        } else {
            format!("{}\n\n{}", v.summary.trim(), v.description.trim())
        };
        repo::op(
            state,
            s_commit.clone(),
            id,
            RepoOp::Commit {
                message: msg,
                amend: v.amend,
            },
        );
    };
    let commit2 = commit.clone();

    rsx! {
        div { class: "flex shrink-0 flex-col gap-2 border-t bg-sidenav p-3",
            div { class: "flex items-center gap-2 text-xs",
                span { class: "flex-1 font-semibold", "Commit" }
                label { class: "flex items-center gap-1 text-muted-foreground",
                    input { r#type: "checkbox", checked: amend, onchange: move |e| repo::toggle_amend(state, s_amend.clone(), id, e.checked()) }
                    "Amend"
                }
            }
            for r in [rev] {
                {
                let commit = commit.clone();
                rsx! {
                div { key: "{r}", class: "flex flex-col gap-2",
                    div { class: "relative",
                        input {
                            class: "h-8 w-full rounded-md border bg-background px-2 pr-10 text-sm outline-none focus:border-ring select-text",
                            placeholder: "Summary (required)", "aria-label": "Commit summary",
                            initial_value: "{summary0}",
                            oninput: move |e| state.update_view(id, |v| v.summary = e.value()),
                            onkeydown: move |e: KeyboardEvent| {
                                let m = e.modifiers();
                                if e.key() == Key::Enter && (m.meta() || m.ctrl()) && can_commit { commit() }
                            },
                        }
                        span { class: if summary_len > 72 { "absolute right-2 top-2 text-[10px] text-warning" } else { "absolute right-2 top-2 text-[10px] text-muted-foreground" }, "{summary_len}" }
                    }
                    textarea {
                        class: "h-20 w-full resize-none rounded-md border bg-background px-2 py-1.5 text-sm outline-none focus:border-ring select-text",
                        placeholder: "Description", "aria-label": "Commit description",
                        initial_value: "{desc0}",
                        oninput: move |e| state.update_view(id, |v| v.description = e.value()),
                    }
                }
                }
                }
            }
            Button { class: "w-full", disabled: !can_commit, onclick: move |_| commit2(), "{label}" }
            p { class: "truncate text-[11px] text-muted-foreground", title: "Set per repository via its profile",
                match who { Some(w) => rsx! { "as {w}" }, None => rsx! { "as your git config identity" } }
                " · ⌘↩ to commit"
            }
        }
    }
}
