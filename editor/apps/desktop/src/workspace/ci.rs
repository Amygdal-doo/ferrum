//! CI in the workspace: the CI page (Editor | Git | CI | Logs switch, ⇧⌘I),
//! `ci:*` commands, and keeping the page on the editor's project. The
//! page and its background runtime are created the first time it opens.

use ferrum_ui::prelude::*;

use super::Workspace;
use super::git::ViewMode;
use crate::ci::{CiEvent, CiView, Services};
use crate::registry::CommandRegistry;

pub(super) fn register(r: &mut CommandRegistry) {
    r.register("ci:toggle-view", |w, window, cx| {
        w.toggle_ci_view(window, cx)
    });
    r.register("ci:refresh", |w, window, cx| {
        if w.mode != ViewMode::Ci {
            w.set_mode(ViewMode::Ci, window, cx);
        }
        if let Some(ci) = w.ci.clone() {
            ci.update(cx, |ci, cx| ci.refresh_all(cx));
        }
    });
    r.register("ci:search-log", |w, window, cx| {
        let Some(ci) = w.ci.clone().filter(|_| w.mode == ViewMode::Ci) else {
            w.info("Open a job log on the CI page to search it (⇧⌘I)", cx);
            return;
        };
        if !ci.update(cx, |ci, cx| ci.focus_log_search(window, cx)) {
            w.info("Open a finished job to search its log", cx);
        }
    });
}

impl Workspace {
    /// The CI page, created (with its runtime) on first use.
    fn ensure_ci(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Entity<CiView>> {
        if let Some(ci) = &self.ci {
            return Some(ci.clone());
        }
        let services = match self.ci_services.take() {
            Some(services) => services,
            None => match Services::start() {
                Ok(services) => services,
                Err(error) => {
                    self.error(format!("Could not start CI: {error}"), cx);
                    return None;
                }
            },
        };
        let ci = cx.new(|cx| CiView::new(services, window, cx));
        let subscription = cx.subscribe_in(&ci, window, |this, _, event, window, cx| {
            let CiEvent::OpenProject(path) = event;
            this.set_root(path.clone(), window, cx);
        });
        self._subscriptions.push(subscription);
        self.ci = Some(ci.clone());
        Some(ci)
    }

    /// Entering the CI page: point it at the project and focus it.
    pub(super) fn show_ci(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ci) = self.ensure_ci(window, cx) else {
            self.mode = ViewMode::Editor;
            return;
        };
        let root = self.root.clone();
        ci.update(cx, |ci, cx| ci.set_project(root, cx));
        ci.read(cx).focus_handle(cx).focus(window, cx);
    }

    /// The editor opened another folder: an open CI page follows it.
    pub(super) fn sync_ci_project(&mut self, cx: &mut Context<Self>) {
        if self.mode != ViewMode::Ci {
            return;
        }
        if let Some(ci) = self.ci.clone() {
            let root = self.root.clone();
            ci.update(cx, |ci, cx| ci.set_project(root, cx));
        }
    }

    pub(super) fn toggle_ci_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = if self.mode == ViewMode::Ci {
            ViewMode::Editor
        } else {
            ViewMode::Ci
        };
        self.set_mode(next, window, cx);
    }
}

#[cfg(test)]
impl Workspace {
    /// Services (fake HTTP and tokens) for the CI page tests open.
    pub fn set_ci_services(&mut self, services: Services) {
        self.ci_services = Some(services);
    }

    pub fn ci(&self) -> Option<&Entity<CiView>> {
        self.ci.as_ref()
    }

    pub fn in_ci_view(&self) -> bool {
        self.mode == ViewMode::Ci
    }
}
