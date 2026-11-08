//! Editor model helpers that do not depend on any UI.
//!
//! Offsets are UTF-8 byte offsets into the document. UI layers that work in
//! UTF-16 code units (the webview) must convert with [`utf16_to_byte_offset`].

mod split;

pub use split::{Statement, split_statements, statement_at};

/// Converts a UTF-16 code-unit offset (as reported by a browser textarea)
/// into a byte offset, clamped to the nearest char boundary at or before it.
#[must_use]
pub fn utf16_to_byte_offset(text: &str, utf16_offset: usize) -> usize {
    let mut units = 0;
    for (byte_idx, ch) in text.char_indices() {
        if units >= utf16_offset {
            return byte_idx;
        }
        units += ch.len_utf16();
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offsets_map_to_bytes() {
        let s = "aé😀b";
        assert_eq!(utf16_to_byte_offset(s, 0), 0);
        assert_eq!(utf16_to_byte_offset(s, 1), 1);
        assert_eq!(utf16_to_byte_offset(s, 2), 3); // after é (2 bytes, 1 unit)
        assert_eq!(utf16_to_byte_offset(s, 4), 7); // after 😀 (4 bytes, 2 units)
        assert_eq!(utf16_to_byte_offset(s, 99), s.len());
    }
}
