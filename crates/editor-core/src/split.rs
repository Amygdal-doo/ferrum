//! Lexical statement splitter. Understands quotes, identifiers, line and
//! block comments, and PostgreSQL dollar-quoting. It is not a parser and is
//! never used for security decisions.

use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// Byte range of the trimmed statement text, excluding the `;`.
    pub range: Range<usize>,
}

impl Statement {
    #[must_use]
    pub fn text<'a>(&self, doc: &'a str) -> &'a str {
        &doc[self.range.clone()]
    }
}

/// Splits `sql` on top-level semicolons. Empty statements are dropped.
#[must_use]
pub fn split_statements(sql: &str) -> Vec<Statement> {
    let bytes = sql.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' | b'"' | b'`' => i = skip_quoted(bytes, i),
            b'-' if bytes.get(i + 1) == Some(&b'-') => i = skip_line(bytes, i),
            b'/' if bytes.get(i + 1) == Some(&b'*') => i = skip_block(bytes, i),
            b'$' => i = skip_dollar(sql, i),
            b';' => {
                push_trimmed(sql, start..i, &mut out);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    push_trimmed(sql, start..sql.len(), &mut out);
    out
}

/// The statement containing (or immediately preceding) the cursor.
#[must_use]
pub fn statement_at(sql: &str, cursor: usize) -> Option<Statement> {
    let statements = split_statements(sql);
    statements
        .iter()
        .find(|s| cursor >= s.range.start && cursor <= s.range.end)
        .or_else(|| statements.iter().rev().find(|s| s.range.end <= cursor))
        .or_else(|| statements.first())
        .cloned()
}

fn push_trimmed(sql: &str, range: Range<usize>, out: &mut Vec<Statement>) {
    let raw = &sql[range.clone()];
    let lead = raw.len() - raw.trim_start().len();
    let trimmed = raw.trim();
    if trimmed.is_empty() || is_only_comments(trimmed) {
        return;
    }
    let start = range.start + lead;
    out.push(Statement {
        range: start..start + trimmed.len(),
    });
}

fn is_only_comments(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
        } else if b[i] == b'-' && b.get(i + 1) == Some(&b'-') {
            i = skip_line(b, i);
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            i = skip_block(b, i);
        } else {
            return false;
        }
    }
    true
}

fn skip_quoted(b: &[u8], open: usize) -> usize {
    let q = b[open];
    let mut i = open + 1;
    while i < b.len() {
        if b[i] == q {
            // Doubled quote is an escaped quote.
            if b.get(i + 1) == Some(&q) {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    b.len()
}

fn skip_line(b: &[u8], i: usize) -> usize {
    b[i..]
        .iter()
        .position(|&c| c == b'\n')
        .map_or(b.len(), |p| i + p + 1)
}

fn skip_block(b: &[u8], i: usize) -> usize {
    b[i + 2..]
        .windows(2)
        .position(|w| w == b"*/")
        .map_or(b.len(), |p| i + 2 + p + 2)
}

/// `$tag$ ... $tag$` (tag may be empty). A `$1` placeholder is not a quote.
fn skip_dollar(sql: &str, i: usize) -> usize {
    let rest = &sql[i + 1..];
    let tag_len = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    let tag = &rest[..tag_len];
    let starts_with_digit = tag.chars().next().is_some_and(|c| c.is_ascii_digit());
    if starts_with_digit || rest.as_bytes().get(tag_len) != Some(&b'$') {
        return i + 1;
    }
    let delim = format!("${tag}$");
    let body_start = i + delim.len();
    sql[body_start..]
        .find(&delim)
        .map_or(sql.len(), |p| body_start + p + delim.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(sql: &str) -> Vec<&str> {
        split_statements(sql).iter().map(|s| s.text(sql)).collect()
    }

    #[test]
    fn splits_simple_statements() {
        assert_eq!(
            texts("select 1; select 2 ;\n\n"),
            vec!["select 1", "select 2"]
        );
    }

    #[test]
    fn ignores_semicolons_in_strings_comments_and_identifiers() {
        let sql = "select ';' as \"a;b\" -- c;\n; /* d; */ select 'it''s;'";
        assert_eq!(
            texts(sql),
            vec!["select ';' as \"a;b\" -- c;", "/* d; */ select 'it''s;'"]
        );
    }

    #[test]
    fn handles_dollar_quoting_but_not_placeholders() {
        let sql = "create function f() returns int as $body$ begin; return 1; end $body$ language plpgsql; select $1";
        let t = texts(sql);
        assert_eq!(t.len(), 2);
        assert!(t[0].ends_with("plpgsql"));
        assert_eq!(t[1], "select $1");
        assert_eq!(texts("select $$a;b$$; select 2").len(), 2);
    }

    #[test]
    fn comment_only_chunks_are_dropped() {
        assert_eq!(texts("-- nothing\n; /* x */ ;select 1"), vec!["select 1"]);
    }

    #[test]
    fn statement_at_cursor() {
        let sql = "select 1;\nselect 2;\n";
        assert_eq!(
            statement_at(sql, 0).map(|s| s.text(sql).to_owned()),
            Some("select 1".into())
        );
        assert_eq!(
            statement_at(sql, 12).map(|s| s.text(sql).to_owned()),
            Some("select 2".into())
        );
        // Cursor in trailing whitespace picks the previous statement.
        assert_eq!(
            statement_at(sql, sql.len()).map(|s| s.text(sql).to_owned()),
            Some("select 2".into())
        );
        assert_eq!(statement_at("", 0), None);
    }

    #[test]
    fn unterminated_constructs_do_not_panic() {
        for sql in ["select 'abc", "select /* x", "select $tag$ x", "\"x", "--"] {
            let _ = split_statements(sql);
        }
    }
}
