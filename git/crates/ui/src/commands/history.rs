//! File history / blame, commit search and undo.

use crate::app::GitView;
use crate::state::{Dialog, FileTab, FileView, SearchHits};
use ferrumgit_domain::search::{CommitQuery, step_match};
use ferrumgit_domain::{Oid, RepoId, RepoOp};
use gpui::{Context, Window};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

/// Commits listed in a file's history at most.
const FILE_HISTORY_LIMIT: usize = 1000;
/// Typing pause before message bodies are searched with git.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);

impl GitView {
    /// Opens the file panel for `path` at `rev` (`None` = HEAD / working tree).
    pub fn open_file_view(
        &mut self,
        id: RepoId,
        path: String,
        rev: Option<Oid>,
        tab: FileTab,
        cx: &mut Context<Self>,
    ) {
        self.state.update_view(id, |v| {
            v.file = Some(FileView {
                path,
                rev,
                tab,
                ..Default::default()
            });
        });
        self.load_file_tab(id, cx);
        cx.notify();
    }

    pub fn set_file_tab(&mut self, id: RepoId, tab: FileTab, cx: &mut Context<Self>) {
        self.state.update_view(id, |v| {
            if let Some(f) = v.file.as_mut() {
                f.tab = tab;
            }
        });
        self.load_file_tab(id, cx);
        cx.notify();
    }

    /// Fetches the shown tab's data if it is not loaded yet.
    fn load_file_tab(&mut self, id: RepoId, cx: &mut Context<Self>) {
        let (Some(entry), Some(file)) = (self.state.entry(id), self.state.view(id).file) else {
            return;
        };
        let services = self.services.clone();
        let (path, rev) = (file.path.clone(), file.rev.clone());
        // The panel may have moved on to another file / revision meanwhile.
        let same = move |f: &FileView| f.path == file.path && f.rev == file.rev;
        match file.tab {
            FileTab::History if file.history.is_none() => self.spawn_run(
                cx,
                async move {
                    services
                        .file_history(&entry.path, &path, rev.as_ref(), FILE_HISTORY_LIMIT)
                        .await
                },
                move |app, result, _| {
                    app.state.update_view(id, |v| {
                        if let Some(f) = v.file.as_mut().filter(|f| same(f)) {
                            match result {
                                Ok(h) => f.history = Some(Arc::new(h)),
                                Err(e) => f.error = Some(e),
                            }
                        }
                    });
                },
            ),
            FileTab::Blame if file.blame.is_none() => self.spawn_run(
                cx,
                async move { services.blame(&entry.path, &path, rev.as_ref()).await },
                move |app, result, _| {
                    app.state.update_view(id, |v| {
                        if let Some(f) = v.file.as_mut().filter(|f| same(f)) {
                            match result {
                                Ok(b) => f.blame = Some(Arc::new(b)),
                                Err(e) => f.error = Some(e),
                            }
                        }
                    });
                },
            ),
            _ => {}
        }
    }

    /// After a reload, re-reads a file panel that follows HEAD / the working tree.
    pub(crate) fn refresh_file_view(&mut self, id: RepoId, cx: &mut Context<Self>) {
        let follows_head = self.state.view(id).file.is_some_and(|f| f.rev.is_none());
        if follows_head {
            self.state.update_view(id, |v| {
                if let Some(f) = v.file.as_mut() {
                    f.history = None;
                    f.blame = None;
                    f.error = None;
                }
            });
            self.load_file_tab(id, cx);
        }
    }

    pub fn close_file_view(&mut self, id: RepoId, cx: &mut Context<Self>) {
        self.state.update_view(id, |v| v.file = None);
        cx.notify();
    }

    /// New search text: matches in summaries / authors / SHAs show at once;
    /// message bodies are searched by git after a short pause in typing.
    pub fn search_changed(&mut self, id: RepoId, text: String, cx: &mut Context<Self>) {
        self.state.update_view(id, |v| v.search = text.clone());
        let query = CommitQuery::new(&text);
        if query.is_empty() {
            self.state
                .update_view(id, |v| v.search_hits = SearchHits::default());
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            if let Err(error) = this.update(cx, |app, cx| {
                if CommitQuery::new(&app.state.view(id).search) == query {
                    app.grep_messages(id, query, cx);
                }
            }) {
                tracing::debug!(error = %error, "view closed during search");
            }
        })
        .detach();
    }

    /// `git log --grep` over the loaded history's refs for `query`.
    fn grep_messages(&mut self, id: RepoId, query: CommitQuery, cx: &mut Context<Self>) {
        let Some(entry) = self.state.entry(id) else {
            return;
        };
        let services = self.services.clone();
        let limit = self.state.log_limit();
        let needle = query.as_str().to_owned();
        self.spawn_run(
            cx,
            async move { services.grep_messages(&entry.path, &needle, limit).await },
            move |app, result, _| {
                let oids = match result {
                    Ok(oids) => oids,
                    Err(error) => {
                        tracing::debug!(error = %error, "message search failed");
                        return;
                    }
                };
                app.state.update_view(id, |v| {
                    if CommitQuery::new(&v.search) == query {
                        v.search_hits = SearchHits {
                            query: query.as_str().to_owned(),
                            oids: Arc::new(oids.into_iter().collect()),
                        };
                    }
                });
            },
        );
    }

    /// Rows of the loaded history matching the current search.
    #[must_use]
    pub fn search_matches(&self, id: RepoId) -> Vec<usize> {
        let view = self.state.view(id);
        let query = CommitQuery::new(&view.search);
        let Some(data) = view.data.as_ref() else {
            return Vec::new();
        };
        let empty = HashSet::new();
        let extra = if view.search_hits.query == query.as_str() {
            view.search_hits.oids.as_ref()
        } else {
            &empty
        };
        query.find(&data.snapshot.commits, extra)
    }

    /// Selects the next / previous search match and scrolls to it.
    pub fn search_step(&mut self, id: RepoId, forward: bool, cx: &mut Context<Self>) {
        let matches = self.search_matches(id);
        let view = self.state.view(id);
        let Some(data) = view.data.as_ref() else {
            return;
        };
        let current = match &view.selection {
            crate::state::Selection::Commit(oid) => {
                data.snapshot.commits.iter().position(|c| &c.oid == oid)
            }
            _ => None,
        };
        let Some(row) = step_match(&matches, current, forward) else {
            if !view.search.trim().is_empty() {
                self.notify("No matching commits in the loaded history", false, cx);
            }
            return;
        };
        if let Some(oid) = data.snapshot.commits.get(row).map(|c| c.oid.clone()) {
            self.reveal_commit(id, &oid, cx);
        }
    }

    /// Asks before undoing the last local operation.
    pub fn undo(&mut self, id: RepoId, window: &mut Window, cx: &mut Context<Self>) {
        let plan = self
            .state
            .view(id)
            .data
            .as_ref()
            .and_then(|d| d.snapshot.undo.clone());
        let Some(plan) = plan else {
            self.notify("Nothing to undo", false, cx);
            return;
        };
        self.open_dialog(
            Dialog::Confirm {
                title: format!("{}?", plan.label()),
                body: plan.description(),
                confirm: "Undo".into(),
                destructive: false,
                op: RepoOp::Undo(plan),
            },
            window,
            cx,
        );
    }
}
