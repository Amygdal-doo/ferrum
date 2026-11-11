//! Per-frame layout of the visible rows (shaped with GPUI's text system)
//! and the mapping between buffer offsets and window coordinates.

use std::ops::Range;

use ferrumeditor_core::display::{DisplayMap, DisplayPoint, ExpandedLine};
use ferrumeditor_core::editor::EditorState;
use ferrumeditor_core::highlight::HighlightSpan;
use ferrumeditor_core::theme::Theme;
use gpui::{
    Bounds, Font, FontStyle, FontWeight, Hsla, Pixels, Point, ShapedLine, TextRun, UnderlineStyle,
    px,
};

use crate::theme_apply::hsla;

/// One visible display row.
#[derive(Clone)]
pub struct RowLayout {
    pub display_row: usize,
    pub buffer_row: usize,
    /// Buffer offset of the start of the buffer line.
    pub line_start: usize,
    /// Byte columns of the line shown on this row.
    pub segment: Range<usize>,
    /// Whether the segment ends the buffer line.
    pub last_segment: bool,
    pub expanded: ExpandedLine,
    /// Display byte where the segment starts within `expanded.text`.
    pub display_start: usize,
    pub shaped: ShapedLine,
    pub top: Pixels,
}

impl RowLayout {
    pub fn contains_offset(&self, offset: usize) -> bool {
        let start = self.line_start + self.segment.start;
        let end = self.line_start + self.segment.end;
        offset >= start && (offset < end || (offset == end && self.last_segment))
    }

    /// X (relative to the text origin, unscrolled) of a buffer offset in
    /// this row; clamped to the row.
    pub fn x_for_offset(&self, offset: usize) -> Pixels {
        let col = offset
            .saturating_sub(self.line_start)
            .clamp(self.segment.start, self.segment.end);
        let display = self
            .expanded
            .to_display(col)
            .saturating_sub(self.display_start);
        self.shaped.x_for_index(display)
    }

    pub fn offset_for_x(&self, x: Pixels) -> usize {
        let index = self.shaped.closest_index_for_x(x.max(px(0.)));
        let col = self.expanded.to_buffer(self.display_start + index);
        self.line_start + col.clamp(self.segment.start, self.segment.end)
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
    pub first_row: usize,
    pub rows: Vec<RowLayout>,
}

impl LayoutSnapshot {
    pub fn display_row_at(&self, y: Pixels) -> isize {
        ((y - self.bounds.top() + self.scroll_y) / self.line_height).floor() as isize
    }

    /// Buffer offset under a window position (rows off screen are
    /// estimated with the cell width).
    pub fn offset_at(
        &self,
        position: Point<Pixels>,
        state: &EditorState,
        map: &DisplayMap,
    ) -> usize {
        let row = self.display_row_at(position.y);
        if row < 0 {
            return 0;
        }
        let x = position.x - self.text_left + self.scroll_x;
        if let Some(layout) = self.rows.iter().find(|r| r.display_row == row as usize) {
            return layout.offset_for_x(x);
        }
        if row as usize >= map.row_count() {
            return state.buffer().len();
        }
        let col = (x / self.char_width).round().max(0.) as usize;
        map.to_offset(
            state.buffer(),
            DisplayPoint {
                row: row as usize,
                col,
            },
        )
    }

    /// Display point (row, cell column) under a position, for column selection.
    pub fn display_point_at(&self, position: Point<Pixels>) -> DisplayPoint {
        let row = self.display_row_at(position.y).max(0) as usize;
        let x = position.x - self.text_left + self.scroll_x;
        DisplayPoint {
            row,
            col: (x / self.char_width).round().max(0.) as usize,
        }
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

/// Style of one highlight name.
pub fn style_for(theme: &Theme, name: &str, base: Hsla) -> (Hsla, FontWeight, FontStyle) {
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
    /// Buffer offset of the row's first byte and the row's text length.
    pub row_start: usize,
    pub row_end: usize,
    pub line_start: usize,
    pub expanded: &'a ExpandedLine,
    pub display_start: usize,
    pub marked: Option<Range<usize>>,
    /// Problem ranges (buffer offsets) drawn with a wavy underline.
    pub squiggles: &'a [(Range<usize>, Hsla)],
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
    let to_display = |offset: usize| {
        let col = offset.saturating_sub(input.line_start);
        input
            .expanded
            .to_display(col)
            .saturating_sub(input.display_start)
            .min(display_len)
    };
    // Boundaries where the style may change.
    let first = input
        .highlights
        .partition_point(|s| s.range.end <= input.row_start);
    let mut styled: Vec<(Range<usize>, Option<&HighlightSpan>)> = Vec::new();
    let mut pos = input.row_start;
    for span in &input.highlights[first..] {
        if span.range.start >= input.row_end {
            break;
        }
        let start = span.range.start.max(input.row_start);
        let end = span.range.end.min(input.row_end);
        if start > pos {
            styled.push((pos..start, None));
        }
        if end > start.max(pos) {
            styled.push((start.max(pos)..end, Some(span)));
            pos = end;
        }
    }
    if pos < input.row_end || styled.is_empty() {
        styled.push((pos..input.row_end, None));
    }
    // `styled` is contiguous from row_start to row_end, so each run starts
    // where the previous one ended.
    let mut runs: Vec<TextRun> = Vec::new();
    let mut covered = 0;
    for (range, span) in styled {
        let end = if range.end >= input.row_end {
            display_len
        } else {
            to_display(range.end)
        };
        if end <= covered {
            continue;
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
    }
    if covered < display_len {
        let mut run = base.clone();
        run.len = display_len - covered;
        runs.push(run);
    }
    for (range, color) in input.squiggles {
        if range.end <= input.row_start || range.start >= input.row_end {
            continue;
        }
        let start = range.start.max(input.row_start);
        let end = range.end.min(input.row_end);
        runs = underline_range(
            runs,
            to_display(start)..to_display(end),
            UnderlineStyle {
                color: Some(*color),
                thickness: px(1.),
                wavy: true,
            },
        );
    }
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
    runs.retain(|r| r.len > 0);
    if runs.is_empty() {
        runs.push(TextRun {
            len: display_len,
            ..base
        });
    }
    runs
}

/// Splits runs so `range` (display bytes) is underlined (IME composition,
/// problems).
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
