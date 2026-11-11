//! Logs in the workspace: the Logs page (Editor | Git | CI | Logs switch,
//! ⌥⌘L), `logs:*` commands, and keeping the page on the editor's project.
//! The page is created the first time it opens; its sources keep running
//! while the editor shows something else.

use ferrum_ui::prelude::*;

use super::Workspace;
use super::git::ViewMode;
use crate::logs::{LogsEvent, LogsView, Options};
use crate::registry::CommandRegistry;

pub(super) fn register(r: &mut CommandRegistry) {
    r.register("logs:toggle-view", |w, window, cx| {
        w.toggle_logs_view(window, cx)
    });
    r.register("logs:add-source", |w, window, cx| {
        if w.mode != ViewMode::Logs {
            w.set_mode(ViewMode::Logs, window, cx);
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

    /// Entering the Logs page: point it at the project and focus it.
    pub(super) fn show_logs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let logs = self.ensure_logs(window, cx);
        let root = self.root.clone();
        logs.update(cx, |logs, cx| logs.set_project(root, cx));
        logs.read(cx).focus_handle(cx).focus(window, cx);
    }

    /// The editor opened another folder: an open Logs page follows it.
    pub(super) fn sync_logs_project(&mut self, cx: &mut Context<Self>) {
        if self.mode != ViewMode::Logs {
            return;
        }
        if let Some(logs) = self.logs.clone() {
            let root = self.root.clone();
            logs.update(cx, |logs, cx| logs.set_project(root, cx));
        }
    }

    pub(super) fn toggle_logs_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = if self.mode == ViewMode::Logs {
            ViewMode::Editor
        } else {
            ViewMode::Logs
        };
        self.set_mode(next, window, cx);
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
        self.mode == ViewMode::Logs
    }
}
