//! SQL tokenizer for highlighting and completion. Lexical only (no parser):
//! it understands each dialect's quoting and comments, and classifies words
//! with the keyword tables in `keywords.rs`.

use crate::keywords::{is_constant, is_keyword, is_type};
use std::ops::Range;

/// Quoting rules differ per engine; everything else is shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dialect {
    /// `"ident"`, `$tag$ … $tag$` strings, `E'…'` escapes, `$1` parameters.
    #[default]
    Postgres,
    /// `` `ident` ``, backslash escapes in strings, `#` line comments.
    MySql,
    /// `"ident"`, `` `ident` `` and `[ident]`; `?`, `:name`, `@name` parameters.
    Sqlite,
}

impl Dialect {
    /// Quotes `name` so it is used verbatim.
    #[must_use]
    pub fn quote(self, name: &str) -> String {
        match self {
            Self::MySql => format!("`{}`", name.replace('`', "``")),
            Self::Postgres | Self::Sqlite => format!("\"{}\"", name.replace('"', "\"\"")),
        }
    }

    /// `name` as written in SQL: bare when that resolves to the same object,
    /// otherwise quoted (PostgreSQL folds bare names to lower case).
    #[must_use]
    pub fn ident(self, name: &str) -> String {
        let mut chars = name.chars();
        let first_ok = chars
            .next()
            .is_some_and(|c| c == '_' || c.is_ascii_alphabetic());
        let rest_ok = name
            .chars()
            .all(|c| c == '_' || c.is_ascii_alphanumeric() || (self == Self::MySql && c == '$'));
        let case_ok = self != Self::Postgres || !name.chars().any(|c| c.is_ascii_uppercase());
        if first_ok && rest_ok && case_ok && !is_keyword(name) {
            name.to_owned()
        } else {
            self.quote(name)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    /// A built-in type name (`INTEGER`, `varchar`, …).
    Type,
    /// `NULL`, `TRUE`, `FALSE`, `CURRENT_DATE`, …
    Constant,
    /// A bare word followed by `(`.
    Function,
    Identifier,
    QuotedIdentifier,
    String,
    Number,
    Comment,
    /// `$1`, `?`, `:name`, `@name`.
    Parameter,
    Operator,
    /// `(`, `)`, `,`, `;`, `.`
    Punctuation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub range: Range<usize>,
}

impl Token {
    #[must_use]
    pub fn text<'a>(&self, doc: &'a str) -> &'a str {
        &doc[self.range.clone()]
    }

    /// The identifier's name: quotes removed and doubled quotes collapsed.
    #[must_use]
    pub fn ident_name(&self, doc: &str) -> String {
        let text = self.text(doc);
        if self.kind != TokenKind::QuotedIdentifier || text.len() < 2 {
            return text.to_owned();
        }
        let (open, inner) = text.split_at(1);
        let close = match open {
            "[" => "]",
            q => q,
        };
        let inner = inner.strip_suffix(close).unwrap_or(inner);
        inner.replace(&close.repeat(2), close)
    }

    #[must_use]
    pub fn is_word(&self) -> bool {
        matches!(
            self.kind,
            TokenKind::Keyword
                | TokenKind::Type
                | TokenKind::Constant
                | TokenKind::Function
                | TokenKind::Identifier
        )
    }
}

/// Tokens of `sql` in order; whitespace is skipped. Unterminated strings and
/// comments run to the end of the text.
#[must_use]
pub fn tokenize(sql: &str, dialect: Dialect) -> Vec<Token> {
    let b = sql.as_bytes();
    let mut out: Vec<Token> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let start = i;
        let kind = match c {
            _ if c.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            b'-' if b.get(i + 1) == Some(&b'-') => {
                i = line_end(b, i);
                TokenKind::Comment
            }
            b'#' if dialect == Dialect::MySql => {
                i = line_end(b, i);
                TokenKind::Comment
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = b[i + 2..]
                    .windows(2)
                    .position(|w| w == b"*/")
                    .map_or(b.len(), |p| i + 2 + p + 2);
                TokenKind::Comment
            }
            b'\'' => {
                let backslash = dialect == Dialect::MySql || escape_prefix(b, &out, start);
                i = quoted_end(b, i, b'\'', backslash);
                // `E'…'`, `X'…'`, `N'…'`: the prefix word joins the string.
                if let Some(prev) = out.last().filter(|t| string_prefix(sql, t, start)) {
                    let begin = prev.range.start;
                    out.pop();
                    out.push(Token {
                        kind: TokenKind::String,
                        range: begin..i,
                    });
                    continue;
                }
                TokenKind::String
            }
            b'"' if dialect == Dialect::MySql => {
                i = quoted_end(b, i, b'"', true);
                TokenKind::String
            }
            b'"' => {
                i = quoted_end(b, i, b'"', false);
                TokenKind::QuotedIdentifier
            }
            b'`' if dialect != Dialect::Postgres => {
                i = quoted_end(b, i, b'`', false);
                TokenKind::QuotedIdentifier
            }
            b'[' if dialect == Dialect::Sqlite => {
                i = b[i..]
                    .iter()
                    .position(|&x| x == b']')
                    .map_or(b.len(), |p| i + p + 1);
                TokenKind::QuotedIdentifier
            }
            b'$' if dialect == Dialect::Postgres => match dollar_end(sql, i) {
                Some(end) => {
                    i = end;
                    TokenKind::String
                }
                None => {
                    i += 1;
                    i += digits(b, i);
                    TokenKind::Parameter
                }
            },
            b'?' => {
                i += 1;
                i += digits(b, i);
                TokenKind::Parameter
            }
            b':' | b'@' if b.get(i + 1).is_some_and(|&n| is_word_start(n)) => {
                // `::type` casts in PostgreSQL are operators, not parameters.
                if c == b':' && i > 0 && b[i - 1] == b':' {
                    i += 1;
                    TokenKind::Operator
                } else {
                    i += 1;
                    i += word_len(b, i);
                    TokenKind::Parameter
                }
            }
            b'0'..=b'9' => {
                i = number_end(b, i);
                TokenKind::Number
            }
            b'.' if b.get(i + 1).is_some_and(u8::is_ascii_digit)
                && !out
                    .last()
                    .is_some_and(|t| t.range.end == i && t.kind != TokenKind::Operator) =>
            {
                i = number_end(b, i);
                TokenKind::Number
            }
            b'(' | b')' | b',' | b';' | b'.' => {
                i += 1;
                TokenKind::Punctuation
            }
            _ if is_word_start(c) => {
                i += word_len(b, i);
                let word = &sql[start..i];
                if is_constant(word) {
                    TokenKind::Constant
                } else if next_non_space(b, i) == Some(b'(') && !is_keyword(word) {
                    TokenKind::Function
                } else if is_type(word) {
                    TokenKind::Type
                } else if is_keyword(word) {
                    TokenKind::Keyword
                } else {
                    TokenKind::Identifier
                }
            }
            _ => {
                // One operator run (`<=`, `||`, `->>`); a lone non-ASCII char
                // is consumed whole so ranges stay on char boundaries.
                let ch_len = sql[i..].chars().next().map_or(1, char::len_utf8);
                i += ch_len;
                while i < b.len()
                    && is_operator(b[i])
                    && !matches!(b.get(i..i + 2), Some(b"--" | b"/*"))
                {
                    i += 1;
                }
                TokenKind::Operator
            }
        };
        out.push(Token {
            kind,
            range: start..i,
        });
    }
    out
}

fn is_word_start(c: u8) -> bool {
    c == b'_' || c.is_ascii_alphabetic() || c >= 0x80
}

fn word_len(b: &[u8], i: usize) -> usize {
    b[i..]
        .iter()
        .position(|&c| !(c == b'_' || c == b'$' || c.is_ascii_alphanumeric() || c >= 0x80))
        .unwrap_or(b.len() - i)
}

fn digits(b: &[u8], i: usize) -> usize {
    b[i..]
        .iter()
        .position(|c| !c.is_ascii_digit())
        .unwrap_or(b.len() - i)
}

fn is_operator(c: u8) -> bool {
    b"+-*/<>=!~^&|%:#@?".contains(&c)
}

fn next_non_space(b: &[u8], i: usize) -> Option<u8> {
    b[i..].iter().copied().find(|c| !c.is_ascii_whitespace())
}

fn line_end(b: &[u8], i: usize) -> usize {
    b[i..]
        .iter()
        .position(|&c| c == b'\n')
        .map_or(b.len(), |p| i + p)
}

/// Digits, one fraction, an exponent; `0x…` hex.
fn number_end(b: &[u8], mut i: usize) -> usize {
    if b[i] == b'0' && matches!(b.get(i + 1), Some(b'x' | b'X')) {
        i += 2;
        return i + b[i..]
            .iter()
            .position(|c| !c.is_ascii_hexdigit())
            .unwrap_or(b.len() - i);
    }
    i += digits(b, i);
    if b.get(i) == Some(&b'.') {
        i += 1;
        i += digits(b, i);
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(b.get(i + 1), Some(b'+' | b'-')));
        if b.get(i + 1 + sign).is_some_and(u8::is_ascii_digit) {
            i += 1 + sign;
            i += digits(b, i);
        }
    }
    i
}

/// End of a quoted run opened at `open`: doubled quotes always escape,
/// backslashes too when `backslash` is set.
fn quoted_end(b: &[u8], open: usize, quote: u8, backslash: bool) -> usize {
    let mut i = open + 1;
    while i < b.len() {
        if backslash && b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == quote {
            if b.get(i + 1) == Some(&quote) {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    b.len()
}

/// A one-letter `E`/`X`/`B`/`N` word directly before the quote.
fn string_prefix(sql: &str, token: &Token, quote: usize) -> bool {
    token.range.end == quote
        && token.is_word()
        && matches!(
            token.text(sql).to_ascii_uppercase().as_str(),
            "E" | "X" | "B" | "N"
        )
}

/// PostgreSQL `E'…'` strings use backslash escapes.
fn escape_prefix(b: &[u8], out: &[Token], quote: usize) -> bool {
    out.last().is_some_and(|t| {
        t.range.end == quote && t.range.len() == 1 && matches!(b[t.range.start], b'E' | b'e')
    })
}

/// `$tag$ … $tag$` (empty tag allowed); `None` if `i` is not a dollar quote.
fn dollar_end(sql: &str, i: usize) -> Option<usize> {
    let rest = &sql[i + 1..];
    let tag_len = rest.find('$')?;
    let tag = &rest[..tag_len];
    if !tag.chars().all(|c| c == '_' || c.is_alphanumeric())
        || tag.starts_with(|c: char| c.is_ascii_digit())
    {
        return None;
    }
    let delim = format!("${tag}$");
    let body = i + delim.len();
    Some(
        sql[body..]
            .find(&delim)
            .map_or(sql.len(), |p| body + p + delim.len()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(sql: &str, dialect: Dialect) -> Vec<(TokenKind, &str)> {
        tokenize(sql, dialect)
            .into_iter()
            .map(|t| (t.kind, &sql[t.range]))
            .collect()
    }

    #[test]
    fn classifies_words_strings_and_numbers() {
        use TokenKind as K;
        assert_eq!(
            kinds(
                "SELECT count(*), 'it''s', 1.5e3 FROM t WHERE x IS NULL",
                Dialect::Postgres
            ),
            vec![
                (K::Keyword, "SELECT"),
                (K::Function, "count"),
                (K::Punctuation, "("),
                (K::Operator, "*"),
                (K::Punctuation, ")"),
                (K::Punctuation, ","),
                (K::String, "'it''s'"),
                (K::Punctuation, ","),
                (K::Number, "1.5e3"),
                (K::Keyword, "FROM"),
                (K::Identifier, "t"),
                (K::Keyword, "WHERE"),
                (K::Identifier, "x"),
                (K::Keyword, "IS"),
                (K::Constant, "NULL"),
            ]
        );
    }

    #[test]
    fn comments_and_unterminated_runs() {
        use TokenKind as K;
        assert_eq!(
            kinds("a -- c\n/* b */ 'open", Dialect::Sqlite),
            vec![
                (K::Identifier, "a"),
                (K::Comment, "-- c"),
                (K::Comment, "/* b */"),
                (K::String, "'open"),
            ]
        );
    }

    #[test]
    fn postgres_dollar_quotes_casts_and_params() {
        use TokenKind as K;
        assert_eq!(
            kinds("$f$ a; 'b' $f$ $1::int E'\\''", Dialect::Postgres),
            vec![
                (K::String, "$f$ a; 'b' $f$"),
                (K::Parameter, "$1"),
                (K::Operator, "::"),
                (K::Type, "int"),
                (K::String, "E'\\''"),
            ]
        );
    }

    #[test]
    fn mysql_backticks_hash_comments_and_backslashes() {
        use TokenKind as K;
        assert_eq!(
            kinds("`my tab` # x\n'a\\'b' \"s\"", Dialect::MySql),
            vec![
                (K::QuotedIdentifier, "`my tab`"),
                (K::Comment, "# x"),
                (K::String, "'a\\'b'"),
                (K::String, "\"s\""),
            ]
        );
    }

    #[test]
    fn quoted_names_unescape() {
        let sql = r#""we""ird" [x y]"#;
        let tokens = tokenize(sql, Dialect::Sqlite);
        assert_eq!(tokens[0].ident_name(sql), "we\"ird");
        assert_eq!(tokens[1].ident_name(sql), "x y");
    }

    #[test]
    fn non_ascii_text_keeps_char_boundaries() {
        let sql = "SELECT 'é' § ünïcode";
        for t in tokenize(sql, Dialect::Postgres) {
            assert!(sql.is_char_boundary(t.range.start) && sql.is_char_boundary(t.range.end));
        }
    }

    #[test]
    fn identifiers_are_quoted_only_when_needed() {
        assert_eq!(Dialect::Postgres.ident("users"), "users");
        assert_eq!(Dialect::Postgres.ident("User"), "\"User\"");
        assert_eq!(Dialect::Postgres.ident("order"), "\"order\"");
        assert_eq!(Dialect::Sqlite.ident("User"), "User");
        assert_eq!(Dialect::MySql.ident("my col"), "`my col`");
    }
}
