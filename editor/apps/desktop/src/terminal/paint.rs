//! Paints an emulator [`Frame`] cell by cell: backgrounds, selection,
//! text (narrow characters shaped together and snapped to the grid, wide
//! ones on their own) and the cursor. The emulator's default ANSI palette
//! is mapped onto the editor theme's `[terminal]` colours.

use std::collections::HashMap;

use ferrumeditor_core::theme::Theme;
use ferrumterm_protocol::{CursorShape, Frame, Span, flags};
use gpui::{
    App, Bounds, Font, FontStyle, FontWeight, Hsla, Pixels, Point, StrikethroughStyle, TextRun,
    UnderlineStyle, Window, fill, point, px, size,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::selection::Selection;
use crate::theme_apply::hsla;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub font_size: Pixels,
    pub cell_width: Pixels,
    pub cell_height: Pixels,
}

impl Metrics {
    pub fn measure(font: &Font, font_size: Pixels, window: &Window) -> Self {
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(font);
        let cell_width = text_system
            .advance(font_id, font_size, 'M')
            .map(|a| a.width)
            .unwrap_or(font_size * 0.6);
        Self {
            font_size,
            cell_width: cell_width.max(px(1.)),
            cell_height: (font_size * 1.3).round(),
        }
    }

    /// Cells that fit in `area` (minus padding).
    pub fn grid(&self, area: gpui::Size<Pixels>, padding: Pixels) -> (u16, u16) {
        let cols = ((area.width - padding * 2.) / self.cell_width)
            .floor()
            .max(4.0);
        let rows = ((area.height - padding * 2.) / self.cell_height)
            .floor()
            .max(2.0);
        (cols as u16, rows as u16)
    }
}

/// Theme colours for the terminal.
pub struct Palette {
    pub foreground: Hsla,
    pub background: Hsla,
    pub cursor: Hsla,
    pub selection: Hsla,
    map: HashMap<u32, Hsla>,
}

impl Palette {
    pub fn from_theme(theme: &Theme) -> Self {
        let ansi = theme.ansi();
        let mut map: HashMap<u32, Hsla> = ferrumterm_emulator::ANSI
            .iter()
            .zip(ansi)
            .map(|(default, ours)| (*default, hsla(ours)))
            .collect();
        map.insert(
            ferrumterm_emulator::DEFAULT_FG,
            hsla(theme.terminal("foreground")),
        );
        map.insert(
            ferrumterm_emulator::DEFAULT_BG,
            hsla(theme.terminal("background")),
        );
        Self {
            foreground: hsla(theme.terminal("foreground")),
            background: hsla(theme.terminal("background")),
            cursor: hsla(theme.terminal("cursor")),
            selection: hsla(theme.terminal("selection")),
            map,
        }
    }

    pub fn color(&self, rgb: u32) -> Hsla {
        self.map
            .get(&rgb)
            .copied()
            .unwrap_or_else(|| gpui::rgb(rgb).into())
    }
}

pub struct PaintArgs<'a> {
    pub frame: &'a Frame,
    pub origin: Point<Pixels>,
    pub metrics: Metrics,
    pub font: &'a Font,
    pub palette: &'a Palette,
    pub selection: Option<&'a Selection>,
    pub focused: bool,
    pub ime_marked: Option<&'a str>,
}

/// Paints the frame; returns the caret bounds (for IME popups).
pub fn paint(args: &PaintArgs<'_>, window: &mut Window, cx: &mut App) -> Option<Bounds<Pixels>> {
    let m = args.metrics;
    for (row, line) in args.frame.lines.iter().enumerate() {
        let row_origin = args.origin + point(px(0.), m.cell_height * row as f32);
        paint_backgrounds(line, row_origin, &m, args.palette, window);
        if let Some((from, to)) = args
            .selection
            .and_then(|s| s.columns(row as u16, args.frame.cols))
        {
            window.paint_quad(fill(
                Bounds::new(
                    row_origin + point(m.cell_width * f32::from(from), px(0.)),
                    size(m.cell_width * f32::from(to - from), m.cell_height),
                ),
                args.palette.selection,
            ));
        }
        paint_row(line, row_origin, args, window, cx);
    }
    let cursor = args.frame.cursor?;
    let caret = Bounds::new(
        args.origin
            + point(
                m.cell_width * f32::from(cursor.col),
                m.cell_height * f32::from(cursor.row),
            ),
        size(m.cell_width, m.cell_height),
    );
    paint_cursor(cursor.shape, caret, args, window);
    if let Some(marked) = args.ime_marked.filter(|_| args.focused) {
        let run = TextRun {
            len: marked.len(),
            font: args.font.clone(),
            color: args.palette.foreground,
            background_color: Some(args.palette.background),
            underline: Some(UnderlineStyle {
                color: Some(args.palette.foreground),
                thickness: px(1.),
                wavy: false,
            }),
            strikethrough: None,
        };
        let shaped =
            window
                .text_system()
                .shape_line(marked.to_owned().into(), m.font_size, &[run], None);
        let _ = shaped.paint(
            caret.origin,
            m.cell_height,
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        );
    }
    Some(caret)
}

fn paint_backgrounds(
    line: &[Span],
    origin: Point<Pixels>,
    m: &Metrics,
    palette: &Palette,
    window: &mut Window,
) {
    let mut col = 0usize;
    for span in line {
        let width = UnicodeWidthStr::width(span.t.as_str());
        if let Some(background) = span.bg {
            window.paint_quad(fill(
                Bounds::new(
                    origin + point(m.cell_width * col as f32, px(0.)),
                    size(m.cell_width * width as f32, m.cell_height),
                ),
                palette.color(background),
            ));
        }
        col += width;
    }
}

fn span_run(span: &Span, len: usize, args: &PaintArgs<'_>) -> TextRun {
    let mut font = args.font.clone();
    if span.f & flags::BOLD != 0 {
        font.weight = FontWeight::BOLD;
    }
    if span.f & flags::ITALIC != 0 {
        font.style = FontStyle::Italic;
    }
    let mut color = span
        .fg
        .map_or(args.palette.foreground, |c| args.palette.color(c));
    if span.f & flags::DIM != 0 {
        color = color.opacity(0.6);
    }
    let thin = (args.metrics.cell_height / 14.0).max(px(1.));
    TextRun {
        len,
        font,
        color,
        background_color: None,
        underline: (span.f & flags::UNDERLINE != 0).then_some(UnderlineStyle {
            color: Some(color),
            thickness: thin,
            wavy: false,
        }),
        strikethrough: (span.f & flags::STRIKE != 0).then_some(StrikethroughStyle {
            color: Some(color),
            thickness: thin,
        }),
    }
}

/// A run of narrow characters starting at `col`, shaped in one go.
#[derive(Default)]
struct Segment {
    col: usize,
    text: String,
    runs: Vec<TextRun>,
    last_span: Option<usize>,
    decorated: bool,
}

impl Segment {
    fn push(&mut self, c: char, span_index: usize, span: &Span, args: &PaintArgs<'_>) {
        self.text.push(c);
        match self.runs.last_mut() {
            Some(run) if self.last_span == Some(span_index) => run.len += c.len_utf8(),
            _ => {
                let run = span_run(span, c.len_utf8(), args);
                self.decorated |= run.underline.is_some() || run.strikethrough.is_some();
                self.runs.push(run);
                self.last_span = Some(span_index);
            }
        }
    }

    fn paint(
        &mut self,
        origin: Point<Pixels>,
        m: &Metrics,
        snap: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let text = std::mem::take(&mut self.text);
        let runs = std::mem::take(&mut self.runs);
        self.last_span = None;
        let decorated = std::mem::take(&mut self.decorated);
        if text.is_empty() || (!decorated && text.chars().all(|c| c == ' ')) {
            return;
        }
        let width = snap.then_some(m.cell_width);
        let shaped = window
            .text_system()
            .shape_line(text.into(), m.font_size, &runs, width);
        let position = origin + point(m.cell_width * self.col as f32, px(0.));
        let _ = shaped.paint(
            position,
            m.cell_height,
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}

fn paint_row(
    line: &[Span],
    origin: Point<Pixels>,
    args: &PaintArgs<'_>,
    window: &mut Window,
    cx: &mut App,
) {
    let m = args.metrics;
    let mut segment = Segment::default();
    let mut col = 0usize;
    for (span_index, span) in line.iter().enumerate() {
        for c in span.t.chars() {
            let width = UnicodeWidthChar::width(c).unwrap_or(0);
            if width >= 2 {
                segment.paint(origin, &m, true, window, cx);
                let mut wide = Segment {
                    col,
                    ..Segment::default()
                };
                wide.push(c, span_index, span, args);
                wide.paint(origin, &m, false, window, cx);
                col += width;
                segment.col = col;
                continue;
            }
            if segment.text.is_empty() {
                segment.col = col;
            }
            segment.push(c, span_index, span, args);
            col += width;
        }
    }
    segment.paint(origin, &m, true, window, cx);
}

fn paint_cursor(
    shape: CursorShape,
    caret: Bounds<Pixels>,
    args: &PaintArgs<'_>,
    window: &mut Window,
) {
    let cell = caret.size;
    let color = args.palette.cursor;
    let thin = (cell.height / 14.0).max(px(1.));
    if !args.focused {
        for (offset, extent) in [
            (point(px(0.), px(0.)), size(cell.width, thin)),
            (point(px(0.), cell.height - thin), size(cell.width, thin)),
            (point(px(0.), px(0.)), size(thin, cell.height)),
            (point(cell.width - thin, px(0.)), size(thin, cell.height)),
        ] {
            window.paint_quad(fill(Bounds::new(caret.origin + offset, extent), color));
        }
        return;
    }
    let beam = (cell.width / 6.0).max(px(2.));
    let quad = match shape {
        CursorShape::Block => fill(caret, color.opacity(0.6)),
        CursorShape::Beam => fill(Bounds::new(caret.origin, size(beam, cell.height)), color),
        CursorShape::Underline => fill(
            Bounds::new(
                caret.origin + point(px(0.), cell.height - beam),
                size(cell.width, beam),
            ),
            color,
        ),
    };
    window.paint_quad(quad);
}
