//! What a host drives: opening a thread, starting a task (automations),
//! the settings page, MCP status and the branch shown in the footer; and
//! the thread commands of the header's ⋯ menu (rename, delete, copy).

use ferrum_ui::{Notice, TextInput, TextInputEvent};
use ferrumagent_core::modes::Mode;
use ferrumagent_core::session::McpServerStatus;
use gpui::{AppContext as _, ClipboardItem, Context, Window};

use crate::live::Target;
use crate::view::{ChatView, HeaderSlot};

/// The project's branch, pushed by the host with
/// [`ChatView::set_git_status`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitStatus {
    pub branch: String,
    /// Commits not pushed / not pulled.
    pub ahead: u32,
    pub behind: u32,
    /// Changed files in the working tree.
    pub dirty: usize,
}

/// A git command the footer asks the host to run
/// ([`crate::ChatEvent::Git`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GitRequest {
    Push,
    Pull,
    Fetch,
    Sync,
    Commit,
    SwitchBranch,
}

impl GitRequest {
    pub const ALL: [GitRequest; 6] = [
        GitRequest::Push,
        GitRequest::Pull,
        GitRequest::Fetch,
        GitRequest::Sync,
        GitRequest::Commit,
        GitRequest::SwitchBranch,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GitRequest::Push => "Push",
            GitRequest::Pull => "Pull",
            GitRequest::Fetch => "Fetch",
            GitRequest::Sync => "Sync",
            GitRequest::Commit => "Commit…",
            GitRequest::SwitchBranch => "Switch branch…",
        }
    }
}

/// A host call that arrived before the session opened.
pub(crate) enum HostRequest {
    Open(String),
    Start(Mode, Option<String>),
}

impl ChatView {
    /// Shows a saved thread of this project.
    pub fn open_thread(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.host_request(HostRequest::Open(id.to_string()), cx);
        self.show_threads = false;
        self.refocus(window, cx);
    }

    /// Starts a new thread in `mode`, and sends `prompt` right away when
    /// given (automations).
    pub fn start_task(
        &mut self,
        mode: Mode,
        prompt: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.host_request(HostRequest::Start(mode, prompt), cx);
        self.show_threads = false;
        self.refocus(window, cx);
    }

    /// The "Models and API keys" page.
    pub fn open_settings_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings(window, cx);
    }

    /// The MCP servers' state after the first turn connected them.
    pub fn mcp_status(&self) -> &[McpServerStatus] {
        &self.mcp_status
    }

    /// Puts a shell host's controls in the chat's header row, so the chat
    /// card starts at the top of the window like FerrumTerm's: `start` before
    /// the title (window controls while the host's sidebar is folded; then
    /// `lights` leaves room for the traffic lights), `end` after the ⋯ menu.
    pub fn set_header_slots(
        &mut self,
        lights: bool,
        start: Option<HeaderSlot>,
        end: Option<HeaderSlot>,
        cx: &mut Context<Self>,
    ) {
        self.header_lights = lights;
        self.header_start = start;
        self.header_end = end;
        cx.notify();
    }

    /// The branch shown in the footer (`None` hides it: not a repo).
    pub fn set_git_status(&mut self, status: Option<GitStatus>, cx: &mut Context<Self>) {
        if self.git_status != status {
            self.git_status = status;
            cx.notify();
        }
    }

    /// Runs a host call now, or once the session is open.
    pub(crate) fn host_request(&mut self, request: HostRequest, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            self.pending = Some(request);
            return;
        };
        drop(session);
        let target = match request {
            HostRequest::Open(id) => Target::Open(id),
            HostRequest::Start(mode, prompt) => Target::New(mode, prompt),
        };
        self.switch_to(target, cx);
        cx.notify();
    }

    // The ⋯ menu

    pub(crate) fn start_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.thread.title.clone();
        let input = cx.new(|cx| TextInput::new(window, cx).borderless().with_text(&title));
        let subscription = cx.subscribe_in(&input, window, |this, _, event, window, cx| {
            if matches!(event, TextInputEvent::Submit | TextInputEvent::Blurred) {
                this.commit_rename(window, cx);
            }
        });
        input.update(cx, |input, cx| {
            input.select_range(0..title.len(), cx);
            input.focus(window, cx);
        });
        self.renaming = Some((input, subscription));
        cx.notify();
    }

    pub(crate) fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((input, _)) = self.renaming.take() else {
            return;
        };
        let title = input.read(cx).text().trim().to_string();
        if !title.is_empty() && title != self.thread.title {
            match self.session.clone() {
                Some(session) => match session.rename_thread(&self.thread.id, &title) {
                    Ok(()) => self.thread.title = title,
                    Err(e) => self.show_notice(Notice::error(e), cx),
                },
                None => self.show_notice(Notice::error("The chat is still starting."), cx),
            }
        }
        self.refocus(window, cx);
        cx.notify();
    }

    pub(crate) fn delete_current_thread(&mut self, cx: &mut Context<Self>) {
        let id = self.thread.id.clone();
        self.delete_thread(&id, cx);
    }

    pub(crate) fn copy_markdown(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(self.thread.to_markdown()));
        self.show_notice(Notice::info("Copied the thread as Markdown."), cx);
    }
}
