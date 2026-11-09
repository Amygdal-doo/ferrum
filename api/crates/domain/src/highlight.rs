//! Pure helpers behind the response body view: find-in-text, a lenient JSON
//! tokenizer for syntax colouring, and layering of styled byte ranges. All
//! ranges are byte offsets on char boundaries, sorted and non-overlapping.

use std::ops::Range;

/// Syntax class of a JSON token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonToken {
    /// A string in key position (followed by `:`).
    Key,
    String,
    Number,
    Bool,
    Null,
}

/// Non-overlapping occurrences of `needle`, left to right, at most `limit`.
/// Case-insensitive matching folds Unicode lowercase per char. An empty
/// needle matches nothing.
#[must_use]
pub fn find_matches(
    haystack: &str,
    needle: &str,
    case_sensitive: bool,
    limit: usize,
) -> Vec<Range<usize>> {
    if needle.is_empty() || limit == 0 {
        return Vec::new();
    }
    if case_sensitive {
        return haystack
            .match_indices(needle)
            .take(limit)
            .map(|(i, m)| i..i + m.len())
            .collect();
    }
    let needle: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
    let mut out = Vec::new();
    let mut from = 0;
    while from < haystack.len() && out.len() < limit {
        let Some(len) = folded_prefix_len(&haystack[from..], &needle) else {
            from += haystack[from..].chars().next().map_or(1, char::len_utf8);
            continue;
        };
        out.push(from..from + len);
        from += len;
    }
    out
}

/// Byte length of the prefix of `text` that equals `needle` (already
/// lowercased) under per-char lowercase folding.
fn folded_prefix_len(text: &str, needle: &[char]) -> Option<usize> {
    let mut want = needle.iter();
    let mut len = 0;
    for c in text.chars() {
        for lower in c.to_lowercase() {
            if want.next() != Some(&lower) {
                return None;
            }
        }
        len += c.len_utf8();
        if want.len() == 0 {
            return Some(len);
        }
    }
    None
}

/// Colourable tokens of `text`, read leniently (invalid JSON still yields
/// tokens for what looks like strings, numbers and literals). Punctuation
/// and whitespace are not reported.
#[must_use]
pub fn json_tokens(text: &str) -> Vec<(Range<usize>, JsonToken)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let start = i;
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(bytes.len());
                let rest = bytes[i..].iter().find(|b| !b.is_ascii_whitespace());
                let kind = if rest == Some(&b':') {
                    JsonToken::Key
                } else {
                    JsonToken::String
                };
                out.push((start..i, kind));
            }
            b'-' | b'0'..=b'9' => {
                let start = i;
                i += 1;
                while i < bytes.len()
                    && matches!(bytes[i], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    i += 1;
                }
                out.push((start..i, JsonToken::Number));
            }
            b't' | b'f' | b'n' => {
                let literal = [
                    ("true", JsonToken::Bool),
                    ("false", JsonToken::Bool),
                    ("null", JsonToken::Null),
                ]
                .into_iter()
                .find(|(word, _)| bytes[i..].starts_with(word.as_bytes()));
                match literal {
                    Some((word, kind)) => {
                        out.push((i..i + word.len(), kind));
                        i += word.len();
                    }
                    None => i += 1,
                }
            }
            _ => i += 1,
        }
    }
    out
}

/// Lays `overlay` spans over `base` spans. Where both cover a byte the
/// styles are merged with `combine(base, overlay)`; elsewhere each keeps its
/// own style. Both inputs and the output are sorted and non-overlapping.
#[must_use]
pub fn layer_spans<S: Clone>(
    base: &[(Range<usize>, S)],
    overlay: &[(Range<usize>, S)],
    combine: impl Fn(&S, &S) -> S,
) -> Vec<(Range<usize>, S)> {
    let mut out = Vec::with_capacity(base.len() + overlay.len() * 2);
    let mut base = base.iter().cloned().peekable();
    // Unconsumed tail of a base span cut by the previous overlay span.
    let mut pending: Option<(Range<usize>, S)> = None;
    for (top, top_style) in overlay {
        let mut pos = top.start;
        loop {
            let Some((range, style)) = pending.take().or_else(|| base.next()) else {
                break;
            };
            if range.start >= top.end {
                pending = Some((range, style));
                break;
            }
            if range.end <= top.start {
                out.push((range, style));
                continue;
            }
            if range.start < top.start {
                out.push((range.start..top.start, style.clone()));
            }
            let start = range.start.max(top.start);
            if start > pos {
                out.push((pos..start, top_style.clone()));
            }
            let end = range.end.min(top.end);
            out.push((start..end, combine(&style, top_style)));
            pos = end;
            if range.end > top.end {
                pending = Some((top.end..range.end, style));
                break;
            }
        }
        if pos < top.end {
            out.push((pos..top.end, top_style.clone()));
        }
    }
    out.extend(pending);
    out.extend(base);
    out
}

/// Zero-based line number of byte `offset` in `text`.
#[must_use]
pub fn line_of(text: &str, offset: usize) -> usize {
    let end = offset.min(text.len());
    text.as_bytes()[..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

#[cfg(test)]
#[path = "highlight_tests.rs"]
mod tests;
