//! Mouse selection inside one pane, in screen cells: drag to select (a
//! stream from anchor to head, like other terminals), ⌘C to copy. The text
//! is taken from the frame on mouse-up, so later output doesn't change what
//! gets copied.

use ferrumterm_protocol::{BlockId, Frame};
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cell {
    pub row: u16,
    pub col: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub block: BlockId,
    pub anchor: Cell,
    pub head: Cell,
}

/// A pane's hit rectangle in window px: `(block, x, y, width, height)`.
pub type PaneHit = (BlockId, f32, f32, f32, f32);

/// The cell under window point `(x, y)` in `pane`, clamped to the pane (so a
/// drag past its edge selects to the edge).
#[must_use]
pub fn cell_at(pane: &PaneHit, x: f32, y: f32, cell_w: f32, cell_h: f32) -> Cell {
    let (_, px, py, w, h) = *pane;
    let cols = (w / cell_w).round().max(1.0);
    let rows = (h / cell_h).round().max(1.0);
    Cell {
        col: ((x - px) / cell_w).floor().clamp(0.0, cols - 1.0) as u16,
        row: ((y - py) / cell_h).floor().clamp(0.0, rows - 1.0) as u16,
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

    /// Selected columns `start..end` on `row` of a `cols`-wide pane.
    #[must_use]
    pub fn columns(&self, row: u16, cols: u16) -> Option<(u16, u16)> {
        let (start, end) = self.ordered();
        if self.is_empty() || row < start.row || row > end.row {
            return None;
        }
        let from = if row == start.row { start.col } else { 0 };
        let to = if row == end.row { end.col + 1 } else { cols };
        (from < to).then_some((from, to.min(cols)))
    }

    /// The selected text: lines joined with `\n`, trailing spaces trimmed.
    #[must_use]
    pub fn text(&self, frame: &Frame) -> String {
        let (start, end) = self.ordered();
        let mut lines = Vec::new();
        for row in start.row..=end.row {
            let Some((from, to)) = self.columns(row, frame.cols) else {
                continue;
            };
            let mut text = String::new();
            let mut col = 0u16;
            for c in frame
                .lines
                .get(usize::from(row))
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

    fn sel(anchor: (u16, u16), head: (u16, u16)) -> Selection {
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
}
