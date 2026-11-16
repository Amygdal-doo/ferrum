//! The minimap: a narrow strip at the right of the text showing every
//! display row as small blocks in its syntax colours, with the visible
//! part marked. Clicking or dragging in it scrolls there. Rows are drawn
//! as quads (no text shaping), and only the rows the strip shows.

use ferrumeditor_core::theme::Theme;
use gpui::{Bounds, Context, Hsla, PaintQuad, Pixels, Point, fill, point, px, size};

use super::EditorView;
use super::element::Frame;
use super::layout::style_for;
use crate::theme_apply::ui;

/// Width of the strip.
pub(super) const MINIMAP_WIDTH: Pixels = px(96.);
/// Height of one display row in the strip.
const ROW_HEIGHT: f32 = 3.;
/// Width of one character cell in the strip.
const CELL: f32 = 1.5;
/// Space between the strip's left edge and the first column.
const INSET: f32 = 6.;

/// Where the strip was drawn, for hit-testing.
#[derive(Clone, Copy, Debug)]
pub struct MinimapLayout {
    pub bounds: Bounds<Pixels>,
    /// How far the strip's own content is scrolled (long files).
    pub offset: Pixels,
}

impl MinimapLayout {
    /// The display row under window y (may be past the last row).
    pub fn row_at(&self, y: Pixels) -> usize {
        ((y - self.bounds.top() + self.offset) / px(ROW_HEIGHT))
            .floor()
            .max(0.) as usize
    }
}

/// What prepaint needs to place the strip.
pub(super) struct MinimapInput {
    pub bounds: Bounds<Pixels>,
    pub total_rows: usize,
    pub line_height: Pixels,
    pub scroll_y: Pixels,
    pub max_scroll: Pixels,
}

impl EditorView {
    /// Lays out the strip and appends its quads to `frame.minimap`.
    pub(super) fn minimap(
        &self,
        input: MinimapInput,
        frame: &mut Frame,
        theme: &Theme,
        text_color: Hsla,
    ) -> MinimapLayout {
        let MinimapInput {
            bounds,
            total_rows,
            line_height,
            scroll_y,
            max_scroll,
        } = input;
        let content = px(ROW_HEIGHT * total_rows as f32);
        // Long files: the strip scrolls in step with the text.
        let offset = if content > bounds.size.height && max_scroll > px(0.) {
            (content - bounds.size.height) * (scroll_y / max_scroll).clamp(0., 1.)
        } else {
            px(0.)
        };
        let layout = MinimapLayout { bounds, offset };
        frame.minimap.push(fill(bounds, frame.colors.editor));
        // The visible part of the text.
        let top_row = scroll_y / line_height;
        let rows_shown = bounds.size.height / line_height;
        frame.minimap.push(fill(
            Bounds::new(
                point(
                    bounds.left(),
                    bounds.top() + px(ROW_HEIGHT * top_row) - offset,
                ),
                size(bounds.size.width, px(ROW_HEIGHT * rows_shown)),
            ),
            ui(theme, "scrollbar").opacity(0.35),
        ));
        let first = (offset / px(ROW_HEIGHT)).floor() as usize;
        let last =
            (first + (bounds.size.height / px(ROW_HEIGHT)).ceil() as usize + 1).min(total_rows);
        let buffer = self.state.buffer();
        let tab_size = self.state.settings.tab_size.max(1);
        let columns = ((f32::from(bounds.size.width) - INSET) / CELL).floor() as usize;
        let highlights = &self.highlights;
        for display_row in first..last {
            let (row, segment) = self.display_map.buffer_segment(buffer, display_row);
            let line_start = buffer.line_start(row);
            let line = buffer.line(row);
            let Some(text) = line.get(segment.clone()) else {
                continue;
            };
            let y = bounds.top() + px(ROW_HEIGHT * display_row as f32) - offset;
            let mut span =
                highlights.partition_point(|s| s.range.end <= line_start + segment.start);
            // Runs of non-blank characters of one colour become one block.
            let mut run: Option<(usize, usize, Hsla)> = None;
            let mut col = 0;
            let push = |run: Option<(usize, usize, Hsla)>, quads: &mut Vec<PaintQuad>| {
                if let Some((start, end, color)) = run {
                    quads.push(fill(
                        Bounds::new(
                            point(bounds.left() + px(INSET + CELL * start as f32), y),
                            size(px(CELL * (end - start) as f32), px(ROW_HEIGHT - 1.)),
                        ),
                        color.opacity(0.75),
                    ));
                }
            };
            for (index, c) in text.char_indices() {
                if col >= columns {
                    break;
                }
                let width = if c == '\t' {
                    tab_size - col % tab_size
                } else {
                    1
                };
                if c.is_whitespace() {
                    push(run.take(), &mut frame.minimap);
                    col += width;
                    continue;
                }
                let offset = line_start + segment.start + index;
                while highlights.get(span).is_some_and(|s| s.range.end <= offset) {
                    span += 1;
                }
                let color = highlights
                    .get(span)
                    .filter(|s| s.range.start <= offset)
                    .map_or(text_color, |s| style_for(theme, s.name(), text_color).0);
                match &mut run {
                    Some((_, end, run_color)) if *run_color == color => *end = col + width,
                    _ => {
                        push(run.take(), &mut frame.minimap);
                        run = Some((col, col + width, color));
                    }
                }
                col += width;
            }
            push(run, &mut frame.minimap);
        }
        layout
    }

    /// Where the minimap was last drawn and the text's scroll position.
    #[cfg(test)]
    pub fn minimap_layout(&self) -> Option<(MinimapLayout, Pixels)> {
        let layout = self.last_layout.as_ref()?;
        Some((layout.minimap?, layout.scroll_y))
    }

    /// Scrolls so the row under `position` in the strip is centred.
    pub(super) fn minimap_scroll_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(layout) = &self.last_layout else {
            return;
        };
        let Some(minimap) = layout.minimap else {
            return;
        };
        let row = minimap.row_at(position.y);
        self.scroll_y = layout.line_height * row as f32 - layout.bounds.size.height / 2.;
        self.autoscroll = false;
        cx.notify();
    }
}
