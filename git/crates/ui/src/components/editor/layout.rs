//! Per-frame layout of the visible rows (shaped with GPUI's text system),
//! the mapping between buffer offsets and window coordinates, and the
//! syntax-coloured text runs of a row. Follows FerrumIDE's editor layout.

use std::ops::Range;

use ferrumeditor_core::buffer::Buffer;
use ferrumeditor_core::display::ExpandedLine;
use ferrumeditor_core::highlight::HighlightSpan;
use ferrumeditor_core::theme::{Color, Theme};
use gpui::{
    Bounds, Font, FontStyle, FontWeight, Hsla, Pixels, Point, Rgba, ShapedLine, TextRun,
    UnderlineStyle, px,
};

/// A core theme colour as GPUI colour.
pub fn hsla(color: Color) -> Hsla {
    Rgba {
        r: f32::from(color.r) / 255.0,
        g: f32::from(color.g) / 255.0,
        b: f32::from(color.b) / 255.0,
        a: f32::from(color.a) / 255.0,
    }
    .into()
}

/// One visible row (one buffer line: the view does not soft-wrap).
#[derive(Clone)]
pub struct RowLayout {
    pub row: usize,
    /// Buffer offset of the start of the line.
    pub line_start: usize,
    /// Byte length of the line (without its newline).
    pub line_len: usize,
    pub expanded: ExpandedLine,
    pub shaped: ShapedLine,
    pub top: Pixels,
}

impl RowLayout {
    pub fn contains_offset(&self, offset: usize) -> bool {
        offset >= self.line_start && offset <= self.line_start + self.line_len
    }

    /// X (relative to the text origin, unscrolled) of a buffer offset in
    /// this row; clamped to the row.
    pub fn x_for_offset(&self, offset: usize) -> Pixels {
        let col = offset.saturating_sub(self.line_start).min(self.line_len);
        self.shaped.x_for_index(self.expanded.to_display(col))
    }

    pub fn offset_for_x(&self, x: Pixels) -> usize {
        let index = self.shaped.closest_index_for_x(x.max(px(0.)));
        self.line_start + self.expanded.to_buffer(index).min(self.line_len)
    }
}

/// What the last paint looked like; used for mouse hit-testing, IME
/// bounds and page sizes.
#[derive(Clone)]
pub struct LayoutSnapshot {
    pub bounds: Bounds<Pixels>,
    pub text_left: Pixels,
    pub gutter_width: Pixels,
    pub line_height: Pixels,
    pub char_width: Pixels,
    pub scroll_y: Pixels,
    pub scroll_x: Pixels,
    pub rows: Vec<RowLayout>,
}

impl LayoutSnapshot {
    pub fn row_at(&self, y: Pixels) -> isize {
        ((y - self.bounds.top() + self.scroll_y) / self.line_height).floor() as isize
    }

    /// Buffer offset under a window position (rows off screen are
    /// estimated with the cell width).
    pub fn offset_at(&self, position: Point<Pixels>, buffer: &Buffer, tab_size: usize) -> usize {
        let row = self.row_at(position.y);
        if row < 0 {
            return 0;
        }
        let x = position.x - self.text_left + self.scroll_x;
        if let Some(layout) = self.rows.iter().find(|r| r.row == row as usize) {
            return layout.offset_for_x(x);
        }
        let row = row as usize;
        if row >= buffer.line_count() {
            return buffer.len();
        }
        let cells = (x / self.char_width).round().max(0.) as usize;
        let line = buffer.line(row);
        buffer.line_start(row)
            + ferrumeditor_core::display::byte_col_for_visual(&line, cells, tab_size)
    }

    pub fn row_for_offset(&self, offset: usize) -> Option<&RowLayout> {
        self.rows.iter().find(|r| r.contains_offset(offset))
    }

    /// Window-space caret position of `offset` if it is on screen.
    pub fn point_for_offset(&self, offset: usize) -> Option<Point<Pixels>> {
        let row = self.row_for_offset(offset)?;
        Some(Point::new(
            self.text_left + row.x_for_offset(offset) - self.scroll_x,
            row.top,
        ))
    }
}

/// Colour, weight and style of one highlight name.
fn style_for(theme: &Theme, name: &str, base: Hsla) -> (Hsla, FontWeight, FontStyle) {
    let color = theme.syntax(name).map_or(base, hsla);
    let weight = if matches!(
        name,
        "markup.bold" | "text.strong" | "markup.heading" | "text.title"
    ) {
        FontWeight::BOLD
    } else {
        FontWeight::NORMAL
    };
    let style = if matches!(name, "markup.italic" | "text.emphasis") {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };
    (color, weight, style)
}

pub struct RunInput<'a> {
    pub theme: &'a Theme,
    pub font: &'a Font,
    pub text_color: Hsla,
    pub highlights: &'a [HighlightSpan],
    pub line_start: usize,
    pub line_len: usize,
    pub expanded: &'a ExpandedLine,
    /// IME composition (buffer offsets), underlined.
    pub marked: Option<Range<usize>>,
}

/// Text runs covering the row's display text exactly.
pub fn build_runs(input: &RunInput, display_len: usize) -> Vec<TextRun> {
    let base = TextRun {
        len: 0,
        font: input.font.clone(),
        color: input.text_color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let (row_start, row_end) = (input.line_start, input.line_start + input.line_len);
    let to_display = |offset: usize| {
        input
            .expanded
            .to_display(offset.saturating_sub(row_start))
            .min(display_len)
    };
    let first = input
        .highlights
        .partition_point(|s| s.range.end <= row_start);
    let mut runs: Vec<TextRun> = Vec::new();
    let mut covered = 0;
    let mut push = |end: usize, span: Option<&HighlightSpan>, runs: &mut Vec<TextRun>| {
        if end <= covered {
            return;
        }
        let mut run = base.clone();
        if let Some(span) = span {
            let (color, weight, style) = style_for(input.theme, span.name(), input.text_color);
            run.color = color;
            run.font.weight = weight;
            run.font.style = style;
        }
        run.len = end - covered;
        covered = end;
        runs.push(run);
    };
    for span in &input.highlights[first..] {
        if span.range.start >= row_end {
            break;
        }
        push(to_display(span.range.start), None, &mut runs);
        push(
            to_display(span.range.end.min(row_end)),
            Some(span),
            &mut runs,
        );
    }
    push(display_len, None, &mut runs);
    if let Some(marked) = &input.marked {
        runs = underline_range(
            runs,
            to_display(marked.start)..to_display(marked.end),
            UnderlineStyle {
                color: Some(input.text_color),
                thickness: px(1.),
                wavy: false,
            },
        );
    }
    if runs.is_empty() {
        runs.push(TextRun {
            len: display_len,
            ..base
        });
    }
    runs
}

/// Splits runs so `range` (display bytes) is underlined.
fn underline_range(runs: Vec<TextRun>, range: Range<usize>, style: UnderlineStyle) -> Vec<TextRun> {
    if range.is_empty() {
        return runs;
    }
    let mut out = Vec::new();
    let mut pos = 0;
    for run in runs {
        let (start, end) = (pos, pos + run.len);
        pos = end;
        let cuts = [
            start,
            range.start.clamp(start, end),
            range.end.clamp(start, end),
            end,
        ];
        for pair in cuts.windows(2) {
            if pair[1] > pair[0] {
                let mut piece = run.clone();
                piece.len = pair[1] - pair[0];
                if pair[0] >= range.start && pair[1] <= range.end {
                    piece.underline = Some(style);
                }
                out.push(piece);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumeditor_core::language::Language;

    #[test]
    fn runs_cover_the_row_and_follow_the_highlights() {
        let theme = Theme::dark();
        let font = gpui::font("Menlo");
        let text = "fn main() {}";
        let spans = ferrumeditor_core::highlight::highlight(Language::Rust, text, None);
        assert!(!spans.is_empty(), "rust is highlighted");
        let expanded = ExpandedLine::new(text, 4);
        let runs = build_runs(
            &RunInput {
                theme: &theme,
                font: &font,
                text_color: gpui::white(),
                highlights: &spans,
                line_start: 0,
                line_len: text.len(),
                expanded: &expanded,
                marked: Some(3..7),
            },
            expanded.text.len(),
        );
        assert_eq!(runs.iter().map(|r| r.len).sum::<usize>(), text.len());
        // The keyword is coloured, `main` (the composition) underlined.
        assert_ne!(runs[0].color, gpui::white());
        assert!(runs.iter().any(|r| r.underline.is_some()));
    }
}
