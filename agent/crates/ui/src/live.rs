//! Threads that keep running while the user looks elsewhere, their status
//! (Needs you / Done / Working / Idle) and the notifications it raises.
//!
//! The view shows one session (`ChatView::session`). Switching to another
//! thread while that one is busy parks it as a [`Background`] session — a
//! [`ChatSession::sibling`] sharing the project's change set — instead of
//! cancelling its turn; switching back makes it the shown session again.
//! Every session's events go through [`ChatView::route`], which feeds the
//! status board (`ferrumagent_core::status`) and, for the shown session,
//! the view. A background session is let go once its turn ends (the
//! thread is saved by then; its status stays on the board).

use chrono::Utc;
use ferrum_ui::Notice;
use ferrum_ui::NotificationEvent;
use ferrum_ui::components::IconName;
use ferrum_ui::notify::Channels;
use ferrumagent_core::modes::Mode;
use ferrumagent_core::session::{ChatSession, SessionEvent};
use ferrumagent_core::status::{self, Alert, StatusInput, TurnOutcome};
use ferrumagent_core::thread::ThreadEvent;
use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{App, Context, Task, Window};
use std::rc::Rc;

use crate::view::{ChatEvent, ChatView};

/// Settings › Notifications event: a thread waits on a permission prompt
/// or a plan.
pub const NEEDS_YOU_EVENT: &str = "agent-needs-you";
/// Settings › Notifications event: a thread's turn finished while the user
/// was not looking at it.
pub const DONE_EVENT: &str = "agent-done";
/// Settings › Notifications event: a thread's turn ended with an error
/// while the user was not looking at it.
pub const FAILED_EVENT: &str = "agent-failed";

/// Add to the host's `AppInfo` so the user can pick their channels.
pub const NEEDS_YOU: NotificationEvent = NotificationEvent {
    id: NEEDS_YOU_EVENT,
    title: "Agent needs you",
    icon: IconName::Warning,
    tint: 0xf5a524,
    defaults: Channels::ALL,
};

/// See [`NEEDS_YOU`].
pub const DONE: NotificationEvent = NotificationEvent {
    id: DONE_EVENT,
    title: "Agent finished",
    icon: IconName::CircleCheck,
    tint: 0x4c8bf5,
    defaults: Channels::ALL,
};

/// See [`NEEDS_YOU`].
pub const FAILED: NotificationEvent = NotificationEvent {
    id: FAILED_EVENT,
    title: "Agent failed",
    icon: IconName::XCircle,
    tint: 0xe5484d,
    defaults: Channels::ALL,
};

/// What clicking a notification's toast does: shows its thread.
pub type ToastClick = Rc<dyn Fn(&mut Window, &mut App)>;

/// The Settings › Notifications event, title and body of `alert` for the
/// thread titled `title`.
pub(crate) fn notification(title: &str, alert: Alert) -> (&'static str, String, String) {
    let title = status::one_line(title, 60);
    match alert {
        Alert::NeedsYou { summary } => (NEEDS_YOU_EVENT, format!("{title} needs you"), summary),
        Alert::Finished {
            summary,
            error: false,
        } => (DONE_EVENT, format!("{title} finished"), summary),
        Alert::Finished {
            summary,
            error: true,
        } => (FAILED_EVENT, format!("{title} failed"), summary),
    }
}

/// Delivers a notification: `(event id, title, body, on click, cx)`.
pub type Notifier = Rc<dyn Fn(&'static str, String, String, ToastClick, &mut App)>;

/// The default notifier: `ferrum_ui::notify`, for events the app lists in
/// its `AppInfo` (so a host without them — and tests — stay quiet).
pub fn default_notifier() -> Notifier {
    Rc::new(|event, title, body, click, cx| {
        if ferrum_ui::app_info(cx).events.iter().any(|e| e.id == event) {
            ferrum_ui::notify::notify_with_action(
                event,
                title,
                body,
                move |window, cx| click(window, cx),
                cx,
            );
        }
    })
}

/// A session whose thread runs while another is shown.
pub(crate) struct Background {
    pub serial: u64,
    pub session: ChatSession,
    pub thread_id: String,
    pub title: String,
    /// Messages waiting behind its turn.
    pub queued: usize,
}

/// Where a switch goes.
pub(crate) enum Target {
    Open(String),
    New(Mode, Option<String>),
}

impl ChatView {
    /// Starts listening to a session's events; returns its serial.
    pub(crate) fn listen(
        &mut self,
        rx: UnboundedReceiver<SessionEvent>,
        cx: &mut Context<Self>,
    ) -> u64 {
        self.next_serial += 1;
        let serial = self.next_serial;
        let task: Task<()> = cx.spawn(async move |this, cx| {
            let mut rx = rx;
            while let Some(event) = rx.next().await {
                let mut batch = vec![event];
                // Coalesce bursts (streamed deltas) into one update.
                while let Ok(more) = rx.try_recv() {
                    batch.push(more);
                    if batch.len() >= 256 {
                        break;
                    }
                }
                if this
                    .update(cx, |this, cx| {
                        for event in batch {
                            this.route(serial, event, cx);
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
            }
        });
        self.tasks.retain(|t| !t.is_ready());
        self.tasks.push(task);
        serial
    }

    /// One session event: to the view if its session is shown, and to the
    /// status board either way.
    fn route(&mut self, serial: u64, event: SessionEvent, cx: &mut Context<Self>) {
        if serial == self.serial {
            let input = status_input(&event, || {
                let mut thread = self.thread.clone();
                if let SessionEvent::Thread(e) = &event {
                    thread.apply(e);
                }
                (thread, !self.queue.is_empty())
            });
            let switched = matches!(event, SessionEvent::ThreadSwitched);
            self.on_session_event(event, cx);
            if let Some(input) = input {
                let (id, title) = (self.thread.id.clone(), self.thread.title.clone());
                self.apply_status(&id, &title, input, cx);
            }
            if switched {
                self.mark_viewed(cx);
            }
            return;
        }
        let Some(bg) = self.background.iter_mut().find(|b| b.serial == serial) else {
            return;
        };
        match &event {
            SessionEvent::Thread(ThreadEvent::TitleChanged(title)) => bg.title = title.clone(),
            SessionEvent::Queue(queue) => bg.queued = queue.len(),
            _ => {}
        }
        let (session, queued) = (bg.session.clone(), bg.queued);
        let (id, title) = (bg.thread_id.clone(), bg.title.clone());
        if let Some(input) = status_input(&event, || (session.thread(), queued > 0)) {
            self.apply_status(&id, &title, input, cx);
        }
        match event {
            SessionEvent::ChangesUpdated => self.refresh_changes(cx),
            SessionEvent::ThreadsChanged => {
                if let Some(shown) = &self.session {
                    self.threads = shown.threads();
                }
                cx.emit(ChatEvent::ThreadsChanged);
            }
            // Its turn is over and nothing is queued: let it go (it is
            // saved; its status stays on the board).
            SessionEvent::Busy(false) if !session.is_busy() => {
                self.background.retain(|b| b.serial != serial);
            }
            _ => {}
        }
    }

    /// Feeds the board; delivers the alert, if any.
    fn apply_status(&mut self, id: &str, title: &str, input: StatusInput, cx: &mut Context<Self>) {
        let viewing = self.is_viewing(id);
        let before = self.statuses.get(id).cloned();
        let alert = self.statuses.apply(id, input, viewing, Utc::now());
        if self.statuses.get(id) != before.as_ref() {
            cx.emit(ChatEvent::StatusChanged);
        }
        if let Some(alert) = alert {
            self.alert(id, title, alert, cx);
        }
    }

    fn alert(&mut self, id: &str, title: &str, alert: Alert, cx: &mut Context<Self>) {
        let (event, title, body) = notification(title, alert);
        let (view, id) = (cx.entity().downgrade(), id.to_string());
        let click: ToastClick = Rc::new(move |window, cx| {
            view.update(cx, |view, cx| view.reveal_thread(&id, window, cx))
                .ok();
        });
        let notify = self.notifier.clone();
        notify(event, title, body, click, cx);
    }

    /// Shows thread `id` (a notification was clicked) and asks the host to
    /// bring the chat on screen ([`ChatEvent::Reveal`]).
    pub fn reveal_thread(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.switch_to(Target::Open(id.to_string()), cx);
        self.show_threads = false;
        self.settings = None;
        self.refocus(window, cx);
        cx.emit(ChatEvent::Reveal {
            thread_id: id.to_string(),
        });
        cx.notify();
    }

    /// Whether the user is looking at thread `id` right now.
    pub(crate) fn is_viewing(&self, id: &str) -> bool {
        self.visible && self.window_active && self.thread.id == id && self.session.is_some()
    }

    /// The shown thread is being looked at: Done clears.
    pub(crate) fn mark_viewed(&mut self, cx: &mut Context<Self>) {
        let id = self.thread.id.clone();
        if self.is_viewing(&id) && self.statuses.get(&id).is_some() {
            let title = self.thread.title.clone();
            self.apply_status(&id, &title, StatusInput::Viewed, cx);
        }
    }

    /// The host shows or hides the chat (a closed Agent pane, another
    /// project or page). Hidden chats keep running; their turns end as
    /// Done and notify.
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.visible != visible {
            self.visible = visible;
            self.mark_viewed(cx);
            cx.notify();
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub(crate) fn window_activated(&mut self, active: bool, cx: &mut Context<Self>) {
        self.window_active = active;
        self.mark_viewed(cx);
    }

    /// Every thread's status this run (threads not on it are Idle).
    pub fn statuses(&self) -> &ferrumagent_core::status::StatusBoard {
        &self.statuses
    }

    /// Threads running in the background (not shown).
    pub fn background_threads(&self) -> Vec<String> {
        self.background
            .iter()
            .map(|b| b.thread_id.clone())
            .collect()
    }

    /// Shows another thread, or a new one. A busy shown thread keeps
    /// running in the background rather than being cancelled.
    pub(crate) fn switch_to(&mut self, target: Target, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        if let Target::Open(id) = &target {
            if *id == self.thread.id {
                return;
            }
            if let Some(index) = self.background.iter().position(|b| b.thread_id == *id) {
                let bg = self.background.remove(index);
                self.park_shown();
                self.show(bg.session, bg.serial, cx);
                return;
            }
        }
        let keep = session.is_busy() || !session.open_permissions().is_empty();
        if !keep {
            match target {
                Target::Open(id) => {
                    if let Err(e) = session.open_thread(&id) {
                        self.show_notice(Notice::error(e), cx);
                    }
                }
                Target::New(mode, prompt) => {
                    session.new_thread_in(mode);
                    self.thread.mode = mode;
                    // Sent once the view has re-read the new thread, so the
                    // message's events are not applied on top of a copy
                    // that already has them.
                    self.send_after_switch = prompt.filter(|p| !p.trim().is_empty());
                }
            }
            cx.notify();
            return;
        }
        let (id, mode, prompt) = match target {
            Target::Open(id) => (Some(id), self.thread.mode, None),
            Target::New(mode, prompt) => (None, mode, prompt),
        };
        match session.sibling(id.as_deref(), mode) {
            Ok((sibling, rx)) => {
                self.park_shown();
                let serial = self.listen(rx, cx);
                self.show(sibling.clone(), serial, cx);
                if let Some(prompt) = prompt.filter(|p| !p.trim().is_empty()) {
                    sibling.send(&prompt, Vec::new());
                }
            }
            Err(e) => self.show_notice(Notice::error(e), cx),
        }
    }

    /// Moves the shown session to the background if its turn still runs;
    /// otherwise it is let go.
    fn park_shown(&mut self) {
        let Some(session) = self.session.take() else {
            return;
        };
        if session.is_busy() || !session.open_permissions().is_empty() {
            self.background.push(Background {
                serial: self.serial,
                session,
                thread_id: self.thread.id.clone(),
                title: self.thread.title.clone(),
                queued: self.queue.len(),
            });
        }
    }

    /// Makes `session` the shown one.
    fn show(&mut self, session: ChatSession, serial: u64, cx: &mut Context<Self>) {
        self.serial = serial;
        self.thread = session.thread();
        self.threads = session.threads();
        self.permissions = session.open_permissions();
        self.busy = session.is_busy();
        self.queue = session.queued();
        self.session = Some(session);
        self.renaming = None;
        self.all_changed_files = false;
        self.expanded_tools.clear();
        self.follow = true;
        self.scroll.scroll_to_bottom();
        self.mark_viewed(cx);
        self.refresh_changes(cx);
        cx.notify();
    }

    /// Deletes a saved thread; a background one is stopped first, for good
    /// (its cancelled turn does not write the thread back).
    pub(crate) fn delete_thread(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(index) = self.background.iter().position(|b| b.thread_id == id) {
            self.background.remove(index).session.discard();
        }
        if let Some(session) = self.session.clone() {
            if let Err(e) = session.delete_thread(id) {
                self.show_notice(Notice::error(e), cx);
            }
            self.threads = session.threads();
        }
        self.statuses.remove(id);
        cx.emit(ChatEvent::StatusChanged);
        cx.notify();
    }
}

/// What `event` means for its thread's status. `after` gives the thread
/// with the event applied and whether more messages are queued (read only
/// when a turn ends).
fn status_input(
    event: &SessionEvent,
    after: impl FnOnce() -> (ferrumagent_core::thread::Thread, bool),
) -> Option<StatusInput> {
    Some(match event {
        SessionEvent::Busy(true) => StatusInput::TurnStarted,
        SessionEvent::Permission(request) => StatusInput::PermissionAsked {
            summary: status::permission_summary(request),
        },
        SessionEvent::PermissionClosed(_) => StatusInput::PermissionAnswered,
        SessionEvent::Thread(ThreadEvent::PlanWritten(_)) => StatusInput::PlanWritten,
        SessionEvent::Thread(ThreadEvent::TurnEnded(stop)) => {
            let (thread, more_queued) = after();
            StatusInput::TurnEnded {
                outcome: TurnOutcome::of(stop),
                summary: status::turn_summary(&thread, stop),
                more_queued,
            }
        }
        _ => return None,
    })
}
