//! Opening the session: [`ChatSession::new`] snapshots the project off the
//! UI thread, and the chat shows "Opening project" until it answers. It
//! always ends: in the session, or in an error that says what to do with
//! a Try again button — the core's own (git too slow, folder too big, no
//! such folder), a panic turned into one, or, past [`OPEN_TIMEOUT`], ours.
//! An open that answers after the timeout still opens the chat.

use std::panic::AssertUnwindSafe;
use std::rc::Rc;
use std::time::Duration;

use ferrum_ui::prelude::*;
use ferrumagent_core::session::{ChatSession, SessionConfig, SessionEvent};
use futures::channel::mpsc::UnboundedReceiver;
use futures::future::{BoxFuture, Either};
use gpui::Context;

use crate::view::ChatView;

/// What opening a session gives.
pub(crate) type Opened = Result<(ChatSession, UnboundedReceiver<SessionEvent>), String>;

/// Opens a session from its configuration (tests: one that never answers).
pub(crate) type Opener = Rc<dyn Fn(SessionConfig) -> BoxFuture<'static, Opened>>;

/// After this long the chat stops waiting and says why opening may be
/// slow (the core's git step gives up before, with its own message).
pub(crate) const OPEN_TIMEOUT: Duration = Duration::from_secs(90);

/// [`ChatSession::new`]; a panic becomes an error instead of an open that
/// never answers.
pub(crate) fn default_opener() -> Opener {
    Rc::new(|config| {
        Box::pin(async move {
            std::panic::catch_unwind(AssertUnwindSafe(|| ChatSession::new(config)))
                .unwrap_or_else(|_| Err("FerrumChat failed while opening the project.".into()))
        })
    })
}

impl ChatView {
    /// Opens the session in the background (again, after an error).
    pub(crate) fn open_session(&mut self, cx: &mut Context<Self>) {
        let open = cx.background_spawn((self.opener)(self.open_config.clone()));
        self.open_attempt += 1;
        self.load_error = None;
        let attempt = self.open_attempt;
        let timeout = cx.background_executor().timer(OPEN_TIMEOUT);
        let task = cx.spawn(async move |this, cx| {
            let result = match futures::future::select(open, timeout).await {
                Either::Left((result, _)) => result,
                Either::Right(((), open)) => {
                    let late = this.update(cx, |this, cx| {
                        if this.open_attempt == attempt && this.session.is_none() {
                            this.load_error = Some(slow_open_message(&this.project));
                            cx.notify();
                        }
                    });
                    if late.is_err() {
                        return;
                    }
                    open.await
                }
            };
            this.update(cx, |this, cx| this.opened(attempt, result, cx))
                .ok();
        });
        self.tasks.retain(|t| !t.is_ready());
        self.tasks.push(task);
    }

    /// The answer of open number `attempt` (a newer one wins).
    fn opened(&mut self, attempt: u64, result: Opened, cx: &mut Context<Self>) {
        if attempt != self.open_attempt || self.session.is_some() {
            return;
        }
        match result {
            Ok((session, rx)) => {
                self.load_error = None;
                self.serial = self.listen(rx, cx);
                self.thread = session.thread();
                self.threads = session.threads();
                self.session = Some(session);
                if let Some(request) = self.pending.take() {
                    self.host_request(request, cx);
                }
                self.refresh_changes(cx);
            }
            Err(error) => self.load_error = Some(error),
        }
        cx.notify();
    }

    /// Why the chat is not open, and Try again.
    pub(crate) fn render_load_error(&self, error: &str, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .flex_1()
            .p_4()
            .gap_2()
            .child(Label::new("Could not open the project").color(Color::Error))
            .child(
                Label::new(error.to_string())
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                h_flex().child(
                    Button::new("chat-open-retry", "Try again")
                        .style(ButtonStyle::Filled)
                        .label_size(LabelSize::Small)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_session(cx);
                            cx.notify();
                        })),
                ),
            )
            .into_any_element()
    }
}

fn slow_open_message(project: &std::path::Path) -> String {
    format!(
        "Opening {} is taking over {} s. The agent first takes a snapshot of the project's \
         files, so it can show and undo its changes; large untracked folders (build output, \
         data, dependencies) slow this down: add them to .gitignore. The chat opens by itself \
         if the snapshot finishes.",
        project.display(),
        OPEN_TIMEOUT.as_secs()
    )
}
