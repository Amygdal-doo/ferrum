//! Mouse selection inside one pane: drag to select (a stream from anchor to
//! head, like other terminals), ⌘C to copy. Rows are *lines* — the screen
//! row minus the frame's `display_offset` (0 = top of the live screen,
//! negative = scrollback) — so a selection stays on its text while the view
//! scrolls (dragging past the top / bottom edge autoscrolls, see
//! `view::drag_scroll`). The text is taken on mouse-up from the lines seen
//! during the drag, so later output doesn't change what gets copied.

use ferrumterm_protocol::{BlockId, Frame, Span};
use std::collections::BTreeMap;
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cell {
    /// Line: screen row − `display_offset` when hit, see the module docs.
    pub row: i64,
    pub col: u16,
}

/// The line shown at screen `row` of `frame`.
#[must_use]
pub fn line_of(frame: &Frame, row: u16) -> i64 {
    i64::from(row) - i64::from(frame.display_offset)
}

/// Lines of one pane seen during a drag, by line number (see [`Cell::row`]):
/// scrolled-away lines are still in the copied text.
#[derive(Debug, Default, Clone)]
pub struct SeenLines(BTreeMap<i64, Vec<Span>>);

impl SeenLines {
    /// Remembers every line `frame` shows.
    pub fn record(&mut self, frame: &Frame) {
        for (row, line) in frame.lines.iter().enumerate() {
            let row = u16::try_from(row).unwrap_or(u16::MAX);
            self.0.insert(line_of(frame, row), line.clone());
        }
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    #[must_use]
    pub fn get(&self, line: i64) -> Option<&[Span]> {
        self.0.get(&line).map(Vec::as_slice)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub block: BlockId,
    pub anchor: Cell,
    pub head: Cell,
}

/// A pane's hit rectangle in window px: `(block, x, y, width, height)`.
pub type PaneHit = (BlockId, f32, f32, f32, f32);

/// The screen cell under window point `(x, y)` in `pane`, clamped to the
/// pane (so a drag past its edge selects to the edge).
#[must_use]
pub fn cell_at(pane: &PaneHit, x: f32, y: f32, cell_w: f32, cell_h: f32) -> Cell {
    let (_, px, py, w, h) = *pane;
    let cols = (w / cell_w).round().max(1.0);
    let rows = (h / cell_h).round().max(1.0);
    Cell {
        col: ((x - px) / cell_w).floor().clamp(0.0, cols - 1.0) as u16,
        row: ((y - py) / cell_h).floor().clamp(0.0, rows - 1.0) as i64,
    }
}

impl Selection {
    #[must_use]
    pub fn new(block: BlockId, at: Cell) -> Self {
        Self {
            block,
            anchor: at,
            head: at,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    fn ordered(&self) -> (Cell, Cell) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// Selected columns `start..end` on line `row` of a `cols`-wide pane.
    #[must_use]
    pub fn columns(&self, row: i64, cols: u16) -> Option<(u16, u16)> {
        let (start, end) = self.ordered();
        if self.is_empty() || row < start.row || row > end.row {
            return None;
        }
        let from = if row == start.row { start.col } else { 0 };
        let to = if row == end.row { end.col + 1 } else { cols };
        (from < to).then_some((from, to.min(cols)))
    }

    /// The selected text of what `frame` shows.
    #[cfg(test)]
    #[must_use]
    pub fn text(&self, frame: &Frame) -> String {
        self.text_from(frame.cols, |line| {
            let row = usize::try_from(line + i64::from(frame.display_offset)).ok()?;
            frame.lines.get(row).map(Vec::as_slice)
        })
    }

    /// The selected text: lines joined with `\n`, trailing spaces trimmed.
    /// `line` gives a line's spans (missing lines count as empty).
    #[must_use]
    pub fn text_from<'a>(&self, cols: u16, line: impl Fn(i64) -> Option<&'a [Span]>) -> String {
        let (start, end) = self.ordered();
        let mut lines = Vec::new();
        for row in start.row..=end.row {
            let Some((from, to)) = self.columns(row, cols) else {
                continue;
            };
            let mut text = String::new();
            let mut col = 0u16;
            for c in line(row)
                .into_iter()
                .flatten()
                .flat_map(|span| span.t.chars())
            {
                if col >= from && col < to {
                    text.push(c);
                }
                col += UnicodeWidthChar::width(c).unwrap_or(0) as u16;
            }
            lines.push(text.trim_end().to_owned());
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumterm_protocol::Span;

    fn frame(lines: &[&str]) -> Frame {
        Frame {
            cols: 10,
            rows: lines.len() as u16,
            lines: lines
                .iter()
                .map(|l| {
                    vec![Span {
                        t: (*l).to_owned(),
                        ..Span::default()
                    }]
                })
                .collect(),
            ..Frame::default()
        }
    }

    fn sel(anchor: (i64, u16), head: (i64, u16)) -> Selection {
        Selection {
            block: BlockId(1),
            anchor: Cell {
                row: anchor.0,
                col: anchor.1,
            },
            head: Cell {
                row: head.0,
                col: head.1,
            },
        }
    }

    #[test]
    fn single_line_and_backwards_drag() {
        let f = frame(&["hello world"]);
        assert_eq!(sel((0, 0), (0, 4)).text(&f), "hello");
        assert_eq!(sel((0, 4), (0, 0)).text(&f), "hello");
        assert!(sel((0, 3), (0, 3)).is_empty());
        assert_eq!(sel((0, 3), (0, 3)).columns(0, 10), None);
    }

    #[test]
    fn multi_line_streams_and_trims() {
        let f = frame(&["ab   ", "cdef", "ghij"]);
        let s = sel((0, 1), (2, 1));
        assert_eq!(s.text(&f), "b\ncdef\ngh");
        assert_eq!(s.columns(0, 10), Some((1, 10)));
        assert_eq!(s.columns(1, 10), Some((0, 10)));
        assert_eq!(s.columns(2, 10), Some((0, 2)));
        assert_eq!(s.columns(3, 10), None);
    }

    #[test]
    fn wide_characters_take_two_columns() {
        let f = frame(&["日本x"]);
        assert_eq!(sel((0, 2), (0, 4)).text(&f), "本x");
    }

    #[test]
    fn hit_testing_clamps_to_the_pane() {
        let pane = (BlockId(1), 100.0, 50.0, 80.0, 40.0); // 10 × 2 cells of 8 × 20
        assert_eq!(
            cell_at(&pane, 117.0, 71.0, 8.0, 20.0),
            Cell { row: 1, col: 2 }
        );
        assert_eq!(
            cell_at(&pane, 0.0, 500.0, 8.0, 20.0),
            Cell { row: 1, col: 0 }
        );
        assert_eq!(
            cell_at(&pane, 999.0, 0.0, 8.0, 20.0),
            Cell { row: 0, col: 9 }
        );
    }

    #[test]
    fn lines_are_screen_rows_minus_the_scroll_offset() {
        let mut f = frame(&["older", "old", "new"]);
        f.display_offset = 2;
        assert_eq!(line_of(&f, 0), -2);
        assert_eq!(line_of(&f, 2), 0);
        // The selection follows lines, so it reads the same text scrolled.
        assert_eq!(sel((-2, 0), (-1, 2)).text(&f), "older\nold");
    }

    #[test]
    fn text_spans_lines_seen_while_scrolling() {
        let mut seen = SeenLines::default();
        let mut top = frame(&["one", "two"]);
        top.display_offset = 2;
        seen.record(&top);
        let bottom = frame(&["three", "four"]);
        seen.record(&bottom);
        let s = sel((-2, 0), (1, 3));
        assert_eq!(s.text_from(10, |l| seen.get(l)), "one\ntwo\nthree\nfour");
        // Lines never seen copy as empty instead of failing.
        assert_eq!(
            sel((-4, 0), (-2, 2)).text_from(10, |l| seen.get(l)),
            "\n\none"
        );
        seen.clear();
        assert_eq!(seen.get(0), None);
    }
}
