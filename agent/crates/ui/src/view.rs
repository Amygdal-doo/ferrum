//! [`ChatView`]: state, the session event pump and the top-level layout.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ferrum_ui::prelude::*;
use ferrum_ui::{Notice, TextInput, TextInputEvent, render_notice};
use ferrumagent_core::context::{Mention, Suggestion};
use ferrumagent_core::modes::Mode;
use ferrumagent_core::permissions::{Decision, PermissionRequest};
use ferrumagent_core::review::FileChange;
use ferrumagent_core::session::{
    AgentChoice, ChatSession, McpServerStatus, SessionConfig, SessionEvent,
};
use ferrumagent_core::status::StatusBoard;
use ferrumagent_core::store::ThreadSummary;
use ferrumagent_core::thread::Thread;
use gpui::{
    Context, Entity, EventEmitter, FocusHandle, Focusable, ScrollHandle, Subscription, Task, Window,
};

use crate::actions::*;
use crate::host::{GitRequest, GitStatus};
use crate::settings::SettingsSheet;

/// What the host learns from the chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatEvent {
    /// Open `path` (absolute), at a 1-based line when known.
    OpenFile { path: PathBuf, line: Option<u32> },
    /// The user asked to run a command in the host's terminal.
    RunInTerminal { cmd: String, cwd: PathBuf },
    /// The agent's pending changes changed (`files` still to review).
    ChangesUpdated { files: usize },
    /// The project's saved threads changed: one was created, saved under
    /// a new title, renamed or deleted.
    ThreadsChanged,
    /// The footer's branch menu or Push button: the host runs it.
    Git(GitRequest),
    /// A thread's status (Needs you / Done / Working / Idle) changed: see
    /// [`ChatView::statuses`].
    StatusChanged,
    /// A notification of this chat was clicked: it now shows `thread_id`;
    /// the host brings the chat on screen (opens its pane or page).
    Reveal { thread_id: String },
}

/// How the view starts.
#[derive(Default)]
pub struct ChatOptions {
    /// The host draws the thread list and the window chrome: no thread
    /// panel or toggle here, and the header is just the thread's title
    /// and its ⋯ menu.
    pub shell: bool,
    /// The whole session configuration; `None` loads it from
    /// `~/.ferrum/agent` and the keychain.
    pub config: Option<SessionConfig>,
    /// `PATH` for external agents and MCP servers (GUI apps should pass
    /// the login shell's).
    pub path_env: Option<String>,
    /// Start in this mode.
    pub mode: Option<Mode>,
    /// Delivers "needs you" / "finished" notifications; `None`:
    /// [`crate::default_notifier`] (`ferrum_ui::notify`).
    pub notifier: Option<crate::Notifier>,
}

pub(crate) struct Picker {
    /// Byte offset of the `@` in the composer.
    pub start: usize,
    pub query: String,
    pub items: Vec<Suggestion>,
    pub selected: usize,
}

/// Something a shell host draws inside the chat's header row (see
/// [`ChatView::set_header_slots`]).
pub type HeaderSlot = std::rc::Rc<dyn Fn(&mut Window, &mut App) -> gpui::AnyElement>;

pub struct ChatView {
    pub(crate) project: PathBuf,
    pub(crate) shell: bool,
    /// Shell host: leave room for the traffic lights in the header row.
    pub(crate) header_lights: bool,
    /// Shell host's controls before the title and after the ⋯ menu.
    pub(crate) header_start: Option<HeaderSlot>,
    pub(crate) header_end: Option<HeaderSlot>,
    pub(crate) focus: FocusHandle,
    /// The shown session.
    pub(crate) session: Option<ChatSession>,
    /// Which listener's events are the shown session's.
    pub(crate) serial: u64,
    pub(crate) next_serial: u64,
    /// Sessions whose threads run while another is shown.
    pub(crate) background: Vec<crate::live::Background>,
    pub(crate) statuses: StatusBoard,
    /// The host shows the chat ([`ChatView::set_visible`]).
    pub(crate) visible: bool,
    pub(crate) window_active: bool,
    pub(crate) notifier: crate::Notifier,
    /// Why the session did not open (see `opening`).
    pub(crate) load_error: Option<String>,
    pub(crate) opener: crate::opening::Opener,
    /// The configuration the session opens with (kept for Try again).
    pub(crate) open_config: SessionConfig,
    /// Counts opens: only the latest one's answer is shown.
    pub(crate) open_attempt: u64,
    pub(crate) thread: Thread,
    pub(crate) busy: bool,
    pub(crate) queue: Vec<String>,
    pub(crate) permissions: Vec<PermissionRequest>,
    pub(crate) changes: Vec<FileChange>,
    pub(crate) can_undo_restore: bool,
    scanning: bool,
    rescan: bool,
    pub(crate) expanded_files: HashSet<PathBuf>,
    pub(crate) show_changes: bool,
    pub(crate) show_threads: bool,
    pub(crate) threads: Vec<ThreadSummary>,
    pub(crate) settings: Option<SettingsSheet>,
    pub(crate) expanded_tools: HashSet<String>,
    pub(crate) expanded_thinking: HashSet<(u64, usize)>,
    pub(crate) composer: Entity<TextInput>,
    pub(crate) mentions: Vec<Mention>,
    pub(crate) picker: Option<Picker>,
    picker_task: Option<Task<()>>,
    pub(crate) notice: Option<Notice>,
    notice_task: Option<Task<()>>,
    pub(crate) scroll: ScrollHandle,
    pub(crate) follow: bool,
    pub(crate) mcp_status: Vec<McpServerStatus>,
    /// Branch info pushed by the host, shown in the footer.
    pub(crate) git_status: Option<GitStatus>,
    /// The header's title field while renaming.
    pub(crate) renaming: Option<(Entity<TextInput>, Subscription)>,
    /// A host call waiting for the session to open.
    pub(crate) pending: Option<crate::host::HostRequest>,
    /// A started task's prompt, sent when its thread is switched to.
    pub(crate) send_after_switch: Option<String>,
    /// The inline change summary lists every file.
    pub(crate) all_changed_files: bool,
    /// The composer's placeholder as last set (empty thread or not).
    pub(crate) placeholder_empty: Option<bool>,
    pub(crate) tasks: Vec<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ChatEvent> for ChatView {}

impl Focusable for ChatView {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ChatView {
    /// The chat for `project_root`. The session (snapshot of the project)
    /// opens in the background.
    pub fn new(
        project_root: impl AsRef<Path>,
        options: ChatOptions,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> Entity<Self> {
        let project = project_root.as_ref().to_path_buf();
        cx.new(|cx| Self::build(project, options, window, cx))
    }

    pub(crate) fn build(
        project: PathBuf,
        options: ChatOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::build_with(
            project,
            options,
            crate::opening::default_opener(),
            window,
            cx,
        )
    }

    pub(crate) fn build_with(
        project: PathBuf,
        options: ChatOptions,
        opener: crate::opening::Opener,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let composer = cx.new(|cx| {
            TextInput::new(window, cx)
                .multiline(2)
                .borderless()
                .placeholder(crate::composer::placeholder(true))
        });
        let subscription =
            cx.subscribe_in(
                &composer,
                window,
                |this, input, event, window, cx| match event {
                    TextInputEvent::Changed => this.composer_changed(input.clone(), window, cx),
                    TextInputEvent::Submit => this.send(&Send, window, cx),
                    TextInputEvent::Blurred => {}
                },
            );
        let shell = options.shell;
        let options_notifier = options.notifier.unwrap_or_else(crate::default_notifier);
        let mut config = options
            .config
            .unwrap_or_else(|| SessionConfig::load(&project));
        if options.path_env.is_some() {
            config.path_env = options.path_env;
        }
        if let Some(mode) = options.mode {
            config.mode = mode;
        }
        let mut thread = Thread::new(project.clone(), "", config.mode);
        thread.title = "Starting…".into();
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            this.window_activated(window.is_window_active(), cx);
        });
        let mut this = Self {
            project,
            shell,
            header_lights: false,
            header_start: None,
            header_end: None,
            focus: cx.focus_handle(),
            session: None,
            serial: 0,
            next_serial: 0,
            background: Vec::new(),
            statuses: StatusBoard::default(),
            visible: true,
            window_active: window.is_window_active(),
            notifier: options_notifier,
            load_error: None,
            opener,
            open_config: config,
            open_attempt: 0,
            thread,
            busy: false,
            queue: Vec::new(),
            permissions: Vec::new(),
            changes: Vec::new(),
            can_undo_restore: false,
            scanning: false,
            rescan: false,
            expanded_files: HashSet::new(),
            show_changes: false,
            show_threads: false,
            threads: Vec::new(),
            settings: None,
            expanded_tools: HashSet::new(),
            expanded_thinking: HashSet::new(),
            composer,
            mentions: Vec::new(),
            picker: None,
            picker_task: None,
            notice: None,
            notice_task: None,
            scroll: ScrollHandle::new(),
            follow: true,
            mcp_status: Vec::new(),
            git_status: None,
            renaming: None,
            pending: None,
            send_after_switch: None,
            all_changed_files: false,
            placeholder_empty: Some(true),
            tasks: Vec::new(),
            _subscriptions: vec![subscription, activation],
        };
        this.open_session(cx);
        this
    }

    /// The session, once open.
    pub fn session(&self) -> Option<&ChatSession> {
        self.session.as_ref()
    }

    /// The view's copy of the current thread.
    pub fn thread(&self) -> &Thread {
        &self.thread
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn pending_changes(&self) -> &[FileChange] {
        &self.changes
    }

    pub fn open_permission_requests(&self) -> &[PermissionRequest] {
        &self.permissions
    }

    /// Adds context from the host (an editor selection, terminal output).
    pub fn add_mention(&mut self, mention: Mention, cx: &mut Context<Self>) {
        if !self.mentions.contains(&mention) {
            self.mentions.push(mention);
            cx.notify();
        }
    }

    /// Fills the composer (e.g. "Explain this").
    pub fn set_draft(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |input, cx| input.set_text(text, cx));
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
    }

    /// An event of the shown session (see `live::route`).
    pub(crate) fn on_session_event(&mut self, event: SessionEvent, cx: &mut Context<Self>) {
        match event {
            SessionEvent::Thread(event) => {
                self.thread.apply(&event);
                if self.follow {
                    self.scroll.scroll_to_bottom();
                }
            }
            SessionEvent::Permission(request) => {
                self.permissions.push(request);
                self.scroll.scroll_to_bottom();
            }
            SessionEvent::PermissionClosed(id) => self.permissions.retain(|p| p.id != id),
            SessionEvent::ChangesUpdated => self.refresh_changes(cx),
            SessionEvent::Busy(busy) => {
                self.busy = busy;
                if !busy {
                    self.permissions.clear();
                }
            }
            SessionEvent::Queue(queue) => self.queue = queue,
            SessionEvent::ThreadSwitched => {
                if let Some(session) = &self.session {
                    self.thread = session.thread();
                    self.threads = session.threads();
                }
                self.renaming = None;
                self.all_changed_files = false;
                self.permissions.clear();
                self.expanded_tools.clear();
                self.follow = true;
                self.scroll.scroll_to_bottom();
                if let Some(prompt) = self.send_after_switch.take()
                    && let Some(session) = &self.session
                {
                    session.send(&prompt, Vec::new());
                }
            }
            SessionEvent::AgentChanged(_) => {
                if let Some(session) = &self.session {
                    self.thread.agent = session.thread().agent;
                }
            }
            SessionEvent::Notice(message) => self.show_notice(Notice::info(message), cx),
            SessionEvent::Mcp(status) => {
                let failed: Vec<String> = status
                    .iter()
                    .filter_map(|s| s.result.as_ref().err().map(|e| format!("{}: {e}", s.name)))
                    .collect();
                if !failed.is_empty() {
                    self.show_notice(
                        Notice::error(format!("MCP servers failed — {}", failed.join("; "))),
                        cx,
                    );
                }
                self.mcp_status = status;
            }
            SessionEvent::ThreadsChanged => {
                if let Some(session) = &self.session {
                    self.threads = session.threads();
                }
                cx.emit(ChatEvent::ThreadsChanged);
            }
        }
    }

    pub(crate) fn show_notice(&mut self, notice: Notice, cx: &mut Context<Self>) {
        self.notice = Some(notice);
        self.notice_task = Some(Notice::dismiss_later(cx, |this, _| this.notice = None));
        cx.notify();
    }

    /// Recomputes the review list in the background.
    pub(crate) fn refresh_changes(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        if self.scanning {
            self.rescan = true;
            return;
        }
        self.scanning = true;
        let task = cx.spawn(async move |this, cx| {
            let scan = cx
                .background_spawn(async move { (session.changes(), session.can_undo_restore()) })
                .await;
            this.update(cx, |this, cx| {
                this.scanning = false;
                match scan.0 {
                    Ok(changes) => this.changes = changes,
                    Err(e) => this.show_notice(Notice::error(e), cx),
                }
                this.can_undo_restore = scan.1;
                cx.emit(ChatEvent::ChangesUpdated {
                    files: this.changes.len(),
                });
                if std::mem::take(&mut this.rescan) {
                    this.refresh_changes(cx);
                }
                cx.notify();
            })
            .ok();
        });
        self.tasks.retain(|t| !t.is_ready());
        self.tasks.push(task);
    }

    /// Runs a blocking session call off the UI thread, then refreshes.
    pub(crate) fn run_blocking(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&ChatSession) -> Result<(), String> + std::marker::Send + 'static,
    ) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let task = cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { f(&session) }).await;
            this.update(cx, |this, cx| {
                if let Err(e) = result {
                    this.show_notice(Notice::error(e), cx);
                }
                this.refresh_changes(cx);
            })
            .ok();
        });
        self.tasks.retain(|t| !t.is_ready());
        self.tasks.push(task);
    }

    // Actions

    pub(crate) fn send(&mut self, _: &Send, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker.as_ref().is_some_and(|p| !p.items.is_empty()) {
            self.accept_mention(&AcceptMention, window, cx);
            return;
        }
        let text = self.composer.read(cx).text().to_string();
        if text.trim().is_empty() && self.mentions.is_empty() {
            return;
        }
        let Some(session) = &self.session else {
            self.show_notice(Notice::error("The chat is still starting."), cx);
            return;
        };
        session.send(&text, std::mem::take(&mut self.mentions));
        self.composer.update(cx, |input, cx| input.set_text("", cx));
        self.picker = None;
        self.follow = true;
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    fn new_line(&mut self, _: &NewLine, window: &mut Window, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |input, cx| input.insert("\n", window, cx));
    }

    pub(crate) fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        if let Some(session) = &self.session {
            session.set_mode(mode);
        }
        self.thread.mode = mode;
        cx.notify();
    }

    fn cycle_mode(&mut self, _: &CycleMode, _: &mut Window, cx: &mut Context<Self>) {
        self.set_mode(self.thread.mode.next(), cx);
    }

    pub(crate) fn stop(&mut self, _: &Stop, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = &self.session {
            session.cancel();
        }
        cx.notify();
    }

    fn dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        let closed_overlay = self.settings.take().is_some()
            || std::mem::take(&mut self.show_threads)
            || self.renaming.take().is_some();
        if self.picker.take().is_some() {
            // The picker closes first.
        } else if closed_overlay {
            self.refocus(window, cx);
        } else if self.busy {
            self.stop(&Stop, window, cx);
        } else {
            cx.propagate();
        }
        cx.notify();
    }

    pub(crate) fn new_thread(
        &mut self,
        _: &NewThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mode = self.thread.mode;
        self.switch_to(crate::live::Target::New(mode, None), cx);
        self.show_threads = false;
        self.refocus(window, cx);
        cx.notify();
    }

    pub(crate) fn toggle_threads(
        &mut self,
        _: &ToggleThreads,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_threads = !self.show_threads;
        if let Some(session) = &self.session {
            self.threads = session.threads();
        }
        self.refocus(window, cx);
        cx.notify();
    }

    /// Keeps keyboard focus inside the chat when the composer is hidden
    /// (thread list, settings sheet) and returns it to the composer after.
    pub(crate) fn refocus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.renaming.is_some() {
            // The title field keeps focus.
        } else if self.show_threads || self.settings.is_some() {
            window.focus(&self.focus, cx);
        } else {
            self.composer
                .update(cx, |input, cx| input.focus(window, cx));
        }
    }

    pub(crate) fn toggle_changes(
        &mut self,
        _: &ToggleChanges,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_changes = !self.show_changes;
        cx.notify();
    }

    pub(crate) fn accept_all(&mut self, _: &AcceptAll, _: &mut Window, cx: &mut Context<Self>) {
        self.run_blocking(cx, |s| s.accept_all().map(|_| ()));
    }

    pub(crate) fn reject_all(&mut self, _: &RejectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.run_blocking(cx, |s| s.reject_all().map(|_| ()));
    }

    pub(crate) fn build_plan(&mut self, _: &BuildPlan, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = &self.session {
            session.build_plan();
            self.thread.mode = Mode::Agent;
        }
        cx.notify();
    }

    pub(crate) fn set_agent(&mut self, choice: AgentChoice, cx: &mut Context<Self>) {
        if let Some(session) = &self.session {
            session.set_agent(choice.clone());
            self.thread.agent = choice.key();
        }
        cx.notify();
    }

    pub(crate) fn respond(&mut self, id: u64, decision: Decision, cx: &mut Context<Self>) {
        if let Some(session) = &self.session {
            session.respond_permission(id, decision);
        }
        self.permissions.retain(|p| p.id != id);
        cx.notify();
    }

    pub(crate) fn open_file(&mut self, rel: &Path, line: Option<u32>, cx: &mut Context<Self>) {
        cx.emit(ChatEvent::OpenFile {
            path: self.project.join(rel),
            line,
        });
    }

    pub(crate) fn picker_task(&mut self, task: Option<Task<()>>) {
        self.picker_task = task;
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let body = if let Some(error) = self.load_error.clone() {
            self.render_load_error(&error, cx)
        } else if self.show_threads && !self.shell {
            self.render_threads(window, cx).into_any_element()
        } else {
            v_flex()
                .flex_1()
                .min_h_0()
                .child(self.render_messages(window, cx))
                .when(self.show_changes && !self.changes.is_empty(), |this| {
                    this.child(self.render_changes(window, cx))
                })
                .child(self.render_composer(window, cx))
                .into_any_element()
        };
        v_flex()
            .id("ferrum-chat")
            .key_context(crate::KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::send))
            .on_action(cx.listener(Self::new_line))
            .on_action(cx.listener(Self::cycle_mode))
            .on_action(cx.listener(Self::stop))
            .on_action(cx.listener(Self::dismiss))
            .on_action(cx.listener(Self::new_thread))
            .on_action(cx.listener(Self::toggle_threads))
            .on_action(cx.listener(Self::toggle_changes))
            .on_action(cx.listener(Self::accept_all))
            .on_action(cx.listener(Self::reject_all))
            .on_action(cx.listener(Self::build_plan))
            .on_action(cx.listener(Self::mention_next))
            .on_action(cx.listener(Self::mention_prev))
            .on_action(cx.listener(Self::accept_mention))
            .on_action(
                cx.listener(|this, _: &OpenSettings, window, cx| this.open_settings(window, cx)),
            )
            .relative()
            .size_full()
            .min_w(px(320.))
            .bg(colors.panel_background)
            .text_color(colors.text)
            .child(self.render_header(window, cx))
            .child(body)
            .on_action(cx.listener(Self::attach_files))
            .when_some(self.settings.is_some().then_some(()), |this, _| {
                this.child(self.render_settings(window, cx))
            })
            .when_some(self.notice.clone(), |this, notice| {
                this.child(render_notice(
                    &notice,
                    cx.listener(|this, _, _, cx| {
                        this.notice = None;
                        cx.notify();
                    }),
                    cx,
                ))
            })
    }
}
