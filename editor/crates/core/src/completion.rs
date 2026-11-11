//! Autocomplete without a window: the word being typed, words collected
//! from open files, ranking candidates against what was typed, and LSP
//! snippet syntax the snippet engine does not know.

use std::collections::HashSet;

use crate::buffer::Buffer;
use crate::fuzzy::fuzzy_match;

/// Shortest word offered from buffer text.
const MIN_WORD_LEN: usize = 3;

pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Start of the identifier that ends at `offset` (or `offset`).
pub fn word_start(buffer: &Buffer, offset: usize) -> usize {
    let row = buffer.row_of(offset);
    let line_start = buffer.line_start(row);
    let before = buffer.slice_cow(line_start..offset);
    let len: usize = before
        .chars()
        .rev()
        .take_while(|c| is_word_char(*c))
        .map(char::len_utf8)
        .sum();
    offset - len
}

/// Words in `text` that fuzzy-match `prefix`, excluding `prefix` itself,
/// in order of first appearance, at most `limit`.
pub fn buffer_words(
    text: &str,
    prefix: &str,
    seen: &mut HashSet<String>,
    limit: usize,
) -> Vec<String> {
    let mut out = Vec::new();
    let first = prefix
        .chars()
        .next()
        .map(|c| c.to_lowercase().collect::<String>());
    for word in text.split(|c: char| !is_word_char(c)) {
        if out.len() >= limit {
            break;
        }
        if word.len() < MIN_WORD_LEN
            || word == prefix
            || word.starts_with(|c: char| c.is_ascii_digit())
            || seen.contains(word)
        {
            continue;
        }
        // Cheap pre-check: the first letter must match.
        if let Some(first) = &first
            && !word
                .chars()
                .next()
                .is_some_and(|c| c.to_lowercase().collect::<String>() == *first)
        {
            continue;
        }
        if fuzzy_match(prefix, word).is_some() {
            seen.insert(word.to_string());
            out.push(word.to_string());
        }
    }
    out
}

/// What the ranking needs from a candidate.
pub struct Candidate<'a> {
    pub filter_text: &'a str,
    pub sort_text: &'a str,
    /// Language server results rank above buffer words.
    pub from_server: bool,
    pub preselect: bool,
}

/// Indices of candidates matching `query`, best first: prefix matches,
/// then fuzzy score, then the server's own order.
pub fn rank(query: &str, candidates: &[Candidate]) -> Vec<usize> {
    let lower = query.to_lowercase();
    let mut scored: Vec<(usize, (bool, bool, bool, i64))> = candidates
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let m = fuzzy_match(query, c.filter_text)?;
            let prefix = !query.is_empty() && c.filter_text.to_lowercase().starts_with(&lower);
            Some((
                i,
                (
                    prefix,
                    c.preselect && query.is_empty(),
                    c.from_server,
                    m.score,
                ),
            ))
        })
        .collect();
    scored.sort_by(|(ia, a), (ib, b)| {
        b.0.cmp(&a.0)
            .then(b.1.cmp(&a.1))
            .then(b.2.cmp(&a.2))
            .then(b.3.cmp(&a.3))
            .then_with(|| candidates[*ia].sort_text.cmp(candidates[*ib].sort_text))
            .then(ia.cmp(ib))
    });
    scored.into_iter().map(|(i, _)| i).collect()
}

/// Rewrites LSP snippet features the snippet engine lacks: choices
/// `${1|a,b|}` become `${1:a}`, variables `$TM_FILENAME` / `${VAR:x}` become
/// their default (or nothing).
pub fn normalize_lsp_snippet(body: &str) -> String {
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            out.push(c);
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c != '$' {
            out.push(c);
            i += 1;
            continue;
        }
        // ${N|a,b|}
        if chars.get(i + 1) == Some(&'{') {
            let digits: String = chars[i + 2..]
                .iter()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            let after = i + 2 + digits.len();
            if !digits.is_empty() && chars.get(after) == Some(&'|') {
                let rest: String = chars[after + 1..].iter().collect();
                if let Some(end) = rest.find("|}") {
                    let first = rest[..end].split(',').next().unwrap_or("");
                    out.push_str(&format!("${{{digits}:{first}}}"));
                    i = after + 1 + rest[..end].chars().count() + 2;
                    continue;
                }
            }
            // ${VAR} / ${VAR:default}
            let name: String = chars[i + 2..]
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
                .collect();
            if name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
                let after = i + 2 + name.chars().count();
                match chars.get(after) {
                    Some('}') => {
                        i = after + 1;
                        continue;
                    }
                    Some(':') => {
                        let mut depth = 1;
                        let mut j = after + 1;
                        let mut default = String::new();
                        while j < chars.len() {
                            match chars[j] {
                                '{' => depth += 1,
                                '}' => {
                                    depth -= 1;
                                    if depth == 0 {
                                        break;
                                    }
                                }
                                _ => {}
                            }
                            default.push(chars[j]);
                            j += 1;
                        }
                        out.push_str(&normalize_lsp_snippet(&default));
                        i = j + 1;
                        continue;
                    }
                    _ => {}
                }
            }
        } else if chars
            .get(i + 1)
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == '_')
        {
            // $VAR
            let len = chars[i + 1..]
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
                .count();
            i += 1 + len;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_start_stops_at_non_word_chars() {
        let buffer = Buffer::new("let foo_bar = self.va");
        let end = buffer.len();
        assert_eq!(buffer.slice(word_start(&buffer, end)..end), "va");
        assert_eq!(word_start(&buffer, 11), 4);
        assert_eq!(word_start(&buffer, 12), 12);
        let buffer = Buffer::new("čevap");
        assert_eq!(word_start(&buffer, buffer.len()), 0);
    }

    #[test]
    fn buffer_words_are_unique_and_skip_the_prefix() {
        let mut seen = HashSet::new();
        let words = buffer_words(
            "render_row render rows Renderer re 9render render",
            "ren",
            &mut seen,
            10,
        );
        assert_eq!(words, vec!["render_row", "render", "Renderer"]);
        let more = buffer_words("render rendering", "ren", &mut seen, 10);
        assert_eq!(more, vec!["rendering"]);
        assert!(buffer_words("alpha beta", "zz", &mut HashSet::new(), 10).is_empty());
    }

    #[test]
    fn ranking_prefers_prefix_then_server_then_score() {
        let c = |filter: &'static str, server: bool| Candidate {
            filter_text: filter,
            sort_text: filter,
            from_server: server,
            preselect: false,
        };
        let candidates = [
            c("to_string", true),
            c("string", false),
            c("strip", true),
            c("str", true),
            c("xyz", true),
        ];
        let ranked = rank("str", &candidates);
        assert_eq!(ranked.len(), 4);
        assert_eq!(&ranked[..2], &[3, 2]);
        assert_eq!(ranked[2], 1);
        assert_eq!(ranked[3], 0);
        // Empty query: preselected first, then server order.
        let mut items = vec![c("b", true), c("a", true)];
        items[0].sort_text = "2";
        items[1].sort_text = "1";
        assert_eq!(rank("", &items), vec![1, 0]);
    }

    #[test]
    fn lsp_snippets_are_normalized() {
        assert_eq!(
            normalize_lsp_snippet("push(${1:value})$0"),
            "push(${1:value})$0"
        );
        assert_eq!(normalize_lsp_snippet("${1|one,two|} x"), "${1:one} x");
        assert_eq!(normalize_lsp_snippet("// $TM_FILENAME end"), "//  end");
        assert_eq!(
            normalize_lsp_snippet("${TM_SELECTED_TEXT:${1:x}}"),
            "${1:x}"
        );
        assert_eq!(normalize_lsp_snippet("a \\$b"), "a \\$b");
        assert_eq!(normalize_lsp_snippet("cost $5"), "cost $5");
    }
}
