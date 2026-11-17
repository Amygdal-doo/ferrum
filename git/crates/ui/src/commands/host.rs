//! What a host app drives without the toolbar: the head's summary for its
//! status line, remote operations, the branch drawer, the commit form and
//! markers above the history.

use crate::app::GitView;
use crate::embed::{HistoryMarker, RemoteCommand, RepoSummary};
use crate::state::Page;
use ferrumgit_domain::RepoOp;
use gpui::{Context, Window};

impl GitView {
    /// The shown repository's branch, ahead / behind counts and how many
    /// files have uncommitted changes; `None` until it has loaded.
    #[must_use]
    pub fn summary(&self) -> Option<RepoSummary> {
        let id = self.state.active?;
        let view = self.state.views.get(&id)?;
        let status = &view.data.as_ref()?.snapshot.status;
        Some(RepoSummary {
            branch: status.branch.head.clone(),
            ahead: status.branch.ahead,
            behind: status.branch.behind,
            changed: status.files.len(),
            busy: self.state.busy.get(&id).map(|b| b.label.clone()),
            error: self
                .state
                .notice
                .as_ref()
                .filter(|n| n.error)
                .map(|n| n.message.clone()),
        })
    }

    /// Fetch, pull, push or sync the shown repository (with its profile,
    /// like the toolbar buttons).
    pub fn remote(&mut self, command: RemoteCommand, cx: &mut Context<Self>) {
        let Some(id) = self.state.active else {
            return;
        };
        let settings = &self.state.settings;
        let op = match command {
            RemoteCommand::Fetch => RepoOp::Fetch {
                remote: None,
                prune: settings.prune_on_fetch,
            },
            RemoteCommand::Pull => RepoOp::Pull(settings.pull_mode),
            RemoteCommand::Push => RepoOp::Push {
                remote: None,
                force_with_lease: false,
            },
            RemoteCommand::Sync => return self.sync(id, cx),
        };
        self.op(id, op, cx);
    }

    /// Opens the branch drawer (switch, create, merge from there).
    pub fn show_branches(&mut self, cx: &mut Context<Self>) {
        self.drawer_open = true;
        cx.notify();
    }

    /// Shows the working copy: uncommitted changes and their diffs. Also
    /// applies to a repository still being opened with [`Self::set_repo`].
    /// On a folder without git: the files changed since it is watched.
    pub fn show_changes(&mut self, cx: &mut Context<Self>) {
        self.show_page(Page::WorkingCopy, cx);
    }

    /// Shows the commit history (see [`Self::show_changes`]); on a folder
    /// without git, the repositories found below it.
    pub fn show_history(&mut self, cx: &mut Context<Self>) {
        self.show_page(Page::History, cx);
    }

    fn show_page(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.in_folder() {
            // A folder without git: its changes, or the repositories below.
            self.set_folder_page(page.into(), cx);
            return;
        }
        self.pending_page = Some(page);
        if let Some(id) = self.state.active
            && self.state.view(id).page != page
        {
            self.set_page(id, page, cx);
        }
        cx.notify();
    }

    /// Shows `markers` (newest first) under `title` above the history's
    /// commits, each with "Restore to here…" ([`crate::RestoreMarker`]).
    /// An empty list hides them.
    pub fn set_history_markers(
        &mut self,
        title: impl Into<gpui::SharedString>,
        markers: Vec<HistoryMarker>,
        cx: &mut Context<Self>,
    ) {
        let markers = Some((title.into(), markers)).filter(|(_, m)| !m.is_empty());
        if self.markers != markers {
            self.markers = markers;
            cx.notify();
        }
    }

    /// The markers shown above the history.
    #[must_use]
    pub fn history_markers(&self) -> &[HistoryMarker] {
        self.markers.as_ref().map_or(&[], |(_, m)| m.as_slice())
    }

    /// Shows the working copy and puts the cursor in the commit summary.
    pub fn start_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.state.active else {
            return;
        };
        if self.state.view(id).page != Page::WorkingCopy {
            self.set_page(id, Page::WorkingCopy, cx);
        }
        let summary = self.inputs.summary.clone();
        summary.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }
}
