//! Mouse selection in screen cells: a stream from anchor to head, like
//! other terminals. The text is taken from the frame when the drag ends,
//! so later output does not change what gets copied.

use ferrumterm_protocol::Frame;
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cell {
    pub row: u16,
    pub col: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Cell,
    pub head: Cell,
}

impl Selection {
    pub fn new(at: Cell) -> Self {
        Self {
            anchor: at,
            head: at,
        }
    }

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

    /// Selected columns `start..end` on `row` of a `cols`-wide screen.
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
    fn streams_across_lines_and_trims() {
        let f = frame(&["ab   ", "cdef", "ghij"]);
        assert_eq!(sel((0, 1), (2, 1)).text(&f), "b\ncdef\ngh");
        assert_eq!(
            sel((2, 1), (0, 1)).text(&f),
            "b\ncdef\ngh",
            "backwards drag"
        );
        assert_eq!(sel((0, 1), (2, 1)).columns(1, 10), Some((0, 10)));
        assert!(sel((1, 1), (1, 1)).is_empty());
    }

    #[test]
    fn wide_characters_take_two_columns() {
        let f = frame(&["日本x"]);
        assert_eq!(sel((0, 2), (0, 4)).text(&f), "本x");
    }
}
