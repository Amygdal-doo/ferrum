//! Per-frame decorations: current line, find matches, selections, bracket
//! match, cursors and line numbers.

use gpui::{
    BorderStyle, Bounds, Hsla, SharedString, TextRun, Window, fill, outline, point, px, size,
};

use super::CodeEditor;
use super::element::{Frame, FrameStyle};
use super::layout::{RowLayout, hsla};

impl CodeEditor {
    /// Current line, find matches, selections, bracket match and cursors.
    pub(super) fn decorate(&self, frame: &mut Frame, current_line: Hsla) {
        let layout = &frame.layout;
        let buffer = self.state.buffer();
        let lh = layout.line_height;
        let x_of = |row: &RowLayout, offset: usize| {
            layout.text_left + row.x_for_offset(offset) - layout.scroll_x
        };
        let selections = self.state.selections();
        if selections.iter().all(|s| s.is_empty()) {
            for row in &layout.rows {
                if selections.iter().any(|s| buffer.row_of(s.head) == row.row) {
                    let origin = point(frame.text_area.left(), row.top);
                    let line = Bounds::new(origin, size(frame.text_area.size.width, lh));
                    frame.backgrounds.push(fill(line, current_line));
                }
            }
        }
        // `newline`: a selection running past the row's end shows its newline.
        let mut spans = |range: std::ops::Range<usize>, color: Hsla, newline: bool| {
            for row in &layout.rows {
                let (start, end) = (row.line_start, row.line_start + row.line_len);
                let (s, e) = (range.start.max(start), range.end.min(end));
                let past_end = newline && range.end > end && range.start <= end;
                if s > e || (s == e && !past_end) {
                    continue;
                }
                let mut x1 = x_of(row, e);
                if past_end {
                    x1 += layout.char_width;
                }
                let corners = (point(x_of(row, s), row.top), point(x1, row.top + lh));
                frame
                    .backgrounds
                    .push(fill(Bounds::from_corners(corners.0, corners.1), color));
            }
        };
        if let (Some(first), Some(last)) = (layout.rows.first(), layout.rows.last()) {
            let (from, to) = (first.line_start, last.line_start + last.line_len);
            let normal = hsla(self.theme.ui("find_match"));
            let strong = hsla(self.theme.ui("find_match_active"));
            for (i, m) in self.find.matches.iter().enumerate() {
                if m.end >= from && m.start <= to {
                    let active = self.find.active == Some(i);
                    spans(m.clone(), if active { strong } else { normal }, false);
                }
            }
        }
        let selection = hsla(self.theme.ui("selection"));
        for s in selections.iter().filter(|s| !s.is_empty()) {
            spans(s.range(), selection, true);
        }
        let primary = self.state.primary();
        if primary.is_empty()
            && let Some((a, b)) = ferrumeditor_core::editor::matching_bracket(buffer, primary.head)
        {
            let color = hsla(self.theme.ui("bracket_match"));
            for range in [a, b] {
                if let Some(row) = layout.row_for_offset(range.start) {
                    let x0 = x_of(row, range.start);
                    let x1 = x_of(row, range.end).max(x0 + px(4.));
                    let bounds = Bounds::from_corners(
                        point(x0, row.top + px(1.)),
                        point(x1, row.top + lh - px(1.)),
                    );
                    frame
                        .foreground
                        .push(outline(bounds, color, BorderStyle::Solid));
                }
            }
        }
        let cursor = hsla(self.theme.ui("cursor"));
        for s in selections {
            if let Some(row) = layout.row_for_offset(s.head) {
                let bounds = Bounds::new(point(x_of(row, s.head), row.top), size(px(2.), lh));
                frame.cursors.push(fill(bounds, cursor));
            }
        }
    }

    pub(super) fn line_numbers(
        &self,
        frame: &mut Frame,
        style: &FrameStyle,
        (normal, active): (Hsla, Hsla),
        window: &mut Window,
    ) {
        let layout = &frame.layout;
        let head_row = self.state.buffer().row_of(self.state.primary().head);
        for row in &layout.rows {
            let text: SharedString = (row.row + 1).to_string().into();
            let run = TextRun {
                len: text.len(),
                font: style.font.clone(),
                color: if row.row == head_row { active } else { normal },
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window
                .text_system()
                .shape_line(text, style.font_size, &[run], None);
            let x = layout.bounds.left() + layout.gutter_width - px(10.) - shaped.width;
            frame.numbers.push((shaped, point(x, row.top)));
        }
    }
}
