//! Loading repositories, running operations, selection and diffs.

use super::run;
use crate::app::FerrumGit;
use crate::state::{Busy, DiffView, Page, Selection};
use ferrumgit_application::{CancellationToken, patch};
use ferrumgit_domain::{DiffSource, FileStatus, Oid, RepoId, RepoOp, Undo};
use gpui::{Context, Task};
use std::sync::Arc;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunkAction {
    Stage,
    Unstage,
    Discard,
}

impl FerrumGit {
    /// (Re)loads status, graph, refs, stashes and remotes.
    pub fn load(&mut self, id: RepoId, cx: &mut Context<Self>) {
        let Some(entry) = self.state.entry(id) else {
            return;
        };
        self.state.update_view(id, |v| v.loading = true);
        let limit = self.state.log_limit();
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.load(&entry.path, limit).await },
            move |app, result, cx| {
                app.state.update_view(id, |v| {
                    v.loading = false;
                    v.fingerprint.clear();
                    match result {
                        Ok(data) => {
                            tracing::debug!(
                                commits = data.snapshot.commits.len(),
                                changes = data.snapshot.status.files.len(),
                                "repository loaded"
                            );
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
                let v = app.state.view(id);
                if v.page == Page::History
                    && v.selection == Selection::Wip
                    && let Some(first) = v.data.as_ref().and_then(|d| d.snapshot.commits.first())
                {
                    app.select(id, Selection::Commit(first.oid.clone()), cx);
                }
                app.refresh_diff(id, cx);
                app.refresh_file_view(id, cx);
                // New commits may match the active search.
                let search = app.state.view(id).search;
                if !search.trim().is_empty() {
                    app.search_changed(id, search, cx);
                }
            },
        );
    }

    /// Reloads the active repository when its status, refs or stashes change on disk.
    pub fn start_polling(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let job = this.update(cx, |app, _| {
                    let id = app.state.active?;
                    let entry = app.state.entry(id)?;
                    let view = app.state.views.get(&id).cloned().unwrap_or_default();
                    if view.loading || view.data.is_none() || app.state.is_busy(id) {
                        return None;
                    }
                    Some((id, entry, view.fingerprint, app.services.clone()))
                });
                let (id, entry, known, services) = match job {
                    Ok(Some(job)) => job,
                    Ok(None) => continue,
                    Err(_) => return,
                };
                let svc = services.clone();
                let Ok(fingerprint) =
                    run(&services, async move { svc.fingerprint(&entry.path).await }).await
                else {
                    continue;
                };
                let updated = this.update(cx, |app, cx| {
                    if known.is_empty() {
                        app.state.update_view(id, |v| v.fingerprint = fingerprint);
                    } else if known != fingerprint {
                        app.load(id, cx);
                        cx.notify();
                    }
                });
                if updated.is_err() {
                    return;
                }
            }
        })
    }

    /// Runs `op` in repository `id` with its profile; reloads afterwards.
    pub fn op(&mut self, id: RepoId, op: RepoOp, cx: &mut Context<Self>) {
        let Some(entry) = self.state.entry(id) else {
            return;
        };
        if self.state.is_busy(id) {
            self.notify(
                "Another operation is still running in this repository",
                true,
                cx,
            );
            return;
        }
        let profile = self.state.profile_of(&entry);
        let cancel = op.is_network().then(CancellationToken::new);
        self.state.busy.insert(
            id,
            Busy {
                label: op.label(),
                cancel: cancel.clone(),
            },
        );
        let services = self.services.clone();
        let o = op.clone();
        self.spawn_run(
            cx,
            async move { services.run_op(&entry, profile.as_ref(), &o, cancel).await },
            move |app, result, cx| {
                app.state.busy.remove(&id);
                match result {
                    Ok(summary) => {
                        // Undoing a commit offers its message again.
                        if let RepoOp::Undo(Undo::Commit {
                            summary,
                            amend: false,
                            ..
                        }) = &op
                        {
                            app.state.update_view(id, |v| {
                                if v.summary.trim().is_empty() {
                                    v.summary.clone_from(summary);
                                    v.draft_rev += 1;
                                }
                            });
                        }
                        if let RepoOp::Commit { .. } = op {
                            app.state.update_view(id, |v| {
                                v.summary.clear();
                                v.description.clear();
                                v.amend = false;
                                v.draft_rev += 1;
                            });
                        }
                        if op.is_network()
                            || matches!(
                                op,
                                RepoOp::Commit { .. }
                                    | RepoOp::Merge { .. }
                                    | RepoOp::Rebase { .. }
                                    | RepoOp::Undo(_)
                            )
                        {
                            let msg = summary
                                .filter(|s| s.len() < 160)
                                .unwrap_or_else(|| format!("{} — done", op.label()));
                            app.notify(msg, false, cx);
                        }
                    }
                    Err(e) if e == "cancelled" => {
                        app.notify(format!("{} cancelled", op.label()), false, cx);
                    }
                    Err(e) => app.notify(e, true, cx),
                }
                app.load(id, cx);
            },
        );
        cx.notify();
    }

    /// Runs `op` in the active repository.
    pub fn active_op(&mut self, op: RepoOp, cx: &mut Context<Self>) {
        if let Some(id) = self.state.active {
            self.op(id, op, cx);
        }
    }

    pub fn cancel(&mut self, id: RepoId) {
        if let Some(token) = self.state.busy.get(&id).and_then(|b| b.cancel.clone()) {
            token.cancel();
        }
    }

    /// Pull, then push once the pull has finished.
    pub fn sync(&mut self, id: RepoId, cx: &mut Context<Self>) {
        let mode = self.state.settings.pull_mode;
        self.op(id, RepoOp::Pull(mode), cx);
        cx.spawn(async move |this, cx| {
            loop {
                match this.read_with(cx, |app, _| app.state.is_busy(id)) {
                    Ok(true) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(150))
                            .await;
                    }
                    Ok(false) => break,
                    Err(_) => return,
                }
            }
            if let Err(error) = this.update(cx, |app, cx| {
                app.op(
                    id,
                    RepoOp::Push {
                        remote: None,
                        force_with_lease: false,
                    },
                    cx,
                );
            }) {
                tracing::debug!(error = %error, "view closed during sync");
            }
        })
        .detach();
    }

    /// User-initiated selection: recorded for Prev / Next.
    pub fn navigate(&mut self, id: RepoId, sel: Selection, cx: &mut Context<Self>) {
        let current = self.state.view(id).selection;
        if current != sel {
            self.state.update_view(id, |v| {
                v.back.push(current);
                if v.back.len() > 200 {
                    v.back.remove(0);
                }
                v.forward.clear();
            });
        }
        self.select(id, sel, cx);
    }

    /// Toolbar Prev (`back = true`) / Next.
    pub fn go(&mut self, id: RepoId, back: bool, cx: &mut Context<Self>) {
        let mut target = None;
        self.state.update_view(id, |v| {
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
            self.select(id, s, cx);
        }
    }

    /// Switches the workspace page, picking a sensible selection for it.
    pub fn set_page(&mut self, id: RepoId, page: Page, cx: &mut Context<Self>) {
        if page == Page::PullRequests {
            self.open_pull_requests(id, cx);
            return;
        }
        let view = self.state.view(id);
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
            Page::PullRequests => view.selection.clone(),
        };
        self.state.update_view(id, |v| v.page = page);
        let has_stash = view
            .data
            .as_ref()
            .is_some_and(|d| !d.snapshot.stashes.is_empty());
        if page == Page::Stashes && !has_stash {
            self.state.update_view(id, |v| v.details = None);
            cx.notify();
            return;
        }
        self.navigate(id, sel, cx);
    }

    pub fn select(&mut self, id: RepoId, sel: Selection, cx: &mut Context<Self>) {
        let Some(entry) = self.state.entry(id) else {
            return;
        };
        let view = self.state.view(id);
        self.state.update_view(id, |v| {
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
        cx.notify();
        let stash = match &sel {
            Selection::Wip => return,
            Selection::Commit(_) => None,
            Selection::Stash(i) => view
                .data
                .as_ref()
                .and_then(|d| d.snapshot.stashes.get(*i).cloned()),
        };
        let services = self.services.clone();
        let s = sel.clone();
        self.spawn_run(
            cx,
            async move {
                match (&s, stash) {
                    (Selection::Stash(_), Some(st)) => {
                        services.stash_details(&entry.path, &st).await
                    }
                    (Selection::Commit(oid), _) => services.commit_details(&entry.path, oid).await,
                    _ => Err(ferrumgit_application::AppError::Invalid(
                        "stash no longer exists".into(),
                    )),
                }
            },
            move |app, result, cx| match result {
                Ok(d) => app.state.update_view(id, |v| {
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
                Err(e) => app.notify(e, true, cx),
            },
        );
    }

    /// Selects a commit and scrolls the graph to it.
    pub fn reveal_commit(&mut self, id: RepoId, oid: &Oid, cx: &mut Context<Self>) {
        let row = self
            .state
            .view(id)
            .data
            .as_ref()
            .and_then(|d| d.snapshot.commits.iter().position(|c| &c.oid == oid));
        match row {
            Some(r) => {
                self.state.update_view(id, |v| v.scroll_to = Some(r));
                self.navigate(id, Selection::Commit(oid.clone()), cx);
            }
            None => self.notify("That commit is outside the loaded history", true, cx),
        }
    }

    /// Opens the diff of a working-copy file.
    pub fn open_file(
        &mut self,
        id: RepoId,
        file: &FileStatus,
        staged: bool,
        cx: &mut Context<Self>,
    ) {
        let source = if staged {
            DiffSource::Staged
        } else if file.worktree == ferrumgit_domain::ChangeKind::Untracked {
            DiffSource::Untracked
        } else {
            DiffSource::Unstaged
        };
        self.fetch_diff(
            id,
            file.path.clone(),
            file.orig_path.clone(),
            source,
            true,
            cx,
        );
    }

    fn fetch_diff(
        &mut self,
        id: RepoId,
        path: String,
        orig: Option<String>,
        source: DiffSource,
        notify_empty: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.state.entry(id) else {
            return;
        };
        let services = self.services.clone();
        let (o, s) = (orig.clone(), source.clone());
        self.spawn_run(
            cx,
            async move {
                services
                    .file_diff(&entry.path, &path, o.as_deref(), &s)
                    .await
            },
            move |app, result, cx| match result {
                Ok(Some(file)) => app.state.update_view(id, |v| {
                    v.diff = Some(DiffView {
                        file: Arc::new(file),
                        source,
                        orig,
                        lines: Default::default(),
                    });
                }),
                Ok(None) => {
                    app.state.update_view(id, |v| v.diff = None);
                    if notify_empty {
                        app.notify("No textual changes (mode change or empty file)", false, cx);
                    }
                }
                Err(e) => app.notify(e, true, cx),
            },
        );
    }

    /// After a reload, re-reads the open working-copy diff (or closes it).
    fn refresh_diff(&mut self, id: RepoId, cx: &mut Context<Self>) {
        let Some(d) = self.state.view(id).diff else {
            return;
        };
        if d.source == DiffSource::Commit {
            return;
        }
        self.fetch_diff(id, d.file.path.clone(), d.orig.clone(), d.source, false, cx);
    }

    /// Stages / unstages / discards one hunk, or only the selected lines in it.
    pub fn hunk(
        &mut self,
        id: RepoId,
        hunk: usize,
        action: HunkAction,
        only_selected: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(d) = self.state.view(id).diff else {
            return;
        };
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
            self.notify("Select changed (+/−) lines first", true, cx);
            return;
        };
        self.state.update_view(id, |v| {
            if let Some(d) = v.diff.as_mut() {
                d.lines.clear();
            }
        });
        self.op(
            id,
            RepoOp::ApplyPatch {
                patch: p,
                cached,
                reverse,
            },
            cx,
        );
    }

    /// Loads the last commit message into the draft when "Amend" is ticked.
    pub fn toggle_amend(&mut self, id: RepoId, on: bool, cx: &mut Context<Self>) {
        self.state.update_view(id, |v| v.amend = on);
        cx.notify();
        let Some(entry) = self.state.entry(id) else {
            return;
        };
        if !on || !self.state.view(id).summary.trim().is_empty() {
            return;
        }
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.head_message(&entry.path).await },
            move |app, result, _| {
                if let Ok(msg) = result {
                    let (summary, body) = msg.split_once('\n').unwrap_or((&msg, ""));
                    let (summary, body) = (summary.to_owned(), body.trim().to_owned());
                    app.state.update_view(id, |v| {
                        v.summary = summary;
                        v.description = body;
                        v.draft_rev += 1;
                    });
                }
            },
        );
    }

    /// Commits the draft of repository `id`.
    pub fn commit(&mut self, id: RepoId, cx: &mut Context<Self>) {
        let v = self.state.view(id);
        let msg = if v.description.trim().is_empty() {
            v.summary.trim().to_owned()
        } else {
            format!("{}\n\n{}", v.summary.trim(), v.description.trim())
        };
        self.op(
            id,
            RepoOp::Commit {
                message: msg,
                amend: v.amend,
            },
            cx,
        );
    }
}
