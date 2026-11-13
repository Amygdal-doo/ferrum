//! Creating the tool panes' views and keeping them on the project: the
//! first time a pane opens, after a restore, and when the folder changes
//! (each project has its own layout).

use std::collections::BTreeMap;
use std::path::PathBuf;

use ferrum_ui::prelude::*;
use ferrumagent_ui::{ChatOptions, ChatView};
use ferrumeditor_core::session::tools_key;
use ferrumeditor_core::tool_layout::{ToolKind, ToolLayout};
use ferrumgit_ui::{GitView, GitViewOptions};
use ferrumterm_ui::TermView;

use super::super::Workspace;

impl Workspace {
    /// Creates the view behind `kind` if needed. False when it can't open.
    pub(in crate::workspace) fn ensure_tool_view(
        &mut self,
        kind: ToolKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match kind {
            ToolKind::Git => {
                if self.tools.git.is_none() {
                    let git =
                        GitView::new(self.root.clone(), GitViewOptions::embedded(), window, cx);
                    let sub = cx.subscribe_in(&git, window, Self::on_git_view_event);
                    self.tools.subscriptions.push(sub);
                    self.tools.git = Some(git);
                }
            }
            ToolKind::Agent => self.ensure_chat(window, cx),
            ToolKind::Term => {
                if self.tools.real_term {
                    self.ensure_term(window, cx);
                }
            }
            ToolKind::Ci => return self.show_ci(window, cx),
            ToolKind::Logs => self.show_logs(window, cx),
            ToolKind::Debug => {}
        }
        true
    }

    /// The chat on the current project (a new one when the project changed).
    fn ensure_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        if self.tools.chat.is_some() && self.tools.chat_root.as_ref() == Some(&root) {
            return;
        }
        let config = self.tools.chat_config.as_ref().map(|make| make(&root));
        let options = ChatOptions {
            config,
            ..Default::default()
        };
        let chat = ChatView::new(&root, options, window, cx);
        let sub = cx.subscribe_in(&chat, window, Self::on_chat_event);
        self.tools.subscriptions.push(sub);
        self.tools.chat = Some(chat);
        self.tools.chat_root = Some(root);
    }

    pub(in crate::workspace) fn ensure_term(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TermView> {
        if let Some(term) = &self.tools.term {
            return term.clone();
        }
        let mut options = self.tools.term_options.clone();
        if let Some(root) = &self.root {
            options = options.cwd(root.clone());
            if options.session_name.is_none() {
                let name = root
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "project".into());
                options = options.session_name(format!("editor:{name}"));
            }
        }
        let term = TermView::new(options, window, cx);
        let sub = cx.subscribe_in(&term, window, Self::on_term_event);
        self.tools.subscriptions.push(sub);
        self.tools.term = Some(term.clone());
        term
    }

    /// Views of the open panes exist (after a restore or a project switch).
    pub(in crate::workspace) fn sync_tool_views(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for kind in self.tools.layout.open_tools() {
            if !self.ensure_tool_view(kind, window, cx) {
                self.tools.layout.set_open(kind, false);
            }
        }
    }

    /// The project folder changed: views follow it, and the new project's
    /// saved layout (if it has one) replaces the old one's.
    pub(in crate::workspace) fn tools_follow_root(
        &mut self,
        old: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let layout = self.tools.layout.clone();
        self.tools.saved.insert(tools_key(old.as_deref()), layout);
        self.load_tool_layout(cx);
        if let Some(root) = self.root.clone() {
            if let Some(git) = self.tools.git.clone() {
                git.update(cx, |g, cx| g.set_repo(&root, cx));
            }
            if let Some(term) = self.tools.term.clone() {
                term.update(cx, |t, cx| t.set_cwd(root.clone(), cx));
            }
            if self.tools.chat.is_some() {
                self.ensure_chat(window, cx);
            }
        }
        self.sync_tool_views(window, cx);
    }

    /// The saved layout of the current project; a project without one
    /// keeps the current layout.
    pub(in crate::workspace) fn load_tool_layout(&mut self, cx: &mut Context<Self>) {
        let key = tools_key(self.root.as_deref());
        if let Some(saved) = self.tools.saved.get(&key) {
            let mut layout = saved.clone();
            layout.normalize();
            self.tools.layout = layout;
        }
        cx.notify();
    }

    /// Every project's layout, for the session file.
    pub(in crate::workspace) fn tool_layouts(&self) -> BTreeMap<String, ToolLayout> {
        let mut all = self.tools.saved.clone();
        all.insert(tools_key(self.root.as_deref()), self.tools.layout.clone());
        all
    }
}
