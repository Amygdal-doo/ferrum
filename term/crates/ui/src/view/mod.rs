//! The terminal view: sidebar (see `sidebar`), tab bar, the active window's
//! panes (painted cell-by-cell from server frames, see `paint`), dialogs and
//! the transient message strip. Its public, embeddable API is in `api`.

mod api;
mod ime;
mod render;

use crate::app::{App, UiEvent};
use crate::events::TermViewEvent;
use crate::input::{self, Action, Shortcut};
use crate::options::TermViewOptions;
use crate::paint::CellMetrics;
use crate::selection::{Cell, Selection, cell_at};
use ferrum_ui::TextInput;
use ferrum_ui::gpui::{
    self, Bounds, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, Focusable,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, ScrollDelta,
    ScrollEvent, Subscription, Task, Window, px,
};
use ferrumterm_protocol::{BlockId, Event};
use futures::StreamExt as _;
use std::time::{Duration, Instant};

pub(crate) use render::TabKind;

pub(crate) const PADDING: f32 = 6.0;
pub(crate) const MESSAGE_TTL: Duration = Duration::from_secs(6);

pub struct TermView {
    pub(crate) app: App,
    pub(crate) options: TermViewOptions,
    pub(crate) focus_handle: FocusHandle,
    /// Shared Ferrum project bar (desktop window only, see `project`).
    pub(crate) project: Option<crate::project::ProjectState>,
    pub(crate) metrics: Option<CellMetrics>,
    pub(crate) grid: Option<(u16, u16)>,
    wheel: f32,
    /// IME composition in progress (not yet sent to the program).
    pub(crate) ime_marked: Option<String>,
    /// Caret of the focused pane in window coordinates, for IME popups.
    pub(crate) cursor_bounds: Option<Bounds<Pixels>>,
    message_scheduled: Option<Instant>,
    /// Title last reported with `TermViewEvent::TitleChanged`.
    last_title: String,
    /// Sidebar "Custom command…" field while it is open.
    pub(crate) custom_agent: Option<Entity<TextInput>>,
    pub(crate) _custom_agent_events: Option<Subscription>,
    pub(crate) resizing_sidebar: bool,
    /// Open modal (SSH host form, ports).
    pub(crate) dialog: Option<crate::dialogs::Dialog>,
    pub(crate) _dialog_events: Vec<Subscription>,
    pub(crate) selection: Option<Selection>,
    /// Text under the selection when the drag ended (what ⌘C copies).
    selection_text: Option<String>,
    selecting: bool,
    /// Blink phase of the focused pane's cursor (see `settings::Blink`).
    pub(crate) cursor_on: bool,
    /// Font the cell metrics were measured with.
    pub(crate) font_key: Option<(f32, gpui::SharedString)>,
    _blink: Task<()>,
    _message_timer: Option<Task<()>>,
    _events: Task<()>,
}

impl EventEmitter<TermViewEvent> for TermView {}

impl TermView {
    fn build(options: TermViewOptions, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (proxy, mut events) = futures::channel::mpsc::unbounded();
        let app = App::new(&options, proxy);
        app.connect(Duration::ZERO);
        let events = cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                if this
                    .update(cx, |view, cx| view.handle_event(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let blink = cx.spawn(async move |this, cx| {
            loop {
                let interval = crate::settings::get().cursor_blink.interval_ms();
                cx.background_executor()
                    .timer(Duration::from_millis(interval.unwrap_or(500)))
                    .await;
                let alive = this.update(cx, |view, cx| {
                    let next = interval.is_none() || !view.cursor_on;
                    if next != view.cursor_on {
                        view.cursor_on = next;
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        let focus_handle = cx.focus_handle();
        let project = options
            .standalone
            .then(|| crate::project::ProjectState::new(window, cx));
        if options.standalone {
            window.focus(&focus_handle, cx);
        }
        Self {
            app,
            options,
            focus_handle,
            project,
            metrics: None,
            grid: None,
            wheel: 0.0,
            ime_marked: None,
            cursor_bounds: None,
            message_scheduled: None,
            last_title: String::new(),
            custom_agent: None,
            _custom_agent_events: None,
            resizing_sidebar: false,
            dialog: None,
            _dialog_events: Vec::new(),
            selection: None,
            selection_text: None,
            selecting: false,
            cursor_on: true,
            font_key: None,
            _blink: blink,
            _message_timer: None,
            _events: events,
        }
    }

    pub(crate) fn handle_event(&mut self, event: UiEvent, cx: &mut Context<Self>) {
        match event {
            UiEvent::Connected(client, backend) => {
                self.app.client = Some(client);
                self.app.backend = Some(backend);
                self.app.subscribed.clear();
                self.app.refresh();
                self.app.refresh_forwards();
                if backend == crate::Backend::Local && self.app.attach_requested() {
                    self.app.notify(
                        "FerrumTerm server unavailable: terminals run inside this app and end with it",
                    );
                }
                cx.emit(TermViewEvent::Connected(backend));
            }
            UiEvent::CommandStarted(command, block) => {
                self.app.commands.insert(block, command);
                cx.emit(TermViewEvent::CommandStarted { command, block });
                if let Some(i) = self.app.early_exits.iter().position(|(b, _)| *b == block) {
                    let (_, exit_code) = self.app.early_exits.remove(i);
                    self.app.commands.remove(&block);
                    cx.emit(TermViewEvent::CommandFinished {
                        block,
                        command: Some(command),
                        exit_code,
                    });
                }
            }
            UiEvent::Server(event) => {
                self.report(&event, cx);
                let frame = match &event {
                    Event::Frame { block, .. } => Some(*block),
                    _ => None,
                };
                self.app.on_event(event);
                if let Some(block) = frame {
                    self.focused_frame(block, cx);
                    cx.notify();
                    return;
                }
            }
            UiEvent::Sessions(sessions) => self.app.set_sessions(sessions),
            UiEvent::Message(message) => self.app.notify(message),
            UiEvent::Disconnected(why) => {
                self.app.client = None;
                self.app.notify(format!("{why} — retrying…"));
                self.app.connect(Duration::from_secs(2));
            }
            UiEvent::Clipboard(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.app.notify("Copied screen to clipboard");
            }
            UiEvent::Forwards(forwards) => self.app.ssh.forwards = forwards,
            UiEvent::Ports(result) => self.on_ports_result(result),
            UiEvent::Killed(outcome) => self.on_killed(outcome, cx),
        }
        self.after_change(cx);
    }

    /// Tells the host about statuses, exits and bells in this view's panes
    /// (before `on_event`, while the block is still in the session list).
    fn report(&mut self, event: &Event, cx: &mut Context<Self>) {
        match event {
            Event::Status { block, status } if self.app.shows_block(*block) => {
                cx.emit(TermViewEvent::AgentStatus {
                    block: *block,
                    title: self.block_title(*block),
                    status: status.clone(),
                });
            }
            Event::Exited { block, code } => {
                if let Some(command) = self.app.commands.remove(block) {
                    cx.emit(TermViewEvent::CommandFinished {
                        block: *block,
                        command: Some(command),
                        exit_code: *code,
                    });
                } else if self.app.shows_block(*block) && self.app.knows_block(*block) {
                    cx.emit(TermViewEvent::CommandFinished {
                        block: *block,
                        command: None,
                        exit_code: *code,
                    });
                } else {
                    self.app.remember_exit(*block, *code);
                }
            }
            Event::Bell { block } if self.app.shows_block(*block) => {
                cx.emit(TermViewEvent::Bell { block: *block });
            }
            _ => {}
        }
    }

    fn block_title(&self, block: BlockId) -> String {
        self.app
            .sessions
            .iter()
            .flat_map(|s| &s.windows)
            .flat_map(|w| &w.blocks)
            .find(|b| b.id == block)
            .map(crate::sidebar::model::block_title)
            .unwrap_or_else(|| block.to_string())
    }

    /// Resizes/subscribes the visible window and re-renders.
    pub(crate) fn after_change(&mut self, cx: &mut Context<Self>) {
        self.app.sync(self.grid);
        self.sync_project(cx);
        let title = self.title();
        if title != self.last_title {
            self.last_title.clone_from(&title);
            cx.emit(TermViewEvent::TitleChanged(title));
        }
        if let Some((_, shown_at)) = &self.app.message
            && self.message_scheduled != Some(*shown_at)
        {
            self.message_scheduled = Some(*shown_at);
            self._message_timer = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(MESSAGE_TTL).await;
                if let Err(error) = this.update(cx, |_, cx| cx.notify()) {
                    tracing::debug!(%error, "view closed before the message expired");
                }
            }));
        }
        cx.notify();
    }

    pub(crate) fn act(&mut self, action: Action, cx: &mut Context<Self>) {
        match action {
            Action::Shortcut(Shortcut::Paste) => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.clear_selection();
                    self.app.paste(text);
                }
            }
            Action::Shortcut(Shortcut::Copy) => {
                if let Some(text) = &self.selection_text {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                }
            }
            action => {
                if matches!(action, Action::Send(_)) {
                    self.clear_selection();
                }
                self.app.act(action);
            }
        }
        self.after_change(cx);
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Keys typed into the sidebar's text field are not for the terminal.
        if !self.focus_handle.is_focused(window) {
            if event.keystroke.key == "escape" {
                if self.dialog.is_some() {
                    self.close_dialog(window, cx);
                } else {
                    self.end_custom_agent(window, cx);
                }
            }
            return;
        }
        // ⌘ shortcuts normally arrive as actions (see `actions`); this also
        // covers hosts that did not call `ferrumterm_ui::init`.
        self.cursor_on = true;
        if let Some(action) = input::translate(&event.keystroke) {
            cx.stop_propagation();
            self.ime_marked = None;
            self.act(action, cx);
        }
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        self.close_overlay_sidebar();
        let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
        self.app.click(x, y);
        self.selection = self
            .cell_under(x, y, None)
            .map(|(block, cell)| Selection::new(block, cell));
        self.selection_text = None;
        self.selecting = self.selection.is_some();
        self.after_change(cx);
    }

    /// Extends the selection while the left button is held.
    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selecting || event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let Some(block) = self.selection.as_ref().map(|s| s.block) else {
            return;
        };
        let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
        if let Some((_, head)) = self.cell_under(x, y, Some(block))
            && let Some(selection) = self.selection.as_mut()
            && selection.head != head
        {
            selection.head = head;
            cx.notify();
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.selecting) {
            return;
        }
        match &self.selection {
            Some(s) if !s.is_empty() => {
                self.selection_text = self.app.frames.get(&s.block).map(|f| s.text(f));
                if crate::settings::get().copy_on_select
                    && let Some(text) = &self.selection_text
                {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                }
            }
            _ => self.clear_selection(),
        }
        cx.notify();
    }

    /// The pane and cell under a window point; with `block`, that pane
    /// only (clamped to its edges, so drags can leave it).
    fn cell_under(&self, x: f32, y: f32, block: Option<BlockId>) -> Option<(BlockId, Cell)> {
        let metrics = self.metrics?;
        let pane = self.app.panes.iter().find(|pane| match block {
            Some(b) => pane.0 == b,
            None => {
                let (_, px, py, w, h) = **pane;
                x >= px && x < px + w && y >= py && y < py + h
            }
        })?;
        let cell = cell_at(
            pane,
            x,
            y,
            f32::from(metrics.cell_width),
            f32::from(metrics.cell_height),
        );
        Some((pane.0, cell))
    }

    fn clear_selection(&mut self) {
        self.selection = None;
        self.selection_text = None;
        self.selecting = false;
    }

    fn scroll_wheel(&mut self, event: &ScrollEvent, _: &mut Window, _: &mut Context<Self>) {
        // The selection is in screen cells; scrolling would move the text under it.
        self.clear_selection();
        let cell_height = self.metrics.map_or(px(16.), |metrics| metrics.cell_height);
        self.wheel += match event.delta {
            ScrollDelta::Lines(lines) => lines.y * 3.0,
            ScrollDelta::Pixels(pixels) => pixels.y / cell_height,
        };
        let lines = self.wheel.trunc();
        self.wheel -= lines;
        self.app.scroll(lines as i32);
    }
}

impl Focusable for TermView {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
