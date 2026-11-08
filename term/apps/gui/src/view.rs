//! The FerrumTerm window as a GPUI view: tab bar, the active window's panes
//! (painted cell-by-cell from server frames), and the transient message strip.

use crate::app::{App, UiEvent};
use crate::input::{self, Action, Shortcut};
use ferrum_ui::gpui::{
    self, Bounds, ClipboardItem, Context, ElementId, ElementInputHandler, Entity, FocusHandle,
    Focusable, Font, FontStyle, FontWeight, Hsla, KeyDownEvent, MouseButton, MouseDownEvent,
    Pixels, Point, ScrollDelta, ScrollEvent, SharedString, Size, StrikethroughStyle, Task,
    TextRun, UTF16Selection, UnderlineStyle, Window, canvas, div, fill, point, prelude::*, px,
    size,
};
use ferrum_ui::theme::{self, ActiveTheme};
use ferrum_ui::ui::{StyledTypography, h_flex, v_flex};
use ferrumterm_protocol::{
    BlockId, CursorShape, Frame, ProgramState, Rect, Span, WindowInfo, flags,
};
use futures::StreamExt as _;
use futures::channel::mpsc::UnboundedReceiver;
use std::ops::Range;
use std::time::{Duration, Instant};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const TAB_BAR_HEIGHT: f32 = 34.0;
const PADDING: f32 = 6.0;
const MESSAGE_TTL: Duration = Duration::from_secs(6);

/// A clickable tab-bar element.
#[derive(Debug, Clone, PartialEq)]
pub enum TabKind {
    Session,
    Window(String),
    NewWindow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CellMetrics {
    font_pt: f32,
    font_size: Pixels,
    cell_width: Pixels,
    cell_height: Pixels,
}

impl CellMetrics {
    fn measure(font_pt: f32, font: &Font, window: &Window) -> Self {
        let font_size = px(font_pt);
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(font);
        let cell_width = match text_system.advance(font_id, font_size, 'M') {
            Ok(advance) => advance.width,
            Err(error) => {
                tracing::warn!(%error, "could not measure the monospace font");
                font_size * 0.6
            }
        };
        Self {
            font_pt,
            font_size,
            cell_width: cell_width.max(px(1.)),
            cell_height: (font_size * 1.25).round(),
        }
    }

    /// Cells that fit in the terminal area.
    fn grid(&self, area: Size<Pixels>) -> (u16, u16) {
        let padding = px(PADDING) * 2.;
        let cols = ((area.width - padding) / self.cell_width).floor().max(4.0);
        let rows = ((area.height - padding) / self.cell_height).floor().max(2.0);
        (cols as u16, rows as u16)
    }
}

pub struct TermView {
    app: App,
    focus_handle: FocusHandle,
    metrics: Option<CellMetrics>,
    grid: Option<(u16, u16)>,
    wheel: f32,
    /// IME composition in progress (not yet sent to the program).
    ime_marked: Option<String>,
    /// Caret of the focused pane in window coordinates, for IME popups.
    cursor_bounds: Option<Bounds<Pixels>>,
    message_scheduled: Option<Instant>,
    _message_timer: Option<Task<()>>,
    _events: Task<()>,
}

impl TermView {
    pub fn new(
        app: App,
        mut events: UnboundedReceiver<UiEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        Self {
            app,
            focus_handle,
            metrics: None,
            grid: None,
            wheel: 0.0,
            ime_marked: None,
            cursor_bounds: None,
            message_scheduled: None,
            _message_timer: None,
            _events: events,
        }
    }

    fn handle_event(&mut self, event: UiEvent, cx: &mut Context<Self>) {
        match event {
            UiEvent::Connected(client) => {
                self.app.client = Some(client);
                self.app.subscribed.clear();
                self.app.refresh();
            }
            UiEvent::Server(event) => {
                let frame = matches!(event, ferrumterm_protocol::Event::Frame { .. });
                self.app.on_event(event);
                if frame {
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
        }
        self.after_change(cx);
    }

    /// Resizes/subscribes the visible window and re-renders.
    fn after_change(&mut self, cx: &mut Context<Self>) {
        self.app.sync(self.grid);
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

    fn act(&mut self, action: Action, cx: &mut Context<Self>) {
        match action {
            Action::Shortcut(Shortcut::Paste) => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.app.paste(text);
                }
            }
            action => self.app.act(action),
        }
        self.after_change(cx);
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(action) = input::translate(&event.keystroke) {
            cx.stop_propagation();
            self.ime_marked = None;
            self.act(action, cx);
        }
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        self.app
            .click(f32::from(event.position.x), f32::from(event.position.y));
        self.after_change(cx);
    }

    fn scroll_wheel(&mut self, event: &ScrollEvent, _: &mut Window, _: &mut Context<Self>) {
        let cell_height = self.metrics.map_or(px(16.), |metrics| metrics.cell_height);
        self.wheel += match event.delta {
            ScrollDelta::Lines(lines) => lines.y * 3.0,
            ScrollDelta::Pixels(pixels) => pixels.y / cell_height,
        };
        let lines = self.wheel.trunc();
        self.wheel -= lines;
        self.app.scroll(lines as i32);
    }

    fn title(&self) -> String {
        let Some(session) = self.app.session() else {
            return "FerrumTerm".to_owned();
        };
        let title = self
            .app
            .window()
            .and_then(|window| window.blocks.iter().find(|block| block.id == window.focused))
            .map(|block| block.title.clone())
            .unwrap_or_default();
        if title.is_empty() {
            format!("FerrumTerm — {}", session.name)
        } else {
            format!("{title} — {}", session.name)
        }
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let mut bar = h_flex()
            .flex_none()
            .h(px(TAB_BAR_HEIGHT))
            .w_full()
            .px_2()
            .gap(px(2.))
            .bg(colors.tab_bar_background)
            .border_b_1()
            .border_color(colors.border)
            .font_ui(cx)
            .text_size(px(13.));
        let Some(session) = self.app.session() else {
            return bar;
        };
        bar = bar.child(
            self.tab(
                "session",
                format!("◆ {}", session.name),
                false,
                colors.text,
                0.0,
                TabKind::Session,
                cx,
            )
            .mr(px(6.)),
        );
        for window in &session.windows {
            let active = window.id == session.active_window;
            bar = bar.child(self.tab(
                ElementId::Name(format!("window-{}", window.id).into()),
                tab_label(window),
                active,
                if active { colors.text } else { colors.text_muted },
                110.0,
                TabKind::Window(window.id.to_string()),
                cx,
            ));
        }
        bar.child(self.tab(
            "new-window",
            "+".to_owned(),
            false,
            colors.text_muted,
            32.0,
            TabKind::NewWindow,
            cx,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn tab(
        &self,
        id: impl Into<ElementId>,
        label: String,
        active: bool,
        color: Hsla,
        min_width: f32,
        kind: TabKind,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let colors = cx.theme().colors().clone();
        div()
            .id(id)
            .h_full()
            .px(px(14.))
            .min_w(px(min_width))
            .flex()
            .items_center()
            .justify_center()
            .text_color(color)
            .cursor_pointer()
            .hover(|style| style.bg(colors.ghost_element_hover))
            .when(active, |tab| {
                tab.bg(colors.tab_active_background)
                    .border_b_2()
                    .border_color(colors.text_accent)
            })
            .on_click(cx.listener(move |view, _, _, cx| {
                view.app.click_tab(kind.clone());
                view.after_change(cx);
            }))
            .child(label)
    }
}

impl Render for TermView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let font = theme::theme_settings(cx).buffer_font(cx).clone();
        if self
            .metrics
            .is_none_or(|metrics| metrics.font_pt != self.app.font_pt)
        {
            self.metrics = Some(CellMetrics::measure(self.app.font_pt, &font, window));
        }
        window.set_window_title(&self.title());

        let colors = cx.theme().colors().clone();
        let status = cx.theme().status().clone();
        let message = self
            .app
            .message
            .as_ref()
            .filter(|(_, shown_at)| shown_at.elapsed() < MESSAGE_TTL)
            .map(|(text, _)| SharedString::from(text.clone()));
        let entity = cx.entity();

        v_flex()
            .relative()
            .size_full()
            .key_context("FerrumTerm")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::key_down))
            .bg(colors.terminal_background)
            .text_color(colors.terminal_foreground)
            .child(self.render_tab_bar(cx))
            .child(
                div()
                    .id("terminal")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
                    .on_scroll(cx.listener(Self::scroll_wheel))
                    .child(
                        canvas(
                            {
                                let entity = entity.clone();
                                move |bounds, _, cx| update_grid(&entity, bounds, cx)
                            },
                            move |bounds, _, window, cx| {
                                paint_terminal(&entity, &font, bounds, window, cx)
                            },
                        )
                        .size_full(),
                    ),
            )
            .when_some(message, |root, message| {
                root.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .h(px(28.))
                        .px(px(14.))
                        .flex()
                        .items_center()
                        .bg(status.warning_background)
                        .text_color(status.warning)
                        .font_ui(cx)
                        .text_size(px(13.))
                        .child(message),
                )
            })
    }
}

impl Focusable for TermView {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn update_grid(entity: &Entity<TermView>, bounds: Bounds<Pixels>, cx: &mut gpui::App) {
    let view = entity.read(cx);
    let Some(grid) = view.metrics.map(|metrics| metrics.grid(bounds.size)) else {
        return;
    };
    if view.grid != Some(grid) {
        entity.update(cx, |view, _| {
            view.grid = Some(grid);
            view.app.sync(Some(grid));
        });
    }
}

fn color(rgb: u32) -> Hsla {
    gpui::rgb(rgb).into()
}

struct Pane {
    block: BlockId,
    rect: Rect,
    frame: Option<Frame>,
}

fn paint_terminal(
    entity: &Entity<TermView>,
    font: &Font,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    let view = entity.read(cx);
    let focus_handle = view.focus_handle.clone();
    window.handle_input(
        &focus_handle,
        ElementInputHandler::new(bounds, entity.clone()),
        cx,
    );

    let view = entity.read(cx);
    let (Some(metrics), Some(info)) = (view.metrics, view.app.window()) else {
        return;
    };
    let area = Rect {
        x: 0,
        y: 0,
        cols: info.cols,
        rows: info.rows,
    };
    let focused_block = info.focused;
    let split = info.blocks.len() > 1;
    let panes: Vec<Pane> = info
        .layout
        .rects(area)
        .into_iter()
        .map(|(block, rect)| Pane {
            block,
            rect,
            frame: view.app.frames.get(&block).cloned(),
        })
        .collect();
    let ime_marked = view.ime_marked.clone();
    let theme = cx.theme();
    let colors = Colors {
        foreground: theme.colors().terminal_foreground,
        muted: theme.colors().text_muted,
        divider: theme.colors().border,
        accent: theme.colors().text_accent,
        background: theme.colors().terminal_background,
    };

    let cell_width = metrics.cell_width;
    let cell_height = metrics.cell_height;
    let origin = bounds.origin + point(px(PADDING), px(PADDING));
    let thin = (cell_height / 14.0).max(px(1.));
    let mut pane_hits = Vec::new();
    let mut cursor_bounds = None;

    window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
        for pane in &panes {
            let pane_origin = origin
                + point(
                    cell_width * f32::from(pane.rect.x),
                    cell_height * f32::from(pane.rect.y),
                );
            let pane_size = size(
                cell_width * f32::from(pane.rect.cols),
                cell_height * f32::from(pane.rect.rows),
            );
            pane_hits.push((
                pane.block,
                f32::from(pane_origin.x),
                f32::from(pane_origin.y),
                f32::from(pane_size.width),
                f32::from(pane_size.height),
            ));
            let focused = pane.block == focused_block;
            if let Some(frame) = &pane.frame {
                let pane_bounds = Bounds::new(pane_origin, pane_size);
                window.with_content_mask(Some(gpui::ContentMask { bounds: pane_bounds }), |window| {
                    for (row, line) in frame.lines.iter().enumerate() {
                        let row_origin = pane_origin + point(px(0.), cell_height * row as f32);
                        paint_backgrounds(line, row_origin, cell_width, cell_height, window);
                        paint_row(line, row_origin, &metrics, font, &colors, window, cx);
                    }
                    if let Some(cursor) = frame.cursor {
                        let caret = Bounds::new(
                            pane_origin
                                + point(
                                    cell_width * f32::from(cursor.col),
                                    cell_height * f32::from(cursor.row),
                                ),
                            size(cell_width, cell_height),
                        );
                        if focused {
                            cursor_bounds = Some(caret);
                        }
                        paint_cursor(cursor.shape, caret, focused, thin, &colors, window);
                        if focused && let Some(marked) = &ime_marked {
                            paint_marked_text(marked, caret, &metrics, font, &colors, window, cx);
                        }
                    }
                });
            }

            let divider = if focused && split {
                colors.accent
            } else {
                colors.divider
            };
            if pane.rect.x + pane.rect.cols < area.cols {
                window.paint_quad(fill(
                    Bounds::new(
                        pane_origin + point(pane_size.width + cell_width / 2., px(0.)),
                        size(px(1.), pane_size.height),
                    ),
                    divider,
                ));
            }
            if pane.rect.y + pane.rect.rows < area.rows {
                window.paint_quad(fill(
                    Bounds::new(
                        pane_origin + point(px(0.), pane_size.height + cell_height / 2.),
                        size(pane_size.width, px(1.)),
                    ),
                    divider,
                ));
            }
        }
    });

    entity.update(cx, |view, _| {
        view.app.panes = pane_hits;
        view.cursor_bounds = cursor_bounds;
    });
}

#[derive(Clone, Copy)]
struct Colors {
    foreground: Hsla,
    muted: Hsla,
    divider: Hsla,
    accent: Hsla,
    background: Hsla,
}

fn paint_backgrounds(
    line: &[Span],
    origin: Point<Pixels>,
    cell_width: Pixels,
    cell_height: Pixels,
    window: &mut Window,
) {
    let mut col = 0usize;
    for span in line {
        let width = UnicodeWidthStr::width(span.t.as_str());
        if let Some(background) = span.bg {
            window.paint_quad(fill(
                Bounds::new(
                    origin + point(cell_width * col as f32, px(0.)),
                    size(cell_width * width as f32, cell_height),
                ),
                color(background),
            ));
        }
        col += width;
    }
}

fn span_run(span: &Span, len: usize, font: &Font, colors: &Colors, thin: Pixels) -> TextRun {
    let mut font = font.clone();
    if span.f & flags::BOLD != 0 {
        font.weight = FontWeight::BOLD;
    }
    if span.f & flags::ITALIC != 0 {
        font.style = FontStyle::Italic;
    }
    let mut foreground = span.fg.map_or(colors.foreground, color);
    if span.f & flags::DIM != 0 {
        foreground = foreground.opacity(150.0 / 255.0);
    }
    TextRun {
        len,
        font,
        color: foreground,
        background_color: None,
        underline: (span.f & flags::UNDERLINE != 0).then_some(UnderlineStyle {
            color: Some(foreground),
            thickness: thin,
            wavy: false,
        }),
        strikethrough: (span.f & flags::STRIKE != 0).then_some(StrikethroughStyle {
            color: Some(foreground),
            thickness: thin,
        }),
    }
}

/// Paints one row. Narrow characters are shaped together and snapped to the
/// cell grid; wide characters (CJK, emoji) are painted on their own so they
/// take two cells.
fn paint_row(
    line: &[Span],
    origin: Point<Pixels>,
    metrics: &CellMetrics,
    font: &Font,
    colors: &Colors,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    let thin = (metrics.cell_height / 14.0).max(px(1.));
    let mut segment = Segment::default();
    let mut col = 0usize;
    for (span_index, span) in line.iter().enumerate() {
        for character in span.t.chars() {
            let width = UnicodeWidthChar::width(character).unwrap_or(0);
            if width >= 2 {
                segment.paint(origin, metrics, window, cx);
                let mut wide = Segment {
                    col,
                    ..Segment::default()
                };
                wide.push(character, span_index, span, font, colors, thin);
                wide.paint_unforced(origin, metrics, window, cx);
                col += width;
                segment.col = col;
                continue;
            }
            if segment.text.is_empty() {
                segment.col = col;
            }
            segment.push(character, span_index, span, font, colors, thin);
            col += width;
        }
    }
    segment.paint(origin, metrics, window, cx);
}

#[derive(Default)]
struct Segment {
    col: usize,
    text: String,
    runs: Vec<TextRun>,
    last_span: Option<usize>,
    decorated: bool,
}

impl Segment {
    fn push(
        &mut self,
        character: char,
        span_index: usize,
        span: &Span,
        font: &Font,
        colors: &Colors,
        thin: Pixels,
    ) {
        self.text.push(character);
        let len = character.len_utf8();
        match self.runs.last_mut() {
            Some(run) if self.last_span == Some(span_index) => run.len += len,
            _ => {
                let run = span_run(span, len, font, colors, thin);
                self.decorated |= run.underline.is_some() || run.strikethrough.is_some();
                self.runs.push(run);
                self.last_span = Some(span_index);
            }
        }
    }

    fn paint(
        &mut self,
        origin: Point<Pixels>,
        metrics: &CellMetrics,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        self.paint_with(origin, metrics, Some(metrics.cell_width), window, cx);
    }

    fn paint_unforced(
        &mut self,
        origin: Point<Pixels>,
        metrics: &CellMetrics,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        self.paint_with(origin, metrics, None, window, cx);
    }

    fn paint_with(
        &mut self,
        origin: Point<Pixels>,
        metrics: &CellMetrics,
        force_width: Option<Pixels>,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let text = std::mem::take(&mut self.text);
        let runs = std::mem::take(&mut self.runs);
        self.last_span = None;
        let decorated = std::mem::take(&mut self.decorated);
        if text.is_empty() || (!decorated && text.chars().all(|character| character == ' ')) {
            return;
        }
        let shaped = window.text_system().shape_line(
            text.into(),
            metrics.font_size,
            &runs,
            force_width,
        );
        let position = origin + point(metrics.cell_width * self.col as f32, px(0.));
        if let Err(error) = shaped.paint(
            position,
            metrics.cell_height,
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        ) {
            tracing::warn!(%error, "failed to paint terminal text");
        }
    }
}

fn paint_cursor(
    shape: CursorShape,
    caret: Bounds<Pixels>,
    focused: bool,
    thin: Pixels,
    colors: &Colors,
    window: &mut Window,
) {
    let cell = caret.size;
    if !focused {
        // Hollow box when the pane is not focused.
        let muted = colors.muted.opacity(0.9);
        for (offset, extent) in [
            (point(px(0.), px(0.)), size(cell.width, thin)),
            (point(px(0.), cell.height - thin), size(cell.width, thin)),
            (point(px(0.), px(0.)), size(thin, cell.height)),
            (point(cell.width - thin, px(0.)), size(thin, cell.height)),
        ] {
            window.paint_quad(fill(Bounds::new(caret.origin + offset, extent), muted));
        }
        return;
    }
    let beam = (cell.width / 6.0).max(px(2.));
    let quad = match shape {
        CursorShape::Block => fill(caret, colors.foreground.opacity(0.55)),
        CursorShape::Beam => fill(
            Bounds::new(caret.origin, size(beam, cell.height)),
            colors.foreground,
        ),
        CursorShape::Underline => fill(
            Bounds::new(
                caret.origin + point(px(0.), cell.height - beam),
                size(cell.width, beam),
            ),
            colors.foreground,
        ),
    };
    window.paint_quad(quad);
}

fn paint_marked_text(
    marked: &str,
    caret: Bounds<Pixels>,
    metrics: &CellMetrics,
    font: &Font,
    colors: &Colors,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    let run = TextRun {
        len: marked.len(),
        font: font.clone(),
        color: colors.foreground,
        background_color: Some(colors.background),
        underline: Some(UnderlineStyle {
            color: Some(colors.foreground),
            thickness: px(1.),
            wavy: false,
        }),
        strikethrough: None,
    };
    let shaped = window.text_system().shape_line(
        marked.to_owned().into(),
        metrics.font_size,
        &[run],
        None,
    );
    if let Err(error) = shaped.paint(
        caret.origin,
        metrics.cell_height,
        gpui::TextAlign::Left,
        None,
        window,
        cx,
    ) {
        tracing::warn!(%error, "failed to paint IME composition");
    }
}

/// Label for a tab: window name, else the focused block's title / command.
fn tab_label(w: &WindowInfo) -> String {
    let b = w
        .blocks
        .iter()
        .find(|b| b.id == w.focused)
        .or(w.blocks.first());
    let title = b.map(|b| {
        b.label
            .clone()
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| {
                if b.title.is_empty() {
                    b.command.first().map_or_else(
                        || "shell".into(),
                        |c| c.rsplit('/').next().unwrap_or(c).to_owned(),
                    )
                } else {
                    b.title.clone()
                }
            })
    });
    let name = if w.name.chars().all(|c| c.is_ascii_digit()) {
        title.unwrap_or_default()
    } else {
        w.name.clone()
    };
    let status = w
        .blocks
        .iter()
        .filter_map(|b| b.status.as_ref().map(|s| s.state))
        .max_by_key(|s| match s {
            ProgramState::Blocked => 4,
            ProgramState::Error => 3,
            ProgramState::Working => 2,
            ProgramState::Done => 1,
            ProgramState::Idle => 0,
        });
    let mut label: String = name.chars().take(28).collect();
    if let Some(s) = status {
        label = format!("{} {label}", s.glyph());
    }
    label
}

impl gpui::EntityInputHandler for TermView {
    fn text_for_range(
        &mut self,
        _: Range<usize>,
        _: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let end = self
            .ime_marked
            .as_ref()
            .map_or(0, |marked| marked.encode_utf16().count());
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.ime_marked
            .as_ref()
            .map(|marked| 0..marked.encode_utf16().count())
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.ime_marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ime_marked = None;
        if text.is_empty() {
            cx.notify();
            return;
        }
        self.act(
            Action::Send(ferrumterm_protocol::Input::Text(text.to_owned())),
            cx,
        );
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ime_marked = (!text.is_empty()).then(|| text.to_owned());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        self.cursor_bounds
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
