//! The three-way conflict view: open a conflicted file, choose a side per
//! conflict, finish in the editor, mark resolved.

use crate::app::GitView;
use crate::state::ConflictView;
use ferrumgit_domain::{ConflictChoice, RepoId, RepoOp};
use gpui::{Context, Window};

impl GitView {
    /// Opens the three-way view of conflicted `path` (closes the diff).
    pub fn open_conflict(&mut self, id: RepoId, path: String, cx: &mut Context<Self>) {
        let Some(entry) = self.state.entry(id) else {
            return;
        };
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.conflict_file(&entry.path, &path).await },
            move |app, result, cx| match result {
                Ok(file) => app.state.update_view(id, |v| {
                    v.diff = None;
                    v.conflict = Some(ConflictView::new(file));
                }),
                Err(e) => app.notify(e, true, cx),
            },
        );
    }

    pub fn close_conflict(&mut self, id: RepoId, cx: &mut Context<Self>) {
        self.state.update_view(id, |v| v.conflict = None);
        cx.notify();
    }

    /// Resolves conflict `index` with `choice`.
    pub fn choose(
        &mut self,
        id: RepoId,
        index: usize,
        choice: ConflictChoice,
        cx: &mut Context<Self>,
    ) {
        self.state.update_view(id, |v| {
            if let Some(slot) = v.conflict.as_mut().and_then(|c| c.choices.get_mut(index)) {
                *slot = choice;
            }
        });
        cx.notify();
    }

    /// Writes the resolved file and stages it.
    pub fn mark_conflict_resolved(&mut self, id: RepoId, cx: &mut Context<Self>) {
        let Some(c) = self.state.view(id).conflict else {
            return;
        };
        let Some(content) = c.file.result(&c.choices) else {
            self.notify("Choose a side for every conflict first", true, cx);
            return;
        };
        let path = c.file.path.clone();
        self.state.update_view(id, |v| v.conflict = None);
        self.op(id, RepoOp::Resolve { path, content }, cx);
    }

    /// Writes the choices made so far (open conflicts keep their markers)
    /// and opens the file in the editor to finish by hand.
    pub fn edit_conflict(&mut self, id: RepoId, window: &Window, cx: &mut Context<Self>) {
        let (Some(c), Some(entry)) = (self.state.view(id).conflict, self.state.entry(id)) else {
            return;
        };
        let path = c.file.path.clone();
        let draft = c
            .choices
            .iter()
            .any(|ch| ch.is_resolved())
            .then(|| c.file.draft(&c.choices));
        let services = self.services.clone();
        cx.spawn_in(window, async move |this, cx| {
            let p = path.clone();
            let written = match draft {
                Some(text) => {
                    crate::commands::run(&services.clone(), async move {
                        services.write_conflict_draft(&entry.path, &p, &text).await
                    })
                    .await
                }
                None => Ok(()),
            };
            let done = this.update_in(cx, |app, window, cx| match written {
                Ok(()) => {
                    app.state.update_view(id, |v| v.conflict = None);
                    app.edit_file(id, &path, None, window, cx);
                }
                Err(e) => app.notify(e, true, cx),
            });
            if let Err(error) = done {
                tracing::debug!(%error, "view closed before the draft was written");
            }
        })
        .detach();
    }

    /// After a reload, closes the view once its file is no longer in conflict.
    pub(crate) fn refresh_conflict(&mut self, id: RepoId) {
        let view = self.state.view(id);
        let (Some(c), Some(data)) = (view.conflict, view.data) else {
            return;
        };
        let still = data
            .snapshot
            .status
            .conflicted()
            .iter()
            .any(|f| f.path == c.file.path);
        if !still {
            self.state.update_view(id, |v| v.conflict = None);
        }
    }
}
