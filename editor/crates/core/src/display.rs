//! Display geometry independent of fonts: tab expansion, visual (cell)
//! columns, soft-wrap rows and rows hidden by folds. A [`DisplayMap`] maps buffer offsets to
//! display rows and cell columns, which vertical motions and the renderer
//! share.

use std::ops::Range;

use unicode_width::UnicodeWidthChar;

use crate::buffer::Buffer;

fn cell_width(c: char, col: usize, tab_size: usize) -> usize {
    if c == '\t' {
        tab_size - (col % tab_size.max(1))
    } else {
        c.width().unwrap_or(0).max(usize::from(!c.is_control()))
    }
}

/// Visual column of byte `col` in `line` (tabs expand to the next stop).
pub fn visual_col(line: &str, col: usize, tab_size: usize) -> usize {
    let mut cells = 0;
    for (index, c) in line.char_indices() {
        if index >= col {
            break;
        }
        cells += cell_width(c, cells, tab_size);
    }
    cells
}

/// Byte column whose visual column is closest to (not past) `target`.
pub fn byte_col_for_visual(line: &str, target: usize, tab_size: usize) -> usize {
    let mut cells = 0;
    for (index, c) in line.char_indices() {
        let width = cell_width(c, cells, tab_size);
        if cells + width > target {
            // Round to the nearer edge of a wide char / tab.
            return if target - cells > width / 2 && width > 1 {
                index + c.len_utf8()
            } else {
                index
            };
        }
        cells += width;
    }
    line.len()
}

/// A line with tabs replaced by spaces, plus the mapping back to the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedLine {
    pub text: String,
    /// (buffer byte of a tab, display byte where its spaces start, width).
    tabs: Vec<(usize, usize, usize)>,
}

impl ExpandedLine {
    pub fn new(line: &str, tab_size: usize) -> Self {
        if !line.contains('\t') {
            return Self {
                text: line.to_string(),
                tabs: Vec::new(),
            };
        }
        let mut text = String::with_capacity(line.len() + 8);
        let mut tabs = Vec::new();
        let mut cells = 0;
        for (index, c) in line.char_indices() {
            let width = cell_width(c, cells, tab_size);
            if c == '\t' {
                tabs.push((index, text.len(), width));
                text.extend(std::iter::repeat_n(' ', width));
            } else {
                text.push(c);
            }
            cells += width;
        }
        Self { text, tabs }
    }

    /// Display byte for a buffer byte column.
    pub fn to_display(&self, col: usize) -> usize {
        let mut shift = 0;
        for &(buffer, _, width) in &self.tabs {
            if buffer >= col {
                break;
            }
            shift += width - 1;
        }
        col + shift
    }

    /// Buffer byte column for a display byte (inside a tab → the nearer edge).
    pub fn to_buffer(&self, display: usize) -> usize {
        let mut shift = 0;
        for &(buffer, start, width) in &self.tabs {
            if display < start {
                break;
            }
            if display < start + width {
                return if display - start > width / 2 {
                    buffer + 1
                } else {
                    buffer
                };
            }
            shift += width - 1;
        }
        display - shift
    }
}

/// Byte offsets (within `line`) where each display row starts when the line
/// is wrapped at `width` cells. Breaks after whitespace when possible.
pub fn wrap_line(line: &str, width: usize, tab_size: usize) -> Vec<usize> {
    let mut starts = vec![0];
    if width == 0 {
        return starts;
    }
    let mut row_cells = 0;
    let mut cells = 0;
    let mut last_break: Option<(usize, usize)> = None; // (byte, cells at byte)
    let mut row_start_cells = 0;
    for (index, c) in line.char_indices() {
        let w = cell_width(c, cells, tab_size);
        if row_cells + w > width && index > *starts.last().unwrap_or(&0) {
            let (break_at, break_cells) = match last_break {
                Some((byte, at)) if byte > *starts.last().unwrap_or(&0) => (byte, at),
                _ => (index, cells),
            };
            starts.push(break_at);
            row_start_cells = break_cells;
            last_break = None;
        }
        cells += w;
        row_cells = cells - row_start_cells;
        if c == ' ' || c == '\t' {
            last_break = Some((index + c.len_utf8(), cells));
        }
    }
    starts
}

/// A position on screen: display row and visual column within that row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DisplayPoint {
    pub row: usize,
    pub col: usize,
}

/// Maps buffer rows to display rows. Without wrapping or folds every
/// buffer row is one display row and nothing is stored. Rows hidden by a
/// fold take no display rows.
#[derive(Debug, Clone, Default)]
pub struct DisplayMap {
    tab_size: usize,
    wrap_width: Option<usize>,
    /// Buffer rows hidden by folds (sorted, disjoint).
    hidden: Vec<Range<usize>>,
    /// Per buffer row: byte columns where display rows start (wrap or
    /// folds only; empty for hidden rows).
    row_starts: Vec<Vec<usize>>,
    /// Per buffer row: first display row (wrap or folds only; a hidden row
    /// gets the next shown row's).
    first_display_row: Vec<usize>,
    display_rows: usize,
    version: Option<u64>,
}

impl DisplayMap {
    pub fn new(tab_size: usize, wrap_width: Option<usize>) -> Self {
        Self {
            tab_size: tab_size.max(1),
            wrap_width: wrap_width.filter(|w| *w > 0),
            ..Self::default()
        }
    }

    pub fn tab_size(&self) -> usize {
        self.tab_size
    }

    pub fn wrap_width(&self) -> Option<usize> {
        self.wrap_width
    }

    /// Recomputes wrap rows if the buffer, tab size or width changed.
    pub fn sync(&mut self, buffer: &Buffer, tab_size: usize, wrap_width: Option<usize>) {
        let hidden = self.hidden.clone();
        self.sync_with_folds(buffer, tab_size, wrap_width, &hidden);
    }

    /// Like [`DisplayMap::sync`], with the buffer rows folds hide (sorted,
    /// disjoint row ranges).
    pub fn sync_with_folds(
        &mut self,
        buffer: &Buffer,
        tab_size: usize,
        wrap_width: Option<usize>,
        hidden: &[Range<usize>],
    ) {
        let tab_size = tab_size.max(1);
        let wrap_width = wrap_width.filter(|w| *w > 0);
        if self.version == Some(buffer.version())
            && self.tab_size == tab_size
            && self.wrap_width == wrap_width
            && self.hidden == hidden
            && (!self.is_identity() || self.display_rows == buffer.line_count())
        {
            return;
        }
        self.tab_size = tab_size;
        self.wrap_width = wrap_width;
        self.hidden = hidden.to_vec();
        self.version = Some(buffer.version());
        self.row_starts.clear();
        self.first_display_row.clear();
        if self.is_identity() {
            self.display_rows = buffer.line_count();
            return;
        }
        let mut total = 0;
        let mut hidden = self.hidden.iter().peekable();
        for row in 0..buffer.line_count() {
            while hidden.next_if(|h| h.end <= row).is_some() {}
            self.first_display_row.push(total);
            if hidden.peek().is_some_and(|h| h.contains(&row)) {
                self.row_starts.push(Vec::new());
                continue;
            }
            let line = buffer.line(row);
            let starts = match wrap_width {
                Some(width) if line.len() > width || line.contains('\t') => {
                    wrap_line(&line, width, tab_size)
                }
                _ => vec![0],
            };
            total += starts.len();
            self.row_starts.push(starts);
        }
        self.display_rows = total;
    }

    /// One display row per buffer row: nothing stored.
    fn is_identity(&self) -> bool {
        self.wrap_width.is_none() && self.hidden.is_empty()
    }

    /// Whether a fold hides buffer row `row`.
    pub fn is_hidden(&self, row: usize) -> bool {
        self.hidden.iter().any(|h| h.contains(&row))
    }

    pub fn row_count(&self) -> usize {
        self.display_rows.max(1)
    }

    pub fn first_display_row(&self, buffer_row: usize) -> usize {
        if self.is_identity() {
            return buffer_row;
        }
        self.first_display_row
            .get(buffer_row)
            .copied()
            .unwrap_or(self.display_rows)
    }

    /// Buffer row and the byte columns of that line shown on display row `row`.
    pub fn buffer_segment(&self, buffer: &Buffer, row: usize) -> (usize, Range<usize>) {
        if self.is_identity() {
            let row = row.min(buffer.line_count().saturating_sub(1));
            return (row, 0..buffer.line_len(row));
        }
        let buffer_row = self
            .first_display_row
            .partition_point(|first| *first <= row)
            .saturating_sub(1);
        let starts = self.row_starts.get(buffer_row).map_or(&[0][..], |s| &s[..]);
        let index = row - self.first_display_row(buffer_row);
        let index = index.min(starts.len().saturating_sub(1));
        let end = starts
            .get(index + 1)
            .copied()
            .unwrap_or_else(|| buffer.line_len(buffer_row));
        (buffer_row, starts.get(index).copied().unwrap_or(0)..end)
    }

    pub fn to_display(&self, buffer: &Buffer, offset: usize) -> DisplayPoint {
        let mut point = buffer.offset_to_point(offset);
        if let Some(fold) = self.hidden.iter().find(|h| h.contains(&point.row)) {
            // Inside a fold: the end of the row the fold hangs from.
            let row = fold.start.saturating_sub(1);
            point = crate::buffer::Point::new(row, buffer.line_len(row));
        }
        let line = buffer.line(point.row);
        let first = self.first_display_row(point.row);
        let (segment, seg_start) = match self.row_starts.get(point.row) {
            Some(starts) if !starts.is_empty() => {
                let i = starts
                    .partition_point(|s| *s <= point.col)
                    .saturating_sub(1);
                // A caret exactly at a wrap point shows at the start of the next row.
                (i, starts[i])
            }
            _ => (0, 0),
        };
        let col = visual_col(&line, point.col, self.tab_size)
            - visual_col(&line, seg_start, self.tab_size);
        DisplayPoint {
            row: first + segment,
            col,
        }
    }

    pub fn to_offset(&self, buffer: &Buffer, point: DisplayPoint) -> usize {
        let row = point.row.min(self.row_count() - 1);
        let (buffer_row, segment) = self.buffer_segment(buffer, row);
        let line = buffer.line(buffer_row);
        let base = visual_col(&line, segment.start, self.tab_size);
        let mut col = byte_col_for_visual(&line, base + point.col, self.tab_size);
        let is_last_segment = segment.end >= line.len();
        if col > segment.end || (col == segment.end && !is_last_segment) {
            // Stay on this row: the char before the wrap point.
            col = line[..segment.end]
                .char_indices()
                .next_back()
                .map_or(segment.start, |(i, _)| i.max(segment.start));
        }
        buffer.line_start(buffer_row) + col.max(segment.start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_columns_expand_tabs_and_wide_chars() {
        assert_eq!(visual_col("\tx", 1, 4), 4);
        assert_eq!(visual_col("ab\tx", 3, 4), 4);
        assert_eq!(visual_col("日本x", 6, 4), 4);
        assert_eq!(byte_col_for_visual("\tx", 4, 4), 1);
        assert_eq!(byte_col_for_visual("\tx", 1, 4), 0);
        assert_eq!(byte_col_for_visual("\tx", 3, 4), 1);
        assert_eq!(byte_col_for_visual("abc", 10, 4), 3);
    }

    #[test]
    fn expanded_line_maps_both_ways() {
        let line = ExpandedLine::new("a\tb\tc", 4);
        assert_eq!(line.text, "a   b   c");
        assert_eq!(line.to_display(2), 4);
        assert_eq!(line.to_display(4), 8);
        assert_eq!(line.to_buffer(4), 2);
        assert_eq!(line.to_buffer(8), 4);
        assert_eq!(line.to_buffer(1), 1);
        assert_eq!(line.to_buffer(3), 2);
    }

    #[test]
    fn wrap_prefers_whitespace() {
        assert_eq!(wrap_line("hello world foo", 8, 4), vec![0, 6, 12]);
        assert_eq!(wrap_line("abcdefghij", 4, 4), vec![0, 4, 8]);
        assert_eq!(wrap_line("short", 10, 4), vec![0]);
    }

    #[test]
    fn display_map_without_wrap_is_identity() {
        let buffer = Buffer::new("a\n\tbc\n");
        let mut map = DisplayMap::new(4, None);
        map.sync(&buffer, 4, None);
        assert_eq!(map.row_count(), 3);
        assert_eq!(map.to_display(&buffer, 4), DisplayPoint { row: 1, col: 5 });
        assert_eq!(map.to_offset(&buffer, DisplayPoint { row: 1, col: 5 }), 4);
        assert_eq!(map.to_offset(&buffer, DisplayPoint { row: 0, col: 9 }), 1);
    }

    #[test]
    fn display_map_with_wrap() {
        let buffer = Buffer::new("hello world foo\nx");
        let mut map = DisplayMap::new(4, Some(8));
        map.sync(&buffer, 4, Some(8));
        assert_eq!(map.row_count(), 4);
        assert_eq!(map.buffer_segment(&buffer, 1), (0, 6..12));
        assert_eq!(map.buffer_segment(&buffer, 3), (1, 0..1));
        assert_eq!(map.to_display(&buffer, 7), DisplayPoint { row: 1, col: 1 });
        assert_eq!(map.to_display(&buffer, 6), DisplayPoint { row: 1, col: 0 });
        assert_eq!(map.to_offset(&buffer, DisplayPoint { row: 1, col: 1 }), 7);
        // past the end of a wrapped (non-final) row stays on that row
        assert_eq!(map.to_offset(&buffer, DisplayPoint { row: 0, col: 20 }), 5);
        assert_eq!(map.to_offset(&buffer, DisplayPoint { row: 2, col: 20 }), 15);
        assert_eq!(map.first_display_row(1), 3);
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init)] // one fold is the point
    fn folded_rows_take_no_display_rows() {
        let buffer = Buffer::new("fn a() {\n    x;\n    y;\n}\nz");
        let mut map = DisplayMap::new(4, None);
        map.sync_with_folds(&buffer, 4, None, &[1..3]);
        assert_eq!(map.row_count(), 3);
        assert_eq!(map.buffer_segment(&buffer, 0), (0, 0..8));
        assert_eq!(map.buffer_segment(&buffer, 1), (3, 0..1));
        assert_eq!(map.buffer_segment(&buffer, 2), (4, 0..1));
        assert!(map.is_hidden(2) && !map.is_hidden(3));
        // An offset inside the fold shows at the end of its first row.
        let inside = buffer.line_start(2) + 2;
        assert_eq!(
            map.to_display(&buffer, inside),
            DisplayPoint { row: 0, col: 8 }
        );
        assert_eq!(
            map.to_offset(&buffer, DisplayPoint { row: 1, col: 0 }),
            buffer.line_start(3)
        );
        // Folds and wrap together; a fold reaching the last row.
        let mut map = DisplayMap::new(4, Some(4));
        map.sync_with_folds(&buffer, 4, Some(4), &[1..5]);
        assert_eq!(map.row_count(), 3);
        assert_eq!(map.buffer_segment(&buffer, 2), (0, 7..8));
        // Unfolding restores the identity mapping.
        let mut map = DisplayMap::new(4, None);
        map.sync_with_folds(&buffer, 4, None, &[1..3]);
        map.sync_with_folds(&buffer, 4, None, &[]);
        assert_eq!(map.row_count(), 5);
    }
}
