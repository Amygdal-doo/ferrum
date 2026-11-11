//! Converting between buffer byte offsets and LSP positions (line plus a
//! column counted in UTF-16 code units, or in bytes when the server agreed
//! to UTF-8).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::buffer::Buffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PositionEncoding {
    Utf8,
    #[default]
    Utf16,
}

impl PositionEncoding {
    /// From the server's `capabilities.positionEncoding` (UTF-16 if absent).
    pub fn from_capabilities(capabilities: &Value) -> Self {
        match capabilities["positionEncoding"].as_str() {
            Some("utf-8") => PositionEncoding::Utf8,
            _ => PositionEncoding::Utf16,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }

    pub fn to_json(self) -> Value {
        json!({ "line": self.line, "character": self.character })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct LspRange {
    pub start: Position,
    pub end: Position,
}

/// Column of byte `col` within `line`, in the encoding's units.
pub fn column_to_units(line: &str, col: usize, encoding: PositionEncoding) -> u32 {
    let mut col = col.min(line.len());
    while !line.is_char_boundary(col) {
        col -= 1;
    }
    match encoding {
        PositionEncoding::Utf8 => col as u32,
        PositionEncoding::Utf16 => line[..col].encode_utf16().count() as u32,
    }
}

/// Byte column for `units` within `line` (clamped to the line).
pub fn units_to_column(line: &str, units: u32, encoding: PositionEncoding) -> usize {
    let units = units as usize;
    match encoding {
        PositionEncoding::Utf8 => {
            let mut col = units.min(line.len());
            while !line.is_char_boundary(col) {
                col -= 1;
            }
            col
        }
        PositionEncoding::Utf16 => {
            let mut count = 0;
            for (i, c) in line.char_indices() {
                if count >= units {
                    return i;
                }
                count += c.len_utf16();
            }
            line.len()
        }
    }
}

pub fn offset_to_position(buffer: &Buffer, offset: usize, encoding: PositionEncoding) -> Position {
    let offset = buffer.clip(offset.min(buffer.len()));
    let row = buffer.row_of(offset);
    let line = buffer.line(row);
    let col = offset - buffer.line_start(row);
    Position::new(row as u32, column_to_units(&line, col, encoding))
}

/// Buffer offset of an LSP position; lines past the end clamp to the end.
pub fn position_to_offset(
    buffer: &Buffer,
    position: Position,
    encoding: PositionEncoding,
) -> usize {
    let row = position.line as usize;
    if row >= buffer.line_count() {
        return buffer.len();
    }
    let line = buffer.line(row);
    buffer.line_start(row) + units_to_column(&line, position.character, encoding)
}

pub fn range_to_offsets(
    buffer: &Buffer,
    range: LspRange,
    encoding: PositionEncoding,
) -> std::ops::Range<usize> {
    let start = position_to_offset(buffer, range.start, encoding);
    let end = position_to_offset(buffer, range.end, encoding).max(start);
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_columns_count_surrogate_pairs() {
        let buffer = Buffer::new("a😀b\nčx");
        let b = 1 + 4; // byte offset of 'b'
        let p = offset_to_position(&buffer, b, PositionEncoding::Utf16);
        assert_eq!(p, Position::new(0, 3));
        assert_eq!(position_to_offset(&buffer, p, PositionEncoding::Utf16), b);
        let p8 = offset_to_position(&buffer, b, PositionEncoding::Utf8);
        assert_eq!(p8, Position::new(0, 5));
        assert_eq!(position_to_offset(&buffer, p8, PositionEncoding::Utf8), b);
        let x = buffer.len() - 1;
        assert_eq!(
            offset_to_position(&buffer, x, PositionEncoding::Utf16),
            Position::new(1, 1)
        );
    }

    #[test]
    fn out_of_range_positions_clamp() {
        let buffer = Buffer::new("ab\ncd");
        assert_eq!(
            position_to_offset(&buffer, Position::new(0, 99), PositionEncoding::Utf16),
            2
        );
        assert_eq!(
            position_to_offset(&buffer, Position::new(9, 0), PositionEncoding::Utf16),
            5
        );
        // Inside a multi-byte char (UTF-8 units) snaps back to its start.
        let buffer = Buffer::new("é");
        assert_eq!(
            position_to_offset(&buffer, Position::new(0, 1), PositionEncoding::Utf8),
            0
        );
    }

    #[test]
    fn encoding_from_capabilities() {
        let caps = serde_json::json!({ "positionEncoding": "utf-8" });
        assert_eq!(
            PositionEncoding::from_capabilities(&caps),
            PositionEncoding::Utf8
        );
        assert_eq!(
            PositionEncoding::from_capabilities(&serde_json::json!({})),
            PositionEncoding::Utf16
        );
    }
}
