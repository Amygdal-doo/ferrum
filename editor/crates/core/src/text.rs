//! Character classes and boundary searches (graphemes, words, smart home)
//! over a [`Buffer`]. Word motions work within a line and step over the
//! newline when they reach a line edge.

use unicode_segmentation::UnicodeSegmentation;

use crate::buffer::Buffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharClass {
    Whitespace,
    Word,
    Punctuation,
}

pub fn char_class(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

/// Start of the grapheme before `offset` (crosses a newline).
pub fn prev_grapheme(buffer: &Buffer, offset: usize) -> usize {
    let offset = buffer.clip(offset);
    let row = buffer.row_of(offset);
    let start = buffer.line_start(row);
    if offset == start {
        return offset.saturating_sub(1);
    }
    let line = buffer.line(row);
    let col = offset - start;
    let prev = line[..col]
        .grapheme_indices(true)
        .next_back()
        .map_or(0, |(index, _)| index);
    start + prev
}

/// End of the grapheme at `offset` (crosses a newline).
pub fn next_grapheme(buffer: &Buffer, offset: usize) -> usize {
    let offset = buffer.clip(offset);
    let row = buffer.row_of(offset);
    let start = buffer.line_start(row);
    let end = buffer.line_end(row);
    if offset >= end {
        return (offset + 1).min(buffer.len());
    }
    let line = buffer.line(row);
    let col = offset - start;
    let next = line[col..]
        .graphemes(true)
        .next()
        .map_or(line.len(), |g| col + g.len());
    start + next
}

/// Word-left: skips whitespace, then a run of one class. At a line start
/// it moves to the end of the previous line.
pub fn prev_word_start(buffer: &Buffer, offset: usize) -> usize {
    let offset = buffer.clip(offset);
    let row = buffer.row_of(offset);
    let start = buffer.line_start(row);
    if offset == start {
        return offset.saturating_sub(1);
    }
    let line = buffer.line(row);
    let chars: Vec<(usize, char)> = line[..offset - start].char_indices().collect();
    let mut i = chars.len();
    while i > 0 && char_class(chars[i - 1].1) == CharClass::Whitespace {
        i -= 1;
    }
    if i == 0 {
        return start;
    }
    let class = char_class(chars[i - 1].1);
    while i > 0 && char_class(chars[i - 1].1) == class {
        i -= 1;
    }
    start + chars.get(i).map_or(0, |(index, _)| *index)
}

/// Word-right: skips whitespace, then a run of one class. At a line end it
/// moves to the start of the next line.
pub fn next_word_end(buffer: &Buffer, offset: usize) -> usize {
    let offset = buffer.clip(offset);
    let row = buffer.row_of(offset);
    let start = buffer.line_start(row);
    let end = buffer.line_end(row);
    if offset >= end {
        return (offset + 1).min(buffer.len());
    }
    let line = buffer.line(row);
    let rest = &line[offset - start..];
    let mut chars = rest.char_indices().peekable();
    while chars
        .peek()
        .is_some_and(|(_, c)| char_class(*c) == CharClass::Whitespace)
    {
        chars.next();
    }
    let Some(&(_, first)) = chars.peek() else {
        return end;
    };
    let class = char_class(first);
    let mut last_end = rest.len();
    for (index, c) in chars {
        if char_class(c) != class {
            last_end = index;
            break;
        }
    }
    offset + last_end
}

/// The word (or punctuation run, or whitespace run) around `offset`, as used
/// by double-click and Cmd+D.
pub fn word_range_at(buffer: &Buffer, offset: usize) -> std::ops::Range<usize> {
    let offset = buffer.clip(offset);
    let row = buffer.row_of(offset);
    let start = buffer.line_start(row);
    let line = buffer.line(row);
    let col = offset - start;
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    if chars.is_empty() {
        return offset..offset;
    }
    // Prefer the char after the caret; fall back to the one before it.
    let index = chars
        .iter()
        .position(|(i, _)| *i >= col)
        .unwrap_or(chars.len());
    let pivot = if index < chars.len()
        && (index == 0 || char_class(chars[index].1) != CharClass::Whitespace)
    {
        index
    } else if index > 0 {
        index - 1
    } else {
        return offset..offset;
    };
    let pivot = if index < chars.len()
        && index > 0
        && char_class(chars[index].1) != CharClass::Word
        && char_class(chars[index - 1].1) == CharClass::Word
    {
        index - 1
    } else {
        pivot
    };
    let class = char_class(chars[pivot].1);
    let mut lo = pivot;
    while lo > 0 && char_class(chars[lo - 1].1) == class {
        lo -= 1;
    }
    let mut hi = pivot + 1;
    while hi < chars.len() && char_class(chars[hi].1) == class {
        hi += 1;
    }
    let byte = |i: usize| chars.get(i).map_or(line.len(), |(b, _)| *b);
    start + byte(lo)..start + byte(hi)
}

/// Smart home: first non-whitespace column, or the line start when the
/// caret is already there (or the line is blank).
pub fn smart_home(buffer: &Buffer, offset: usize) -> usize {
    let row = buffer.row_of(offset);
    let start = buffer.line_start(row);
    let indent = buffer.indent_of(row).len();
    let first = start + indent;
    if offset == first || first == buffer.line_end(row) {
        start
    } else {
        first
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphemes_cross_lines_and_combining_marks() {
        let buffer = Buffer::new("ae\u{301}\nb");
        assert_eq!(next_grapheme(&buffer, 1), 4);
        assert_eq!(prev_grapheme(&buffer, 4), 1);
        assert_eq!(next_grapheme(&buffer, 4), 5);
        assert_eq!(prev_grapheme(&buffer, 5), 4);
        assert_eq!(prev_grapheme(&buffer, 0), 0);
        assert_eq!(next_grapheme(&buffer, 6), 6);
    }

    #[test]
    fn word_motions() {
        let buffer = Buffer::new("let foo_bar = baz(1);\nnext");
        assert_eq!(next_word_end(&buffer, 0), 3);
        assert_eq!(next_word_end(&buffer, 3), 11);
        assert_eq!(next_word_end(&buffer, 11), 13);
        assert_eq!(next_word_end(&buffer, 17), 18);
        assert_eq!(next_word_end(&buffer, 21), 22);
        assert_eq!(prev_word_start(&buffer, 11), 4);
        assert_eq!(prev_word_start(&buffer, 4), 0);
        assert_eq!(prev_word_start(&buffer, 22), 21);
        assert_eq!(prev_word_start(&buffer, 21), 19);
    }

    #[test]
    fn word_range_for_double_click() {
        let buffer = Buffer::new("foo_bar.baz  x");
        assert_eq!(word_range_at(&buffer, 2), 0..7);
        assert_eq!(word_range_at(&buffer, 7), 0..7);
        assert_eq!(word_range_at(&buffer, 8), 8..11);
        assert_eq!(word_range_at(&buffer, 12), 11..13);
        assert_eq!(word_range_at(&buffer, 0), 0..7);
    }

    #[test]
    fn smart_home_toggles() {
        let buffer = Buffer::new("    code\n   ");
        assert_eq!(smart_home(&buffer, 6), 4);
        assert_eq!(smart_home(&buffer, 4), 0);
        assert_eq!(smart_home(&buffer, 0), 4);
        assert_eq!(smart_home(&buffer, 12), 9);
    }
}
