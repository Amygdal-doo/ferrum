//! Find / replace in a single text: plain, case-sensitive, whole-word and
//! regex queries compiled to one [`regex::Regex`].

use std::ops::Range;

use regex::{Regex, RegexBuilder};

/// Most matches reported for one buffer, so a one-letter query in a huge
/// file stays responsive.
pub const MAX_MATCHES: usize = 100_000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchQuery {
    pub pattern: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FindError {
    #[error("empty search")]
    Empty,
    #[error("invalid regex: {0}")]
    InvalidRegex(String),
}

impl SearchQuery {
    pub fn plain(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            ..Self::default()
        }
    }

    pub fn case_sensitive(mut self, on: bool) -> Self {
        self.case_sensitive = on;
        self
    }

    pub fn whole_word(mut self, on: bool) -> Self {
        self.whole_word = on;
        self
    }

    pub fn regex(mut self, on: bool) -> Self {
        self.regex = on;
        self
    }

    /// Compiles the query. Plain patterns are escaped; whole-word wraps the
    /// pattern in `\b` only on sides that start/end with a word character so
    /// `foo(` still matches.
    pub fn compile(&self) -> Result<CompiledQuery, FindError> {
        if self.pattern.is_empty() {
            return Err(FindError::Empty);
        }
        let body = if self.regex {
            self.pattern.clone()
        } else {
            regex::escape(&self.pattern)
        };
        let body = if self.whole_word {
            let starts_word = self.regex || self.pattern.chars().next().is_some_and(is_word_char);
            let ends_word = self.regex || self.pattern.chars().last().is_some_and(is_word_char);
            format!(
                "{}(?:{body}){}",
                if starts_word { r"\b" } else { "" },
                if ends_word { r"\b" } else { "" }
            )
        } else {
            body
        };
        let regex = RegexBuilder::new(&body)
            .case_insensitive(!self.case_sensitive)
            .multi_line(true)
            .size_limit(1 << 24)
            .build()
            .map_err(|error| FindError::InvalidRegex(error.to_string()))?;
        Ok(CompiledQuery {
            regex,
            is_regex: self.regex,
        })
    }
}

pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[derive(Debug, Clone)]
pub struct CompiledQuery {
    regex: Regex,
    is_regex: bool,
}

impl CompiledQuery {
    pub fn regex(&self) -> &Regex {
        &self.regex
    }

    /// Non-empty match ranges (byte offsets), capped at [`MAX_MATCHES`].
    pub fn find_all(&self, text: &str) -> Vec<Range<usize>> {
        self.regex
            .find_iter(text)
            .filter(|m| !m.range().is_empty())
            .take(MAX_MATCHES)
            .map(|m| m.range())
            .collect()
    }

    /// The replacement for the match at `range`: `$1`/`${name}` expand for
    /// regex queries, plain queries insert `replacement` literally.
    pub fn expand_replacement(&self, text: &str, range: Range<usize>, replacement: &str) -> String {
        if !self.is_regex {
            return replacement.to_string();
        }
        let Some(captures) = self.regex.captures_at(text, range.start) else {
            return replacement.to_string();
        };
        let mut out = String::new();
        captures.expand(&unescape_replacement(replacement), &mut out);
        out
    }

    /// Replaces every match; returns the new text and the replacement count.
    pub fn replace_all(&self, text: &str, replacement: &str) -> (String, usize) {
        let matches = self.find_all(text);
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for range in &matches {
            out.push_str(&text[last..range.start]);
            out.push_str(&self.expand_replacement(text, range.clone(), replacement));
            last = range.end;
        }
        out.push_str(&text[last..]);
        (out, matches.len())
    }
}

/// `\n` and `\t` in a regex replacement mean newline / tab, like most editors.
fn unescape_replacement(replacement: &str) -> String {
    let mut out = String::with_capacity(replacement.len());
    let mut chars = replacement.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Index of the first match at or after `offset` (wrapping to 0).
pub fn next_match_index(matches: &[Range<usize>], offset: usize) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    let index = matches.partition_point(|m| m.start < offset);
    Some(if index == matches.len() { 0 } else { index })
}

/// Index of the last match that starts before `offset` (wrapping to the end).
pub fn previous_match_index(matches: &[Range<usize>], offset: usize) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    let index = matches.partition_point(|m| m.start < offset);
    Some(if index == 0 {
        matches.len() - 1
    } else {
        index - 1
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(query: SearchQuery, text: &str) -> Vec<Range<usize>> {
        query.compile().expect("compiles").find_all(text)
    }

    #[test]
    fn plain_search_is_case_insensitive_by_default() {
        assert_eq!(
            find(SearchQuery::plain("foo"), "Foo foo FOO"),
            vec![0..3, 4..7, 8..11]
        );
    }

    #[test]
    fn case_sensitive_search() {
        assert_eq!(
            find(
                SearchQuery::plain("foo").case_sensitive(true),
                "Foo foo FOO"
            ),
            vec![4..7]
        );
    }

    #[test]
    fn plain_search_escapes_regex_metacharacters() {
        assert_eq!(find(SearchQuery::plain("a.b"), "axb a.b"), vec![4..7]);
        assert_eq!(find(SearchQuery::plain("(x)"), "f(x)"), vec![1..4]);
    }

    #[test]
    fn whole_word() {
        let q = SearchQuery::plain("cat").whole_word(true);
        assert_eq!(find(q, "cat concat cat_x cat."), vec![0..3, 17..20]);
    }

    #[test]
    fn whole_word_with_symbol_edges() {
        let q = SearchQuery::plain("foo(").whole_word(true);
        assert_eq!(find(q, "foo( xfoo("), vec![0..4]);
    }

    #[test]
    fn regex_search_and_invalid_regex() {
        let q = SearchQuery::plain(r"\d+").regex(true);
        assert_eq!(find(q, "a1 b22 c333"), vec![1..2, 4..6, 8..11]);
        assert!(matches!(
            SearchQuery::plain("(").regex(true).compile(),
            Err(FindError::InvalidRegex(_))
        ));
        assert_eq!(
            SearchQuery::plain("").compile().err(),
            Some(FindError::Empty)
        );
    }

    #[test]
    fn regex_anchors_are_per_line() {
        let q = SearchQuery::plain("^x").regex(true);
        assert_eq!(find(q, "x\nyx\nx"), vec![0..1, 5..6]);
    }

    #[test]
    fn empty_regex_matches_are_skipped() {
        let q = SearchQuery::plain("a*").regex(true);
        assert_eq!(find(q, "baab"), vec![1..3]);
    }

    #[test]
    fn replace_all_plain_is_literal() {
        let compiled = SearchQuery::plain("a").compile().expect("compiles");
        assert_eq!(
            compiled.replace_all("banana", "$1"),
            ("b$1n$1n$1".to_string(), 3)
        );
    }

    #[test]
    fn replace_all_regex_expands_groups() {
        let compiled = SearchQuery::plain(r"(\w+)@(\w+)")
            .regex(true)
            .compile()
            .expect("compiles");
        assert_eq!(
            compiled.replace_all("me@home you@work", "$2:$1"),
            ("home:me work:you".to_string(), 2)
        );
        let named = SearchQuery::plain(r"(?P<n>\d)")
            .regex(true)
            .compile()
            .expect("compiles");
        assert_eq!(named.replace_all("a1", "<${n}>").0, "a<1>");
    }

    #[test]
    fn regex_replacement_supports_newline_escape() {
        let compiled = SearchQuery::plain(",")
            .regex(true)
            .compile()
            .expect("compiles");
        assert_eq!(compiled.replace_all("a,b", r"\n").0, "a\nb");
    }

    #[test]
    fn expand_single_replacement() {
        let compiled = SearchQuery::plain(r"(\d)")
            .regex(true)
            .compile()
            .expect("compiles");
        let text = "x1 y2";
        let matches = compiled.find_all(text);
        assert_eq!(
            compiled.expand_replacement(text, matches[1].clone(), "[$1]"),
            "[2]"
        );
    }

    #[test]
    fn next_and_previous_match_wrap() {
        let matches = vec![2..3, 5..6, 9..10];
        assert_eq!(next_match_index(&matches, 0), Some(0));
        assert_eq!(next_match_index(&matches, 3), Some(1));
        assert_eq!(next_match_index(&matches, 10), Some(0));
        assert_eq!(previous_match_index(&matches, 5), Some(0));
        assert_eq!(previous_match_index(&matches, 2), Some(2));
        assert_eq!(next_match_index(&[], 0), None);
    }
}
