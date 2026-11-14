//! The embedded views and their wiring: a chat per project, the git view
//! (kept while a project is open: it feeds the chat's branch line and runs
//! its git requests), the terminal (created on first show); chat →
//! terminal, chat → git, files → Ferrum Editor.

use std::path::{Path, PathBuf};

use ferrum_ui::prelude::*;
use ferrum_ui::project;
use ferrumagent_ui::{ChatEvent, ChatView, GitRequest, GitStatus};
use ferrumgit_ui::{GitView, GitViewEvent, GitViewOptions, RemoteCommand, RepoSummary};
use ferrumterm_ui::{TermView, TermViewOptions};

use crate::app::{ChatAction, ChatSlot, FerrumVibe};
use crate::layout::Column;

impl FerrumVibe {
    /// Creates the views the project and the shown panes need.
    pub(crate) fn ensure_views(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_chat(window, cx);
        self.ensure_git(window, cx);
        if self.layout.is_visible(Column::Term) {
            self.ensure_term(window, cx);
        }
    }

    /// The current project's chat, started on first use.
    pub(crate) fn ensure_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(root) = self.project.clone() {
            self.chat_for(&root, window, cx);
        }
    }

    /// `root`'s chat (its agent keeps running while another project is
    /// shown), started on first use.
    pub(crate) fn chat_for(
        &mut self,
        root: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ChatView> {
        if let Some(slot) = self.chats.get(root) {
            return slot.view.clone();
        }
        let mut options = (self.chat_options)(root);
        // The window draws the threads (sidebar) and the git line's data.
        options.shell = true;
        let chat = ChatView::new(root, options, window, cx);
        let project = root.to_path_buf();
        let events = cx.subscribe_in(&chat, window, {
            let project = project.clone();
            move |this, _, event, window, cx| this.on_chat_event(&project, event, window, cx)
        });
        if let Some(summary) = self.git_summary.clone().filter(|_| self.is_current(root)) {
            chat.update(cx, |chat, cx| {
                chat.set_git_status(Some(git_status(&summary)), cx)
            });
        }
        self.chats.insert(
            project,
            ChatSlot {
                view: chat.clone(),
                _subscriptions: vec![events],
            },
        );
        chat
    }

    fn is_current(&self, root: &Path) -> bool {
        self.project.as_deref() == Some(root)
    }

    /// Runs `action` in `root`'s chat (which waits for its session to
    /// open if needed).
    pub(crate) fn chat_action(
        &mut self,
        root: &Path,
        action: ChatAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chat = self.chat_for(root, window, cx);
        chat.update(cx, |chat, cx| match action {
            ChatAction::OpenThread(id) => chat.open_thread(&id, window, cx),
            ChatAction::StartTask(mode, prompt) => chat.start_task(mode, prompt, window, cx),
            ChatAction::Settings => chat.open_settings_page(window, cx),
        });
    }

    fn on_chat_event(
        &mut self,
        project: &Path,
        event: &ChatEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self.is_current(project);
        match event {
            ChatEvent::OpenFile { path, line } => self.open_file(path, *line, cx),
            ChatEvent::RunInTerminal { cmd, cwd } => {
                self.run_in_terminal(cmd, Some(cwd.clone()), window, cx);
            }
            ChatEvent::ChangesUpdated { files } if current => {
                self.pending_changes = *files;
                self.refresh_git(window, cx);
                cx.notify();
            }
            ChatEvent::ChangesUpdated { .. } => {}
            ChatEvent::ThreadsChanged => self.refresh_index(cx),
            ChatEvent::Git(request) if current => self.git_request(*request, window, cx),
            ChatEvent::Git(_) => {}
        }
    }

    /// A git action from the chat's footer, run by the git view.
    pub fn git_request(
        &mut self,
        request: GitRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_git(window, cx);
        let Some(git) = self.git.clone() else {
            self.show_notice(Notice::error("Open a project folder first."), cx);
            return;
        };
        let mut remote = |command| git.update(cx, |git, cx| git.remote(command, cx));
        match request {
            GitRequest::Fetch => remote(RemoteCommand::Fetch),
            GitRequest::Pull => remote(RemoteCommand::Pull),
            GitRequest::Push => remote(RemoteCommand::Push),
            GitRequest::Sync => remote(RemoteCommand::Sync),
            GitRequest::Commit => {
                self.set_visible(Column::Git, true, window, cx);
                git.update(cx, |git, cx| git.start_commit(window, cx));
            }
            GitRequest::SwitchBranch => {
                self.set_visible(Column::Git, true, window, cx);
                git.update(cx, |git, cx| git.show_branches(cx));
            }
        }
    }

    pub(crate) fn ensure_git(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.project.clone() else {
            return;
        };
        if self.git.is_some() {
            return;
        }
        let options = GitViewOptions::embedded().without_toolbar();
        let git = GitView::new(Some(root), options, window, cx);
        let events = cx.subscribe(&git, |this, _, event: &GitViewEvent, cx| match event {
            GitViewEvent::OpenFile { path, line } => this.open_file(path, *line, cx),
            GitViewEvent::CommitCreated { .. } | GitViewEvent::RepoChanged { .. } => {}
        });
        let status = cx.observe(&git, |this, git, cx| {
            let summary = git.read(cx).summary();
            this.git_summary_changed(summary, cx);
        });
        self.git_subscriptions = vec![events, status];
        self.git = Some(git);
    }

    /// Mirrors the head into the chat's footer; a failure while the Git
    /// pane is hidden shows as a notice.
    fn git_summary_changed(&mut self, summary: Option<RepoSummary>, cx: &mut Context<Self>) {
        if summary == self.git_summary {
            return;
        }
        let new_error = summary
            .as_ref()
            .and_then(|s| s.error.clone())
            .filter(|e| self.git_summary.as_ref().and_then(|s| s.error.as_ref()) != Some(e));
        self.git_summary = summary.clone();
        if let Some(chat) = self.chat() {
            chat.update(cx, |chat, cx| {
                chat.set_git_status(summary.as_ref().map(git_status), cx);
            });
        }
        if let Some(error) = new_error.filter(|_| !self.layout.is_visible(Column::Git)) {
            self.show_notice(Notice::error(error), cx);
        }
        cx.notify();
    }

    pub(crate) fn ensure_term(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.term.is_some() {
            return;
        }
        let mut options = TermViewOptions::embedded()
            .session_name("vibe")
            .attach_to_server(self.term_attach);
        if let Some(root) = &self.project {
            options = options
                .cwd(root.clone())
                .session_name(format!("vibe:{}", project::project_name(root)));
        }
        self.term = Some(TermView::new(options, window, cx));
    }

    /// "Run in terminal" from the chat: shows the terminal pane and runs
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

    /// The chat changed files: reload the git view now rather than at its
    /// next poll (it also feeds the chat's branch line).
    pub fn refresh_git(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(git) = &self.git {
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

/// The chat footer's view of the head.
pub(crate) fn git_status(summary: &RepoSummary) -> GitStatus {
    GitStatus {
        branch: summary
            .branch
            .clone()
            .unwrap_or_else(|| "detached HEAD".into()),
        ahead: summary.ahead,
        behind: summary.behind,
        dirty: summary.changed,
    }
}
