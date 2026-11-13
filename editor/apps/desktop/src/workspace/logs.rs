//! Logs in the workspace: the Logs pane (⌥⌘L, View menu, palette),
//! `logs:*` commands, and keeping the pane on the editor's project. The
//! view is created the first time it opens; its sources keep running while
//! the pane is closed.

use ferrum_ui::prelude::*;
use ferrumeditor_core::tool_layout::ToolKind;

use super::Workspace;
use crate::logs::{LogsEvent, LogsView, Options};
use crate::registry::CommandRegistry;

pub(super) fn register(r: &mut CommandRegistry) {
    r.register("logs:toggle-view", |w, window, cx| {
        w.toggle_tool_focus(ToolKind::Logs, window, cx)
    });
    r.register("logs:add-source", |w, window, cx| {
        if !w.tool_open(ToolKind::Logs) {
            w.open_tool(ToolKind::Logs, window, cx);
        }
        if w.root.is_none() {
            w.info("Open a folder to add log sources to it (⌘⇧O)", cx);
            return;
        }
        if let Some(logs) = w.logs.clone() {
            logs.update(cx, |logs, cx| logs.open_dialog(None, window, cx));
        }
    });
    r.register("logs:stop-all", |w, _, cx| {
        if let Some(logs) = w.logs.clone() {
            logs.update(cx, |logs, cx| logs.stop_all(cx));
        }
    });
}

impl Workspace {
    /// The Logs page, created on first use.
    fn ensure_logs(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<LogsView> {
        if let Some(logs) = &self.logs {
            return logs.clone();
        }
        let options = self.logs_options.take().unwrap_or_else(Options::user);
        let logs = cx.new(|cx| LogsView::new(options, None, window, cx));
        let subscription = cx.subscribe_in(&logs, window, |this, _, event, window, cx| {
            let LogsEvent::OpenProject(path) = event;
            this.set_root(path.clone(), window, cx);
        });
        self._subscriptions.push(subscription);
        self.logs = Some(logs.clone());
        logs
    }

    /// The Logs pane is opening: its view exists and shows the project.
    pub(super) fn show_logs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let logs = self.ensure_logs(window, cx);
        let root = self.root.clone();
        logs.update(cx, |logs, cx| logs.set_project(root, cx));
    }

    /// The editor opened another folder: the Logs view follows it.
    pub(super) fn sync_logs_project(&mut self, cx: &mut Context<Self>) {
        if let Some(logs) = self.logs.clone() {
            let root = self.root.clone();
            logs.update(cx, |logs, cx| logs.set_project(root, cx));
        }
    }
}

#[cfg(test)]
impl Workspace {
    /// Options (temp store, `/bin/sh`) for the Logs page tests open.
    pub fn set_logs_options(&mut self, options: Options) {
        self.logs_options = Some(options);
    }

    pub fn logs(&self) -> Option<&Entity<LogsView>> {
        self.logs.as_ref()
    }

    pub fn in_logs_view(&self) -> bool {
        self.tool_open(ToolKind::Logs)
    }
}
