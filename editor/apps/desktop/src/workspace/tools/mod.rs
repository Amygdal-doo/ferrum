//! Tool panes inside the editor window: Git (FerrumGIT's `GitView`), Agent
//! (FerrumChat's `ChatView`), Term (FerrumTerm's `TermView`), CI, Logs and
//! Debug. They sit in three docks around the editor — left, right, bottom —
//! per [`ToolLayout`]; several can be open at once, and each can be moved,
//! resized and closed. The layout is remembered per project.

mod events;
mod render;
mod views;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use ferrum_ui::prelude::*;
use ferrumagent_ui::ChatView;
use ferrumeditor_core::tool_layout::{Dock, ToolKind, ToolLayout};
use ferrumgit_ui::GitView;
use ferrumterm_ui::{CommandId, TermView, TermViewOptions};

use super::Workspace;

pub(super) use events::RunningTask;
pub(super) use render::external_app;
pub use render::tool_icon;

/// The tool panes' state: their layout and their views (created the first
/// time each opens; closing a pane keeps its view, so a terminal keeps
/// running and a chat keeps its thread).
pub struct Tools {
    pub layout: ToolLayout,
    /// Layouts of the other projects (from the session, and this run).
    pub saved: BTreeMap<String, ToolLayout>,
    pub git: Option<Entity<GitView>>,
    pub chat: Option<Entity<ChatView>>,
    /// The project the chat was opened on.
    chat_root: Option<PathBuf>,
    pub term: Option<Entity<TermView>>,
    /// Whether the Term pane creates a real terminal view (tests that
    /// don't need one skip its server threads).
    pub real_term: bool,
    /// Tasks running in the terminal, by command.
    pub(super) tasks: HashMap<CommandId, RunningTask>,
    /// How the terminal is created (tests use a private server).
    pub term_options: TermViewOptions,
    /// Chat configuration per project (tests isolate it; `None` loads the
    /// user's).
    pub chat_config: Option<ChatConfig>,
    /// Commands recorded instead of run (tests).
    pub dry_run: Option<Vec<String>>,
    next_dry_id: u64,
    /// A pane header is being dragged (shows the edge drop zones).
    pub(super) dragging: bool,
    subscriptions: Vec<Subscription>,
}

impl Tools {
    pub fn new() -> Self {
        let mut layout = ToolLayout::default();
        layout.normalize();
        Self {
            layout,
            saved: BTreeMap::new(),
            git: None,
            chat: None,
            chat_root: None,
            term: None,
            real_term: !cfg!(test),
            tasks: HashMap::new(),
            // Tests: a private terminal server, commands recorded instead of
            // run, and a chat that never reads the user's settings or keys.
            term_options: TermViewOptions::embedded().attach_to_server(!cfg!(test)),
            chat_config: test_chat_config(),
            dry_run: cfg!(test).then(Vec::new),
            next_dry_id: 1_000_000,
            dragging: false,
            subscriptions: Vec::new(),
        }
    }
}

type ChatConfig = Box<dyn Fn(&std::path::Path) -> ferrumagent_ui::core::session::SessionConfig>;

#[cfg(test)]
fn test_chat_config() -> Option<ChatConfig> {
    Some(Box::new(|root| {
        let home =
            std::env::temp_dir().join(format!("ferrumeditor-test-agent-{}", std::process::id()));
        ferrumagent_ui::core::session::SessionConfig::isolated(root, &home)
    }))
}

#[cfg(not(test))]
fn test_chat_config() -> Option<ChatConfig> {
    None
}

impl Workspace {
    pub fn tool_open(&self, kind: ToolKind) -> bool {
        self.tools.layout.is_open(kind)
    }

    /// The top-bar buttons: open (and focus) or close the pane.
    pub fn toggle_tool(&mut self, kind: ToolKind, window: &mut Window, cx: &mut Context<Self>) {
        if self.tool_open(kind) {
            self.close_tool(kind, window, cx);
        } else {
            self.open_tool(kind, window, cx);
        }
    }

    /// The shortcuts: closed → open and focus; open but elsewhere → focus;
    /// focused → close and return to the editor.
    pub fn toggle_tool_focus(
        &mut self,
        kind: ToolKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.tool_open(kind) {
            self.open_tool(kind, window, cx);
        } else if self.tool_focused(kind, window, cx) {
            self.close_tool(kind, window, cx);
        } else {
            self.focus_tool(kind, window, cx);
        }
    }

    pub fn open_tool(&mut self, kind: ToolKind, window: &mut Window, cx: &mut Context<Self>) {
        self.tools.layout.set_open(kind, true);
        if !self.ensure_tool_view(kind, window, cx) {
            self.tools.layout.set_open(kind, false);
            return;
        }
        self.focus_tool(kind, window, cx);
        cx.notify();
    }

    /// Opens the pane without taking focus from the editor.
    pub fn show_tool(&mut self, kind: ToolKind, window: &mut Window, cx: &mut Context<Self>) {
        if self.tool_open(kind) && self.tool_view_exists(kind) {
            return;
        }
        self.tools.layout.set_open(kind, true);
        if !self.ensure_tool_view(kind, window, cx) {
            self.tools.layout.set_open(kind, false);
        }
        cx.notify();
    }

    pub fn close_tool(&mut self, kind: ToolKind, window: &mut Window, cx: &mut Context<Self>) {
        let had_focus = self.tool_focused(kind, window, cx);
        self.tools.layout.set_open(kind, false);
        if had_focus {
            self.focus_active(window, cx);
        }
        cx.notify();
    }

    pub fn move_tool(&mut self, kind: ToolKind, dock: Dock, cx: &mut Context<Self>) {
        self.tools.layout.move_to(kind, dock);
        cx.notify();
    }

    /// Drops `kind` onto `target`'s pane: they share its place.
    pub fn move_tool_next_to(
        &mut self,
        kind: ToolKind,
        target: ToolKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tools.layout.move_next_to(kind, target, true);
        self.ensure_tool_view(kind, window, cx);
        cx.notify();
    }

    fn tool_view_exists(&self, kind: ToolKind) -> bool {
        match kind {
            ToolKind::Git => self.tools.git.is_some(),
            ToolKind::Agent => self.tools.chat.is_some() || self.root.is_none(),
            ToolKind::Term => self.tools.term.is_some(),
            ToolKind::Ci => self.ci.is_some(),
            ToolKind::Logs => self.logs.is_some(),
            ToolKind::Debug => true,
        }
    }

    fn tool_focus_handle(&self, kind: ToolKind, cx: &App) -> Option<FocusHandle> {
        Some(match kind {
            ToolKind::Git => self.tools.git.as_ref()?.read(cx).focus_handle(cx),
            ToolKind::Agent => self.tools.chat.as_ref()?.read(cx).focus_handle(cx),
            ToolKind::Term => self.tools.term.as_ref()?.read(cx).focus_handle(cx),
            ToolKind::Ci => self.ci.as_ref()?.read(cx).focus_handle(cx),
            ToolKind::Logs => self.logs.as_ref()?.read(cx).focus_handle(cx),
            ToolKind::Debug => return None,
        })
    }

    pub fn tool_focused(&self, kind: ToolKind, window: &Window, cx: &App) -> bool {
        self.tool_focus_handle(kind, cx)
            .is_some_and(|h| h.contains_focused(window, cx))
    }

    pub(super) fn focus_tool(
        &mut self,
        kind: ToolKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if kind == ToolKind::Term
            && let Some(term) = self.tools.term.clone()
        {
            term.update(cx, |t, cx| t.focus_active(window, cx));
        } else if let Some(handle) = self.tool_focus_handle(kind, cx) {
            window.focus(&handle, cx);
        }
    }

    pub(super) fn close_all_tools(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for kind in self.tools.layout.open_tools() {
            self.tools.layout.set_open(kind, false);
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    pub(super) fn reset_tool_layout(&mut self, cx: &mut Context<Self>) {
        let open = self.tools.layout.open_tools();
        let mut layout = ToolLayout::default();
        layout.normalize();
        for kind in open {
            layout.set_open(kind, true);
        }
        self.tools.layout = layout;
        cx.notify();
    }
}

#[cfg(test)]
impl Workspace {
    pub fn tool_layout(&self) -> &ToolLayout {
        &self.tools.layout
    }

    pub fn tools_mut(&mut self) -> &mut Tools {
        &mut self.tools
    }

    pub fn tools(&self) -> &Tools {
        &self.tools
    }
}
