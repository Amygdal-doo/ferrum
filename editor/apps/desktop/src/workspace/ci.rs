//! CI in the workspace: the CI pane (⇧⌘I, View menu, palette), `ci:*`
//! commands, and keeping the pane on the editor's project. The view and
//! its background runtime are created the first time it opens.

use ferrum_ui::prelude::*;
use ferrumeditor_core::tool_layout::ToolKind;

use super::Workspace;
use crate::ci::{CiEvent, CiView, Services};
use crate::registry::CommandRegistry;

pub(super) fn register(r: &mut CommandRegistry) {
    r.register("ci:toggle-view", |w, window, cx| {
        w.toggle_tool_focus(ToolKind::Ci, window, cx)
    });
    r.register("ci:refresh", |w, window, cx| {
        if !w.tool_open(ToolKind::Ci) {
            w.open_tool(ToolKind::Ci, window, cx);
        }
        if let Some(ci) = w.ci.clone() {
            ci.update(cx, |ci, cx| ci.refresh_all(cx));
        }
    });
    r.register("ci:search-log", |w, window, cx| {
        let Some(ci) = w.ci.clone().filter(|_| w.tool_open(ToolKind::Ci)) else {
            w.info("Open a job log in the CI pane to search it (⇧⌘I)", cx);
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

    /// The CI pane is opening: its view exists and shows the project.
    /// False when CI could not start.
    pub(super) fn show_ci(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(ci) = self.ensure_ci(window, cx) else {
            return false;
        };
        let root = self.root.clone();
        ci.update(cx, |ci, cx| ci.set_project(root, cx));
        true
    }

    /// The editor opened another folder: the CI view follows it.
    pub(super) fn sync_ci_project(&mut self, cx: &mut Context<Self>) {
        if let Some(ci) = self.ci.clone() {
            let root = self.root.clone();
            ci.update(cx, |ci, cx| ci.set_project(root, cx));
        }
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
        self.tool_open(ToolKind::Ci)
    }
}
