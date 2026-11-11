//! Rope-backed text buffer addressed by byte offsets. Line breaks are always
//! `\n` inside the buffer; the file's original line ending is restored on save
//! (see [`crate::files`]).

use std::borrow::Cow;
use std::ops::Range;

use ropey::Rope;

/// A position as (row, byte column within the row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Point {
    pub row: usize,
    pub col: usize,
}

impl Point {
    pub const fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Buffer {
    rope: Rope,
    version: u64,
}

impl Buffer {
    pub fn new(text: &str) -> Self {
        Self {
            rope: Rope::from_str(&normalize_newlines(text)),
            version: 0,
        }
    }

    /// Bumped by every edit; caches (highlights, wraps, search) key on it.
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    pub fn len(&self) -> usize {
        self.rope.len_bytes()
    }

    pub fn is_empty(&self) -> bool {
        self.rope.len_bytes() == 0
    }

    /// Number of rows; a trailing newline starts an (empty) last row.
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn line_start(&self, row: usize) -> usize {
        let row = row.min(self.line_count().saturating_sub(1));
        self.rope.line_to_byte(row)
    }

    /// Byte offset of the end of `row`, before its newline.
    pub fn line_end(&self, row: usize) -> usize {
        let row = row.min(self.line_count().saturating_sub(1));
        if row + 1 < self.line_count() {
            self.rope.line_to_byte(row + 1) - 1
        } else {
            self.len()
        }
    }

    pub fn line_len(&self, row: usize) -> usize {
        self.line_end(row) - self.line_start(row)
    }

    pub fn line_range(&self, row: usize) -> Range<usize> {
        self.line_start(row)..self.line_end(row)
    }

    /// Text of `row` without its newline.
    pub fn line(&self, row: usize) -> Cow<'_, str> {
        self.slice_cow(self.line_range(row))
    }

    pub fn row_of(&self, offset: usize) -> usize {
        self.rope.byte_to_line(offset.min(self.len()))
    }

    pub fn offset_to_point(&self, offset: usize) -> Point {
        let offset = self.clip(offset);
        let row = self.rope.byte_to_line(offset);
        Point::new(row, offset - self.rope.line_to_byte(row))
    }

    /// Clamps the row to the buffer and the column to the row's length (and a
    /// char boundary).
    pub fn point_to_offset(&self, point: Point) -> usize {
        if point.row >= self.line_count() {
            return self.len();
        }
        let start = self.line_start(point.row);
        self.clip(start + point.col.min(self.line_len(point.row)))
    }

    /// Rounds `offset` down to a char boundary inside the buffer.
    pub fn clip(&self, offset: usize) -> usize {
        let offset = offset.min(self.len());
        self.rope.char_to_byte(self.rope.byte_to_char(offset))
    }

    pub fn slice(&self, range: Range<usize>) -> String {
        self.slice_cow(range).into_owned()
    }

    pub fn slice_cow(&self, range: Range<usize>) -> Cow<'_, str> {
        let start = self.clip(range.start);
        let end = self.clip(range.end.max(start));
        self.rope.byte_slice(start..end).into()
    }

    pub fn char_at(&self, offset: usize) -> Option<char> {
        if offset >= self.len() {
            return None;
        }
        self.rope.get_char(self.rope.byte_to_char(offset))
    }

    pub fn char_before(&self, offset: usize) -> Option<char> {
        let offset = self.clip(offset);
        let index = self.rope.byte_to_char(offset);
        index.checked_sub(1).and_then(|i| self.rope.get_char(i))
    }

    /// Offset of the char after the one at `offset` (or the end).
    pub fn next_char_offset(&self, offset: usize) -> usize {
        match self.char_at(offset) {
            Some(c) => offset + c.len_utf8(),
            None => self.len(),
        }
    }

    pub fn prev_char_offset(&self, offset: usize) -> usize {
        match self.char_before(offset) {
            Some(c) => self.clip(offset) - c.len_utf8(),
            None => 0,
        }
    }

    /// Replaces `range` with `text` (newlines normalised). Returns the byte
    /// range of the inserted text.
    pub fn edit(&mut self, range: Range<usize>, text: &str) -> Range<usize> {
        let start = self.clip(range.start);
        let end = self.clip(range.end.max(start));
        let text = normalize_newlines(text);
        if start != end {
            let char_start = self.rope.byte_to_char(start);
            let char_end = self.rope.byte_to_char(end);
            self.rope.remove(char_start..char_end);
        }
        if !text.is_empty() {
            let char_start = self.rope.byte_to_char(start);
            self.rope.insert(char_start, &text);
        }
        self.version += 1;
        start..start + text.len()
    }

    /// Replaces the whole text.
    pub fn set_text(&mut self, text: &str) {
        self.rope = Rope::from_str(&normalize_newlines(text));
        self.version += 1;
    }

    /// Leading whitespace (spaces/tabs) of `row`.
    pub fn indent_of(&self, row: usize) -> String {
        self.line(row)
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect()
    }

    pub fn is_blank_line(&self, row: usize) -> bool {
        self.line(row).trim().is_empty()
    }
}

/// `\r\n` and lone `\r` become `\n`.
pub fn normalize_newlines(text: &str) -> Cow<'_, str> {
    if !text.contains('\r') {
        return Cow::Borrowed(text);
    }
    Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_offsets() {
        let buffer = Buffer::new("ab\ncde\n");
        assert_eq!(buffer.line_count(), 3);
        assert_eq!(buffer.line(0), "ab");
        assert_eq!(buffer.line(1), "cde");
        assert_eq!(buffer.line(2), "");
        assert_eq!(buffer.line_start(1), 3);
        assert_eq!(buffer.line_end(1), 6);
        assert_eq!(buffer.offset_to_point(4), Point::new(1, 1));
        assert_eq!(buffer.point_to_offset(Point::new(1, 99)), 6);
        assert_eq!(buffer.point_to_offset(Point::new(9, 0)), 7);
    }

    #[test]
    fn crlf_is_normalised() {
        let buffer = Buffer::new("a\r\nb\rc");
        assert_eq!(buffer.text(), "a\nb\nc");
        assert_eq!(buffer.line_count(), 3);
    }

    #[test]
    fn unicode_offsets_clip_to_char_boundaries() {
        let buffer = Buffer::new("héllo");
        assert_eq!(buffer.clip(2), 1);
        assert_eq!(buffer.char_at(1), Some('é'));
        assert_eq!(buffer.next_char_offset(1), 3);
        assert_eq!(buffer.prev_char_offset(3), 1);
        assert_eq!(buffer.char_before(0), None);
    }

    #[test]
    fn edits_replace_ranges_and_bump_version() {
        let mut buffer = Buffer::new("hello world");
        let inserted = buffer.edit(6..11, "there\r\nfriend");
        assert_eq!(buffer.text(), "hello there\nfriend");
        assert_eq!(inserted, 6..18);
        assert_eq!(buffer.version(), 1);
        buffer.edit(0..0, ">");
        assert_eq!(buffer.text(), ">hello there\nfriend");
        buffer.edit(0..1, "");
        assert_eq!(buffer.text(), "hello there\nfriend");
    }

    #[test]
    fn indent_helpers() {
        let buffer = Buffer::new("\t  x\n   \n");
        assert_eq!(buffer.indent_of(0), "\t  ");
        assert!(buffer.is_blank_line(1));
        assert!(!buffer.is_blank_line(0));
    }
}
