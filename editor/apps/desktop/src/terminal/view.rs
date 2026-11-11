//! One terminal in the panel: keyboard input to the PTY (named keys and
//! Ctrl / Alt combinations from key-down, text through GPUI's input
//! handler so IME works, see `ime`), mouse selection, wheel scrollback,
//! and painting of the emulator's frame.

use ferrumeditor_core::terminal::{Mods, TermKey, tab_title, translate};
use ferrumterm_protocol::Input;
use futures::StreamExt as _;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    App, Bounds, ClipboardItem, Context, ElementInputHandler, EventEmitter, FocusHandle, Focusable,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point, Render, ScrollDelta,
    ScrollEvent, Subscription, Task, Window, canvas, div, prelude::*, px,
};

use super::paint::{self, Metrics, Palette};
use super::selection::{Cell, Selection};
use super::session::Session;
use crate::actions::RunCommand;
use crate::settings::Settings;

const PADDING: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalEvent {
    /// Output arrived (title may have changed).
    Changed,
    /// The shell ended.
    Exited,
    /// A task ended with this exit code.
    TaskFinished(i32),
    Focused,
}

pub struct TerminalView {
    pub id: u64,
    session: Session,
    focus_handle: FocusHandle,
    metrics: Option<Metrics>,
    /// Where the grid is painted (window coordinates).
    origin: Option<Point<Pixels>>,
    pub(super) caret: Option<Bounds<Pixels>>,
    selection: Option<Selection>,
    selection_text: Option<String>,
    selecting: bool,
    wheel: f32,
    pub(super) ime_marked: Option<String>,
    exited: bool,
    /// Tasks: the label shown in the tab, and whether the end was reported.
    pub task: Option<String>,
    task_reported: bool,
    /// AI agent sessions: the agent's name (the tab shows its status).
    pub agent: Option<String>,
    /// When output last arrived (agents idle → "waiting for input").
    pub last_output: std::time::Instant,
    _reader: Task<()>,
    _focus: Subscription,
}

impl EventEmitter<TerminalEvent> for TerminalView {}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TerminalView {
    pub fn new(
        id: u64,
        session: Session,
        mut output: UnboundedReceiver<()>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let reader = cx.spawn(async move |this, cx| {
            while output.next().await.is_some() {
                // Coalesce bursts of output into one repaint.
                while output.try_recv().is_ok() {}
                if this.update(cx, |view, cx| view.on_output(cx)).is_err() {
                    break;
                }
            }
        });
        let focus_handle = cx.focus_handle();
        let focus = cx.on_focus(&focus_handle, window, |_, _, cx| {
            cx.emit(TerminalEvent::Focused)
        });
        Self {
            id,
            session,
            focus_handle,
            metrics: None,
            origin: None,
            caret: None,
            selection: None,
            selection_text: None,
            selecting: false,
            wheel: 0.0,
            ime_marked: None,
            exited: false,
            task: None,
            task_reported: false,
            agent: None,
            last_output: std::time::Instant::now(),
            _reader: reader,
            _focus: focus,
        }
    }

    fn on_output(&mut self, cx: &mut Context<Self>) {
        self.last_output = std::time::Instant::now();
        if self.task.is_some() || self.agent.is_some() {
            if let Some(code) = self.session.exit_code()
                && !self.task_reported
            {
                self.task_reported = true;
                cx.emit(TerminalEvent::TaskFinished(code));
            }
        } else if self.session.has_exited() && !self.exited {
            self.exited = true;
            cx.emit(TerminalEvent::Exited);
        }
        cx.emit(TerminalEvent::Changed);
        cx.notify();
    }

    #[cfg(test)]
    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn kill(&self) {
        self.session.kill();
    }

    /// Tab title; agents show their status (idle longer than `idle` =
    /// waiting for input).
    pub fn title_with_idle(&self, idle: std::time::Duration) -> String {
        match &self.agent {
            Some(name) => {
                let status = ferrumeditor_core::agents::AgentStatus::of(
                    self.session.exit_code(),
                    false,
                    self.last_output.elapsed(),
                    idle,
                );
                format!("✦ {name} · {}", status.label())
            }
            None => self.title(),
        }
    }

    pub fn title(&self) -> String {
        match &self.task {
            Some(label) => match self.session.exit_code() {
                None => format!("▶ {label}"),
                Some(0) => format!("✓ {label}"),
                Some(_) => format!("✗ {label}"),
            },
            None => tab_title(&self.session.title(), &self.session.shell),
        }
    }

    /// A task's exit code and output, once it has ended.
    pub fn task_result(&self) -> Option<(i32, String)> {
        let code = self.session.exit_code()?;
        Some((code, self.session.captured_output().unwrap_or_default()))
    }

    /// Ends a detached test task.
    #[cfg(test)]
    pub fn finish_task(&mut self, output: &str, code: i32, cx: &mut Context<Self>) {
        self.session.finish(output, code);
        self.on_output(cx);
    }

    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
    }

    pub(super) fn send(&mut self, input: Input, cx: &mut Context<Self>) {
        self.clear_selection();
        self.session.input(&input);
        cx.notify();
    }

    /// Handles terminal-scope commands; false lets others bubble up.
    pub fn run_command(&mut self, name: &str, cx: &mut Context<Self>) -> bool {
        match name {
            "terminal:copy" => {
                if let Some(text) = self.selection_text.clone() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            "terminal:paste" => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.send(Input::Paste(text), cx);
                }
            }
            "terminal:clear" => {
                self.clear_selection();
                self.session.clear();
                cx.notify();
            }
            _ => return false,
        }
        true
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_handle.is_focused(window) {
            return;
        }
        let m = &event.keystroke.modifiers;
        let mods = Mods {
            ctrl: m.control,
            alt: m.alt,
            shift: m.shift,
            platform: m.platform,
        };
        let input = match translate(&event.keystroke.key, mods) {
            Some(TermKey::Key(spec)) => Input::Key(spec),
            Some(TermKey::Text(text)) => Input::Text(text),
            None => return,
        };
        cx.stop_propagation();
        self.ime_marked = None;
        self.send(input, cx);
    }

    fn cell_at(&self, position: Point<Pixels>) -> Option<Cell> {
        let (metrics, origin) = (self.metrics?, self.origin?);
        let frame_cols = f32::from(self.session.frame().cols.max(1));
        let frame_rows = f32::from(self.session.frame().rows.max(1));
        let col = ((position.x - origin.x) / metrics.cell_width)
            .floor()
            .clamp(0.0, frame_cols - 1.0);
        let row = ((position.y - origin.y) / metrics.cell_height)
            .floor()
            .clamp(0.0, frame_rows - 1.0);
        Some(Cell {
            row: row as u16,
            col: col as u16,
        })
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        self.selection = self.cell_at(event.position).map(Selection::new);
        self.selection_text = None;
        self.selecting = self.selection.is_some();
        cx.notify();
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selecting || event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        if let Some(head) = self.cell_at(event.position)
            && let Some(selection) = self.selection.as_mut()
            && selection.head != head
        {
            selection.head = head;
            cx.notify();
        }
    }

    fn mouse_up(&mut self, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.selecting) {
            return;
        }
        match &self.selection {
            Some(s) if !s.is_empty() => self.selection_text = Some(s.text(&self.session.frame())),
            _ => self.clear_selection(),
        }
        cx.notify();
    }

    fn clear_selection(&mut self) {
        self.selection = None;
        self.selection_text = None;
        self.selecting = false;
    }

    fn scroll_wheel(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.clear_selection();
        let cell_height = self.metrics.map_or(px(16.), |m| m.cell_height);
        self.wheel += match event.delta {
            ScrollDelta::Lines(lines) => lines.y * 3.0,
            ScrollDelta::Pixels(pixels) => pixels.y / cell_height,
        };
        let lines = self.wheel.trunc();
        self.wheel -= lines;
        if lines != 0.0 {
            self.session.scroll(Some(lines as i32));
            cx.notify();
        }
    }

    #[cfg(test)]
    pub fn select_cells(&mut self, from: (u16, u16), to: (u16, u16)) {
        let mut selection = Selection::new(Cell {
            row: from.0,
            col: from.1,
        });
        selection.head = Cell {
            row: to.0,
            col: to.1,
        };
        self.selection_text = Some(selection.text(&self.session.frame()));
        self.selection = Some(selection);
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = cx.global::<Settings>();
        let font = gpui::font(settings.config.font_family.clone());
        let font_size = px(settings.font_size());
        let palette = Palette::from_theme(&settings.theme);
        let background = palette.background;
        let metrics = Metrics::measure(&font, font_size, window);
        self.metrics = Some(metrics);
        let entity = cx.entity();
        let focused = self.focus_handle.is_focused(window);
        div()
            .id(("terminal-view", self.id as usize))
            .key_context("FerrumTerminal")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(background)
            .cursor(gpui::CursorStyle::IBeam)
            .on_action(cx.listener(|this, action: &RunCommand, _, cx| {
                if !this.run_command(&action.name, cx) {
                    cx.propagate();
                }
            }))
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.mouse_up(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.mouse_up(cx)),
            )
            .on_scroll(cx.listener(Self::scroll_wheel))
            .child(
                canvas(
                    {
                        let entity = entity.clone();
                        move |bounds, _, cx| {
                            let grid = metrics.grid(bounds.size, px(PADDING));
                            entity.update(cx, |view, _| {
                                view.origin =
                                    Some(bounds.origin + gpui::point(px(PADDING), px(PADDING)));
                                view.session.resize(grid.0, grid.1);
                            });
                        }
                    },
                    move |bounds, _, window, cx| {
                        let focus_handle = entity.read(cx).focus_handle.clone();
                        window.handle_input(
                            &focus_handle,
                            ElementInputHandler::new(bounds, entity.clone()),
                            cx,
                        );
                        let view = entity.read(cx);
                        let frame = view.session.frame();
                        let selection = view.selection.clone().filter(|s| !s.is_empty());
                        let marked = view.ime_marked.clone();
                        let origin = bounds.origin + gpui::point(px(PADDING), px(PADDING));
                        let args = paint::PaintArgs {
                            frame: &frame,
                            origin,
                            metrics,
                            font: &font,
                            palette: &palette,
                            selection: selection.as_ref(),
                            focused,
                            ime_marked: marked.as_deref(),
                        };
                        let caret = window
                            .with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                                paint::paint(&args, window, cx)
                            });
                        entity.update(cx, |view, _| view.caret = caret);
                    },
                )
                .size_full(),
            )
    }
}
