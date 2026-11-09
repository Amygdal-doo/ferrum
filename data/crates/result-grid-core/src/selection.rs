//! Rectangular cell selection in view coordinates (row indices are positions
//! in the displayed, possibly sorted, order).

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPos {
    pub row: usize,
    pub col: usize,
}

/// Anchor is where the selection started; focus is where shift-click moved it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSelection {
    pub anchor: CellPos,
    pub focus: CellPos,
    /// Selected via the row-number gutter: spans every column.
    pub whole_rows: bool,
}

impl GridSelection {
    #[must_use]
    pub const fn cell(row: usize, col: usize) -> Self {
        let pos = CellPos { row, col };
        Self {
            anchor: pos,
            focus: pos,
            whole_rows: false,
        }
    }

    #[must_use]
    pub const fn row(row: usize) -> Self {
        let pos = CellPos { row, col: 0 };
        Self {
            anchor: pos,
            focus: pos,
            whole_rows: true,
        }
    }

    /// Every cell of a `rows` × `cols` grid; `None` if it is empty.
    #[must_use]
    pub fn all(rows: usize, cols: usize) -> Option<Self> {
        (rows > 0 && cols > 0).then(|| Self {
            anchor: CellPos { row: 0, col: 0 },
            focus: CellPos {
                row: rows - 1,
                col: cols - 1,
            },
            whole_rows: true,
        })
    }

    /// Shift-click: keeps the anchor and moves the focus.
    #[must_use]
    pub const fn extend_to(self, row: usize, col: usize) -> Self {
        Self {
            focus: CellPos { row, col },
            ..self
        }
    }

    #[must_use]
    pub fn rows(&self) -> Range<usize> {
        let (a, b) = (self.anchor.row, self.focus.row);
        a.min(b)..a.max(b) + 1
    }

    /// Selected columns, clamped to `col_count`.
    #[must_use]
    pub fn cols(&self, col_count: usize) -> Range<usize> {
        if self.whole_rows {
            return 0..col_count;
        }
        let (a, b) = (self.anchor.col, self.focus.col);
        a.min(b).min(col_count)..(a.max(b) + 1).min(col_count)
    }

    #[must_use]
    pub fn contains(&self, row: usize, col: usize) -> bool {
        self.rows().contains(&row) && (self.whole_rows || self.cols(usize::MAX).contains(&col))
    }

    #[must_use]
    pub fn is_single_cell(&self) -> bool {
        !self.whole_rows && self.anchor == self.focus
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_extension_spans_a_rectangle_in_any_direction() {
        let s = GridSelection::cell(5, 3).extend_to(2, 1);
        assert_eq!(s.rows(), 2..6);
        assert_eq!(s.cols(10), 1..4);
        assert!(s.contains(2, 1) && s.contains(5, 3) && s.contains(4, 2));
        assert!(!s.contains(6, 2) && !s.contains(3, 4));
        assert!(!s.is_single_cell());
        assert!(GridSelection::cell(1, 1).is_single_cell());
    }

    #[test]
    fn row_selection_spans_every_column() {
        let s = GridSelection::row(4).extend_to(1, 0);
        assert_eq!(s.rows(), 1..5);
        assert_eq!(s.cols(7), 0..7);
        assert!(s.contains(3, 6));
        assert!(!s.is_single_cell());
    }

    #[test]
    fn columns_are_clamped_and_empty_grids_have_no_selection() {
        assert_eq!(GridSelection::cell(0, 9).cols(3), 3..3);
        assert_eq!(GridSelection::all(0, 3), None);
        let all = GridSelection::all(3, 2).expect("non-empty");
        assert_eq!((all.rows(), all.cols(2)), (0..3, 0..2));
    }
}
