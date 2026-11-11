//! Per-frame decorations: current line, search matches, selections,
//! indent guides, bracket match, cursors, line numbers and git marks.

use ferrumeditor_core::display::visual_col;
use ferrumeditor_core::git::LineMark;
use ferrumeditor_core::theme::Theme;
use gpui::{
    BorderStyle, Bounds, Hsla, PaintQuad, SharedString, TextRun, Window, fill, outline, point, px,
    size,
};

use super::EditorView;
use super::element::{Frame, FrameStyle};
use super::layout::RowLayout;
use crate::theme_apply::ui;

impl EditorView {
    /// Current line, search matches, selections, indent guides, bracket
    /// match and cursors.
    pub(super) fn decorate(&self, frame: &mut Frame, style: &FrameStyle, theme: &Theme) {
        let layout = &frame.layout;
        let buffer = self.state.buffer();
        let lh = layout.line_height;
        let x_of = |row: &RowLayout, offset: usize| {
            layout.text_left + row.x_for_offset(offset) - layout.scroll_x
        };
        let row_range = |row: &RowLayout| {
            (
                row.line_start + row.segment.start,
                row.line_start + row.segment.end,
            )
        };
        let selections = self.state.selections();

        if style.highlight_current_line && selections.iter().all(|s| s.is_empty()) {
            let color = ui(theme, "current_line");
            for row in &layout.rows {
                if selections
                    .iter()
                    .any(|s| buffer.row_of(s.head) == row.buffer_row)
                {
                    frame.backgrounds.push(fill(
                        Bounds::new(
                            point(frame.text_area.left(), row.top),
                            size(frame.text_area.size.width, lh),
                        ),
                        color,
                    ));
                }
            }
        }
        if let Some(exec) = self.debug_marks.exec_line {
            let color = ui(theme, "warning").opacity(0.18);
            for row in layout.rows.iter().filter(|r| r.buffer_row == exec) {
                frame.backgrounds.push(fill(
                    Bounds::new(
                        point(frame.text_area.left(), row.top),
                        size(frame.text_area.size.width, lh),
                    ),
                    color,
                ));
            }
        }
        let span_quads = |ranges: &mut dyn Iterator<Item = std::ops::Range<usize>>,
                          color: Hsla,
                          out: &mut Vec<PaintQuad>,
                          newline: bool| {
            for range in ranges {
                for row in &layout.rows {
                    let (start, end) = row_range(row);
                    let s = range.start.max(start);
                    let e = range.end.min(end);
                    let past_end =
                        newline && range.end > end && range.start <= end && row.last_segment;
                    if s > e || (s == e && !past_end) {
                        continue;
                    }
                    let x0 = x_of(row, s);
                    let mut x1 = x_of(row, e);
                    if past_end {
                        x1 += layout.char_width;
                    }
                    out.push(fill(
                        Bounds::from_corners(point(x0, row.top), point(x1, row.top + lh)),
                        color,
                    ));
                }
            }
        };
        if let (Some(first), Some(last)) = (layout.rows.first(), layout.rows.last()) {
            let (visible_start, _) = row_range(first);
            let (_, visible_end) = row_range(last);
            let matches = &self.search.matches;
            let from = matches.partition_point(|m| m.end < visible_start);
            let to = matches.partition_point(|m| m.start <= visible_end);
            let active = self.search.active;
            let normal = ui(theme, "find_match");
            let strong = ui(theme, "find_match_active");
            for (i, m) in matches[from..to.max(from)].iter().enumerate() {
                let color = if active == Some(from + i) {
                    strong
                } else {
                    normal
                };
                span_quads(
                    &mut std::iter::once(m.clone()),
                    color,
                    &mut frame.backgrounds,
                    false,
                );
            }
        }
        let selection_color = ui(theme, "selection");
        span_quads(
            &mut selections
                .iter()
                .filter(|s| !s.is_empty())
                .map(|s| s.range()),
            selection_color,
            &mut frame.backgrounds,
            true,
        );
        if self.show_indent_guides {
            self.indent_guides(frame, theme);
        }
        let layout = &frame.layout;
        let x_of = |row: &RowLayout, offset: usize| {
            layout.text_left + row.x_for_offset(offset) - layout.scroll_x
        };
        let primary = self.state.primary();
        if primary.is_empty()
            && let Some((a, b)) = ferrumeditor_core::editor::matching_bracket(buffer, primary.head)
        {
            let color = ui(theme, "bracket_match");
            for range in [a, b] {
                if let Some(row) = layout.row_for_offset(range.start) {
                    let x0 = x_of(row, range.start);
                    let x1 = x_of(row, range.end).max(x0 + px(4.));
                    frame.foreground.push(outline(
                        Bounds::from_corners(
                            point(x0, row.top + px(1.)),
                            point(x1, row.top + lh - px(1.)),
                        ),
                        color,
                        BorderStyle::Solid,
                    ));
                }
            }
        }
        let cursor = ui(theme, "cursor");
        for s in selections {
            if let Some(row) = layout.row_for_offset(s.head) {
                let x = x_of(row, s.head);
                frame.cursors.push(fill(
                    Bounds::new(point(x, row.top), size(px(2.), lh)),
                    cursor,
                ));
            }
        }
    }

    fn indent_guides(&self, frame: &mut Frame, theme: &Theme) {
        let layout = &frame.layout;
        let buffer = self.state.buffer();
        let tab = self.state.settings.tab_size.max(1);
        let color = ui(theme, "indent_guide");
        let indent_cells = |row: usize| -> Option<usize> {
            let line = buffer.line(row);
            let trimmed = line.trim_start_matches([' ', '\t']);
            (!trimmed.is_empty()).then(|| visual_col(&line, line.len() - trimmed.len(), tab))
        };
        let mut cache: Option<(usize, usize)> = None;
        for row in &layout.rows {
            let cells = match cache {
                Some((r, cells)) if r == row.buffer_row => cells,
                _ => {
                    let cells = indent_cells(row.buffer_row).unwrap_or_else(|| {
                        (row.buffer_row + 1..(row.buffer_row + 200).min(buffer.line_count()))
                            .find_map(indent_cells)
                            .unwrap_or(0)
                    });
                    cache = Some((row.buffer_row, cells));
                    cells
                }
            };
            let mut level = tab;
            while level < cells {
                let x = layout.text_left + layout.char_width * level as f32 - layout.scroll_x;
                frame.backgrounds.push(fill(
                    Bounds::new(point(x, row.top), size(px(1.), layout.line_height)),
                    color,
                ));
                level += tab;
            }
            if cells > 0 && row.segment.start == 0 {
                let x = layout.text_left - layout.scroll_x;
                if x >= frame.text_area.left() + px(2.) {
                    frame.backgrounds.push(fill(
                        Bounds::new(point(x, row.top), size(px(1.), layout.line_height)),
                        color,
                    ));
                }
            }
        }
    }

    pub(super) fn gutter(
        &self,
        frame: &mut Frame,
        style: &FrameStyle,
        theme: &Theme,
        window: &mut Window,
    ) {
        let layout = &frame.layout;
        let head_row = self.state.buffer().row_of(self.state.primary().head);
        let normal = ui(theme, "line_number");
        let active = ui(theme, "line_number_active");
        if self.show_line_numbers {
            for row in layout.rows.iter().filter(|r| r.segment.start == 0) {
                let text: SharedString = (row.buffer_row + 1).to_string().into();
                let color = if row.buffer_row == head_row {
                    active
                } else {
                    normal
                };
                let run = TextRun {
                    len: text.len(),
                    font: style.font.clone(),
                    color,
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
        // Problems: a dot per line, coloured by the worst one.
        let diagnostics = &self.lsp.diagnostics;
        if !diagnostics.is_empty() {
            let buffer = self.state.buffer();
            let dot = (layout.line_height * 0.32).max(px(5.));
            for row in layout.rows.iter().filter(|r| r.segment.start == 0) {
                let start = row.line_start;
                let end = buffer.line_end(row.buffer_row);
                let first = diagnostics.partition_point(|d| d.range.start < start);
                let worst = diagnostics[first..]
                    .iter()
                    .take_while(|d| d.range.start <= end)
                    .map(|d| d.severity)
                    .min();
                if let Some(severity) =
                    worst.filter(|s| *s != ferrumeditor_core::lsp::Severity::Hint)
                {
                    let color = ui(theme, super::popups::severity_token(severity));
                    let y = row.top + (layout.line_height - dot) / 2.;
                    frame.gutter_marks.push(
                        fill(
                            Bounds::new(point(layout.bounds.left() + px(7.), y), size(dot, dot)),
                            color,
                        )
                        .corner_radii(dot / 2.),
                    );
                }
            }
        }
        // Breakpoints: red dots (hollow-looking when the adapter has not
        // verified them); the paused line gets an arrow.
        let red = ui(theme, "error");
        let dot = (layout.line_height * 0.5).clamp(px(7.), px(11.));
        for row in layout.rows.iter().filter(|r| r.segment.start == 0) {
            let y = row.top + (layout.line_height - dot) / 2.;
            let x = layout.bounds.left() + px(3.);
            if let Some((_, verified)) = self
                .debug_marks
                .breakpoints
                .iter()
                .find(|(line, _)| *line == row.buffer_row)
            {
                let color = if *verified == Some(false) {
                    red.opacity(0.4)
                } else {
                    red
                };
                frame.gutter_marks.push(
                    fill(Bounds::new(point(x, y), size(dot, dot)), color).corner_radii(dot / 2.),
                );
            }
            if self.debug_marks.exec_line == Some(row.buffer_row) {
                let color = ui(theme, "warning");
                let h = dot * 0.8;
                let y = row.top + (layout.line_height - h) / 2.;
                frame.gutter_marks.push(
                    fill(Bounds::new(point(x + px(1.), y), size(px(9.), h)), color)
                        .corner_radii(px(2.)),
                );
            }
        }
        let marks = &self.git_marks;
        let lane_x = layout.bounds.left() + px(3.);
        for row in &layout.rows {
            let Ok(index) = marks.binary_search_by_key(&row.buffer_row, |(line, _)| *line) else {
                continue;
            };
            let (color, bounds) = match marks[index].1 {
                LineMark::Added => (
                    ui(theme, "git_added"),
                    Bounds::new(point(lane_x, row.top), size(px(3.), layout.line_height)),
                ),
                LineMark::Modified => (
                    ui(theme, "git_modified"),
                    Bounds::new(point(lane_x, row.top), size(px(3.), layout.line_height)),
                ),
                LineMark::Deleted if row.segment.start == 0 => (
                    ui(theme, "git_deleted"),
                    Bounds::new(point(lane_x, row.top - px(2.)), size(px(7.), px(4.))),
                ),
                LineMark::Deleted => continue,
            };
            frame.gutter_marks.push(fill(bounds, color));
        }
    }
}
