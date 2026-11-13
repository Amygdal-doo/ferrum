//! The three embedded views: created on first show, and wired together
//! (chat → terminal, chat → git, files → Ferrum Editor).

use std::path::{Path, PathBuf};

use ferrum_ui::prelude::*;
use ferrum_ui::project;
use ferrumagent_ui::{ChatEvent, ChatView};
use ferrumgit_ui::{GitView, GitViewEvent, GitViewOptions};
use ferrumterm_ui::{TermView, TermViewEvent, TermViewOptions};

use crate::app::FerrumAgentic;
use crate::layout::Column;

impl FerrumAgentic {
    /// Creates the views of shown columns that don't have one yet.
    pub(crate) fn ensure_views(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for column in self.layout.visible() {
            match column {
                Column::Chat => self.ensure_chat(window, cx),
                Column::Git => self.ensure_git(window, cx),
                Column::Term => self.ensure_term(window, cx),
            }
        }
    }

    pub(crate) fn ensure_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.project.clone() else {
            return;
        };
        if self.chat.is_some() {
            return;
        }
        let chat = ChatView::new(&root, (self.chat_options)(&root), window, cx);
        let sub = cx.subscribe_in(&chat, window, |this, _, event, window, cx| match event {
            ChatEvent::OpenFile { path, line } => this.open_file(path, *line, cx),
            ChatEvent::RunInTerminal { cmd, cwd } => {
                this.run_in_terminal(cmd, Some(cwd.clone()), window, cx);
            }
            ChatEvent::ChangesUpdated { files } => {
                this.pending_changes = *files;
                this.refresh_git(window, cx);
                cx.notify();
            }
        });
        self.view_subscriptions.insert(Column::Chat, sub);
        self.chat = Some(chat);
    }

    fn ensure_git(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.project.clone() else {
            return;
        };
        if self.git.is_some() {
            return;
        }
        let git = GitView::new(Some(root), GitViewOptions::embedded(), window, cx);
        let sub = cx.subscribe(&git, |this, _, event: &GitViewEvent, cx| match event {
            GitViewEvent::OpenFile { path, line } => this.open_file(path, *line, cx),
            GitViewEvent::CommitCreated { .. } => {
                this.project_bar
                    .update(cx, |bar, cx| bar.refresh_branch(cx));
            }
            GitViewEvent::RepoChanged { .. } => {}
        });
        self.view_subscriptions.insert(Column::Git, sub);
        self.git = Some(git);
    }

    fn ensure_term(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.term.is_some() {
            return;
        }
        let mut options = TermViewOptions::embedded()
            .session_name("agentic")
            .attach_to_server(self.term_attach);
        if let Some(root) = &self.project {
            options = options
                .cwd(root.clone())
                .session_name(format!("agentic:{}", project::project_name(root)));
        }
        let term = TermView::new(options, window, cx);
        let sub = cx.subscribe(&term, |this, _, event: &TermViewEvent, cx| {
            if let TermViewEvent::TitleChanged(title) = event {
                this.term_title = Some(title.clone().into());
                cx.notify();
            }
        });
        self.view_subscriptions.insert(Column::Term, sub);
        self.term = Some(term);
    }

    /// "Run in terminal" from the chat: shows the terminal column and runs
    /// `cmd` in a new tab there.
    pub fn run_in_terminal(
        &mut self,
        cmd: &str,
        cwd: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_visible(Column::Term, true, window, cx);
        self.ensure_term(window, cx);
        if let Some(term) = &self.term {
            term.update(cx, |term, cx| {
                term.run_command(cmd, cwd, cx);
                term.focus_active(window, cx);
            });
        }
    }

    /// The chat changed files: reload the git column now rather than at its
    /// next poll.
    pub fn refresh_git(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(git) = &self.git
            && self.layout.is_visible(Column::Git)
        {
            git.focus_handle(cx)
                .dispatch_action(&ferrumgit_ui::Reload, window, cx);
            self.git_refreshes += 1;
        }
    }

    pub(crate) fn open_file(&mut self, path: &Path, line: Option<u32>, cx: &mut Context<Self>) {
        if let Err(error) = (self.open_file)(self.project.as_deref(), path, line) {
            self.show_notice(Notice::error(error), cx);
        }
    }
}
