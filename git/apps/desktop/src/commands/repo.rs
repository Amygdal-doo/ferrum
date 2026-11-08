//! Loading repositories, running operations, selection and diffs.

use super::{background, run};
use crate::state::{AppState, Busy, DiffView, Page, Selection};
use dioxus::prelude::*;
use ferrumgit_application::{AppServices, CancellationToken, patch};
use ferrumgit_domain::{DiffSource, FileStatus, Oid, RepoId, RepoOp};
use std::sync::Arc;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(3);

/// (Re)loads status, graph, refs, stashes and remotes.
pub fn load(state: AppState, services: AppServices, id: RepoId) {
    let Some(entry) = state.entry(id) else { return };
    state.update_view(id, |v| v.loading = true);
    let limit = state.log_limit();
    let svc = services.clone();
    spawn(async move {
        let path = entry.path.clone();
        let result = run(&services, async move { svc.load(&path, limit).await }).await;
        state.update_view(id, |v| {
            v.loading = false;
            v.fingerprint.clear();
            match result {
                Ok(data) => {
                    v.error = None;
                    // Drop a selection whose commit/stash is gone.
                    let keep = match &v.selection {
                        Selection::Wip => true,
                        Selection::Commit(oid) => {
                            data.snapshot.commits.iter().any(|c| &c.oid == oid)
                        }
                        Selection::Stash(i) => *i < data.snapshot.stashes.len(),
                    };
                    if !keep {
                        v.selection = Selection::Wip;
                        v.details = None;
                    }
                    v.data = Some(Arc::new(data));
                }
                Err(e) => v.error = Some(e),
            }
        });
        // History opens on HEAD (like Tower) when nothing is selected yet.
        let v = state.view(id);
        if v.page == Page::History
            && v.selection == Selection::Wip
            && let Some(first) = v.data.as_ref().and_then(|d| d.snapshot.commits.first())
        {
            select(
                state,
                services.clone(),
                id,
                Selection::Commit(first.oid.clone()),
            );
        }
        refresh_diff(state, services, id);
    });
}

/// Reloads the active repository when its status, refs or stashes change on disk.
pub async fn poll_loop(state: AppState, services: AppServices) {
    loop {
        let _ = background(&services, tokio::time::sleep(POLL_INTERVAL)).await;
        let Some(id) = *state.active.peek() else {
            continue;
        };
        let Some(entry) = state.entry(id) else {
            continue;
        };
        let view = state.views.peek().get(&id).cloned().unwrap_or_default();
        if view.loading || view.data.is_none() || state.busy.peek().contains_key(&id) {
            continue;
        }
        let svc = services.clone();
        let Ok(fp) = run(&services, async move { svc.fingerprint(&entry.path).await }).await else {
            continue;
        };
        if view.fingerprint.is_empty() {
            state.update_view(id, |v| v.fingerprint = fp);
        } else if view.fingerprint != fp {
            load(state, services.clone(), id);
        }
    }
}

/// Runs `op` in repository `id` with its profile; reloads afterwards.
pub fn op(mut state: AppState, services: AppServices, id: RepoId, op: RepoOp) {
    let Some(entry) = state.entry(id) else { return };
    if state.is_busy(id) {
        state.notify(
            "Another operation is still running in this repository",
            true,
        );
        return;
    }
    let profile = state.profile_of(&entry);
    let cancel = op.is_network().then(CancellationToken::new);
    state.busy.write().insert(
        id,
        Busy {
            label: op.label(),
            cancel: cancel.clone(),
        },
    );
    let svc = services.clone();
    spawn(async move {
        let o = op.clone();
        let result = run(&services, async move {
            svc.run_op(&entry, profile.as_ref(), &o, cancel).await
        })
        .await;
        state.busy.write().remove(&id);
        match result {
            Ok(summary) => {
                if let RepoOp::Commit { .. } = op {
                    state.update_view(id, |v| {
                        v.summary.clear();
                        v.description.clear();
                        v.amend = false;
                        v.draft_rev += 1;
                    });
                }
                if op.is_network()
                    || matches!(
                        op,
                        RepoOp::Commit { .. } | RepoOp::Merge { .. } | RepoOp::Rebase { .. }
                    )
                {
                    let msg = summary
                        .filter(|s| s.len() < 160)
                        .unwrap_or_else(|| format!("{} — done", op.label()));
                    state.notify(msg, false);
                }
            }
            Err(e) if e == "cancelled" => state.notify(format!("{} cancelled", op.label()), false),
            Err(e) => state.notify(e, true),
        }
        load(state, services, id);
    });
}

/// Runs `op` in the active repository.
pub fn active_op(state: AppState, services: AppServices, o: RepoOp) {
    if let Some(id) = *state.active.peek() {
        op(state, services, id, o);
    }
}

pub fn cancel(state: AppState, id: RepoId) {
    if let Some(token) = state.busy.peek().get(&id).and_then(|b| b.cancel.clone()) {
        token.cancel();
    }
}

/// User-initiated selection: recorded for Prev / Next.
pub fn navigate(state: AppState, services: AppServices, id: RepoId, sel: Selection) {
    let current = state.view(id).selection;
    if current != sel {
        state.update_view(id, |v| {
            v.back.push(current);
            if v.back.len() > 200 {
                v.back.remove(0);
            }
            v.forward.clear();
        });
    }
    select(state, services, id, sel);
}

/// Toolbar Prev (`back = true`) / Next.
pub fn go(state: AppState, services: AppServices, id: RepoId, back: bool) {
    let mut target = None;
    state.update_view(id, |v| {
        let (from, to) = if back {
            (&mut v.back, &mut v.forward)
        } else {
            (&mut v.forward, &mut v.back)
        };
        if let Some(s) = from.pop() {
            to.push(v.selection.clone());
            target = Some(s);
        }
    });
    if let Some(s) = target {
        select(state, services, id, s);
    }
}

/// Switches the workspace page, picking a sensible selection for it.
pub fn set_page(state: AppState, services: AppServices, id: RepoId, page: Page) {
    let view = state.view(id);
    let sel = match page {
        Page::WorkingCopy => Selection::Wip,
        Page::History => std::iter::once(&view.selection)
            .chain(view.back.iter().rev())
            .find(|s| matches!(s, Selection::Commit(_)))
            .cloned()
            .or_else(|| {
                view.data
                    .as_ref()
                    .and_then(|d| d.snapshot.commits.first())
                    .map(|c| Selection::Commit(c.oid.clone()))
            })
            .unwrap_or(Selection::Wip),
        Page::Stashes => Selection::Stash(0),
    };
    state.update_view(id, |v| v.page = page);
    let has_stash = view
        .data
        .as_ref()
        .is_some_and(|d| !d.snapshot.stashes.is_empty());
    if page == Page::Stashes && !has_stash {
        state.update_view(id, |v| v.details = None);
        return;
    }
    navigate(state, services, id, sel);
}

pub fn select(state: AppState, services: AppServices, id: RepoId, sel: Selection) {
    let Some(entry) = state.entry(id) else { return };
    let view = state.view(id);
    state.update_view(id, |v| {
        v.page = match sel {
            Selection::Wip => Page::WorkingCopy,
            Selection::Commit(_) => Page::History,
            Selection::Stash(_) => Page::Stashes,
        };
        v.selection = sel.clone();
        v.details = None;
        if v.diff
            .as_ref()
            .is_some_and(|d| d.source == DiffSource::Commit)
            || sel != Selection::Wip
        {
            v.diff = None;
        }
    });
    let stash = match &sel {
        Selection::Wip => return,
        Selection::Commit(_) => None,
        Selection::Stash(i) => view
            .data
            .as_ref()
            .and_then(|d| d.snapshot.stashes.get(*i).cloned()),
    };
    let svc = services.clone();
    spawn(async move {
        let s = sel.clone();
        let result = run(&services, async move {
            match (&s, stash) {
                (Selection::Stash(_), Some(st)) => svc.stash_details(&entry.path, &st).await,
                (Selection::Commit(oid), _) => svc.commit_details(&entry.path, oid).await,
                _ => Err(ferrumgit_application::AppError::Invalid(
                    "stash no longer exists".into(),
                )),
            }
        })
        .await;
        match result {
            Ok(d) => state.update_view(id, |v| {
                if v.selection == sel {
                    v.expanded = d
                        .files
                        .first()
                        .map(|f| f.path.clone())
                        .into_iter()
                        .collect();
                    v.details = Some(Arc::new(d));
                }
            }),
            Err(e) => state.notify(e, true),
        }
    });
}

/// Selects a commit and scrolls the graph to it.
pub fn reveal_commit(state: AppState, services: AppServices, id: RepoId, oid: &Oid) {
    let row = state
        .view(id)
        .data
        .as_ref()
        .and_then(|d| d.snapshot.commits.iter().position(|c| &c.oid == oid));
    match row {
        Some(r) => {
            state.update_view(id, |v| v.scroll_to = Some(r));
            navigate(state, services, id, Selection::Commit(oid.clone()));
        }
        None => state.notify("That commit is outside the loaded history", true),
    }
}

/// Opens the diff of a working-copy file.
pub fn open_file(
    state: AppState,
    services: AppServices,
    id: RepoId,
    file: &FileStatus,
    staged: bool,
) {
    let source = if staged {
        DiffSource::Staged
    } else if file.worktree == ferrumgit_domain::ChangeKind::Untracked {
        DiffSource::Untracked
    } else {
        DiffSource::Unstaged
    };
    fetch_diff(
        state,
        services,
        id,
        file.path.clone(),
        file.orig_path.clone(),
        source,
        true,
    );
}

fn fetch_diff(
    state: AppState,
    services: AppServices,
    id: RepoId,
    path: String,
    orig: Option<String>,
    source: DiffSource,
    notify_empty: bool,
) {
    let Some(entry) = state.entry(id) else { return };
    let svc = services.clone();
    spawn(async move {
        let (p, o, s) = (path.clone(), orig.clone(), source.clone());
        let result = run(&services, async move {
            svc.file_diff(&entry.path, &p, o.as_deref(), &s).await
        })
        .await;
        match result {
            Ok(Some(file)) => state.update_view(id, |v| {
                v.diff = Some(DiffView {
                    file: Arc::new(file),
                    source,
                    orig,
                    lines: Default::default(),
                });
            }),
            Ok(None) => {
                state.update_view(id, |v| v.diff = None);
                if notify_empty {
                    state.notify("No textual changes (mode change or empty file)", false);
                }
            }
            Err(e) => state.notify(e, true),
        }
    });
}

/// After a reload, re-reads the open working-copy diff (or closes it).
fn refresh_diff(state: AppState, services: AppServices, id: RepoId) {
    let Some(d) = state.view(id).diff else { return };
    if d.source == DiffSource::Commit {
        return;
    }
    fetch_diff(
        state,
        services,
        id,
        d.file.path.clone(),
        d.orig.clone(),
        d.source,
        false,
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunkAction {
    Stage,
    Unstage,
    Discard,
}

/// Stages / unstages / discards one hunk, or only the selected lines in it.
pub fn hunk(
    state: AppState,
    services: AppServices,
    id: RepoId,
    hunk: usize,
    action: HunkAction,
    only_selected: bool,
) {
    let Some(d) = state.view(id).diff else { return };
    let (cached, reverse) = match action {
        HunkAction::Stage => (true, false),
        HunkAction::Unstage => (true, true),
        HunkAction::Discard => (false, true),
    };
    let selected: Vec<usize> = d
        .lines
        .iter()
        .filter(|(h, _)| *h == hunk)
        .map(|(_, l)| *l)
        .collect();
    let lines = (only_selected && !selected.is_empty()).then_some(selected.as_slice());
    let Some(p) = patch::hunk_patch(&d.file, hunk, lines, reverse) else {
        state.notify("Select changed (+/−) lines first", true);
        return;
    };
    state.update_view(id, |v| {
        if let Some(d) = v.diff.as_mut() {
            d.lines.clear();
        }
    });
    op(
        state,
        services,
        id,
        RepoOp::ApplyPatch {
            patch: p,
            cached,
            reverse,
        },
    );
}

/// Loads the last commit message into the draft when "Amend" is ticked.
pub fn toggle_amend(state: AppState, services: AppServices, id: RepoId, on: bool) {
    state.update_view(id, |v| v.amend = on);
    let Some(entry) = state.entry(id) else { return };
    if !on || !state.view(id).summary.trim().is_empty() {
        return;
    }
    let svc = services.clone();
    spawn(async move {
        if let Ok(msg) = run(
            &services,
            async move { svc.head_message(&entry.path).await },
        )
        .await
        {
            let (summary, body) = msg.split_once('\n').unwrap_or((&msg, ""));
            let (summary, body) = (summary.to_owned(), body.trim().to_owned());
            state.update_view(id, |v| {
                v.summary = summary;
                v.description = body;
                v.draft_rev += 1;
            });
        }
    });
}
