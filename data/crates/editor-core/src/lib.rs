//! Editor model helpers that do not depend on any UI.
//!
//! Offsets are UTF-8 byte offsets into the document. UI layers that work in
//! UTF-16 code units (the webview) must convert with [`utf16_to_byte_offset`].

mod complete;
mod keywords;
mod lex;
mod split;

pub use complete::{
    Catalog, CatalogTable, CompletionItem, CompletionKind, Completions, TableMention, complete,
    mentions_at,
};
pub use lex::{Dialect, Token, TokenKind, tokenize};
pub use split::{Statement, split_statements, statement_at};

/// Syntax-theme scope names for highlighting, as `(byte range, scope)`,
/// sorted and non-overlapping. Plain identifiers and punctuation are left
/// unstyled.
#[must_use]
pub fn highlight_spans(sql: &str, dialect: Dialect) -> Vec<(std::ops::Range<usize>, &'static str)> {
    tokenize(sql, dialect)
        .into_iter()
        .filter_map(|t| {
            let scope = match t.kind {
                TokenKind::Keyword => "keyword",
                TokenKind::Type => "type",
                TokenKind::Constant => "constant",
                TokenKind::Function => "function",
                TokenKind::String => "string",
                TokenKind::Number => "number",
                TokenKind::Comment => "comment",
                TokenKind::Parameter => "variable.special",
                TokenKind::QuotedIdentifier => "property",
                TokenKind::Identifier | TokenKind::Operator | TokenKind::Punctuation => {
                    return None;
                }
            };
            Some((t.range, scope))
        })
        .collect()
}

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

    #[test]
    fn highlight_spans_skip_plain_identifiers() {
        let sql = "select name from t -- x";
        let spans = highlight_spans(sql, Dialect::Sqlite);
        let named: Vec<(&str, &str)> = spans.iter().map(|(r, s)| (&sql[r.clone()], *s)).collect();
        assert_eq!(
            named,
            vec![
                ("select", "keyword"),
                ("from", "keyword"),
                ("-- x", "comment")
            ]
        );
    }
}
