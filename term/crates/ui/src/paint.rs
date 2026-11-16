//! Painting a window's panes cell by cell from server frames: backgrounds,
//! selection, text shaped per row and snapped to the cell grid, cursor, IME
//! composition, and dividers between split panes.

use crate::view::{PADDING, TermView};
use ferrum_ui::gpui::{
    self, Bounds, ElementInputHandler, Entity, Font, FontStyle, FontWeight, Hsla, Pixels, Point,
    Size, StrikethroughStyle, TextRun, UnderlineStyle, Window, fill, point, px, size,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumterm_protocol::{BlockId, CursorShape, Frame, Rect, Span, flags};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CellMetrics {
    pub font_pt: f32,
    pub font_size: Pixels,
    pub cell_width: Pixels,
    pub cell_height: Pixels,
}

impl CellMetrics {
    pub(crate) fn measure(font_pt: f32, font: &Font, window: &Window) -> Self {
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
    pub(crate) fn grid(&self, area: Size<Pixels>) -> (u16, u16) {
        let padding = px(PADDING) * 2.;
        let cols = ((area.width - padding) / self.cell_width).floor().max(4.0);
        let rows = ((area.height - padding) / self.cell_height)
            .floor()
            .max(2.0);
        (cols as u16, rows as u16)
    }
}

pub(crate) fn update_grid(entity: &Entity<TermView>, bounds: Bounds<Pixels>, cx: &mut gpui::App) {
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

pub(crate) fn paint_terminal(
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
    let cursor_on = view.cursor_on;
    let settings = crate::settings::get();
    let selection = view.selection.clone().filter(|s| !s.is_empty());
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
                window.with_content_mask(
                    Some(gpui::ContentMask {
                        bounds: pane_bounds,
                    }),
                    |window| {
                        for (row, line) in frame.lines.iter().enumerate() {
                            let row_origin = pane_origin + point(px(0.), cell_height * row as f32);
                            paint_backgrounds(line, row_origin, cell_width, cell_height, window);
                            if let Some((from, to)) = selection
                                .as_ref()
                                .filter(|s| s.block == pane.block)
                                .and_then(|s| {
                                    s.columns(
                                        crate::selection::line_of(frame, row as u16),
                                        pane.rect.cols,
                                    )
                                })
                            {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        row_origin + point(cell_width * f32::from(from), px(0.)),
                                        size(cell_width * f32::from(to - from), cell_height),
                                    ),
                                    colors.accent.opacity(0.3),
                                ));
                            }
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
                            if !focused || cursor_on {
                                let shape = settings.cursor_shape(cursor.shape);
                                paint_cursor(shape, caret, focused, thin, &colors, window);
                            }
                            if focused && let Some(marked) = &ime_marked {
                                paint_marked_text(
                                    marked, caret, &metrics, font, &colors, window, cx,
                                );
                            }
                        }
                    },
                );
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
        let shaped =
            window
                .text_system()
                .shape_line(text.into(), metrics.font_size, &runs, force_width);
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
    let shaped =
        window
            .text_system()
            .shape_line(marked.to_owned().into(), metrics.font_size, &[run], None);
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
