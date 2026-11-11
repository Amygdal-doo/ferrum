//! Inline Markdown: emphasis, code, strikethrough, links, images, autolinks
//! and backslash escapes, flattened into styled [`Span`]s.

use super::{Span, SpanStyle};

/// Parses inline content. `\n` in the input is kept as a line break.
pub fn parse_inline(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    parse_into(&chars, &SpanStyle::default(), &mut spans);
    merge(spans)
}

fn parse_into(chars: &[char], style: &SpanStyle, out: &mut Vec<Span>) {
    let mut text = String::new();
    let mut i = 0;
    let flush = |text: &mut String, out: &mut Vec<Span>| {
        if !text.is_empty() {
            out.push(Span {
                text: std::mem::take(text),
                style: style.clone(),
            });
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && chars.get(i + 1).is_some_and(|n| n.is_ascii_punctuation()) {
            text.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c == '`' {
            let run = run_length(chars, i, '`');
            if let Some(end) = find_backtick_close(chars, i + run, run) {
                flush(&mut text, out);
                let mut code: String = chars[i + run..end].iter().collect();
                if code.len() >= 2 && code.starts_with(' ') && code.ends_with(' ') {
                    code = code[1..code.len() - 1].to_string();
                }
                out.push(Span {
                    text: code.replace('\n', " "),
                    style: SpanStyle {
                        code: true,
                        ..style.clone()
                    },
                });
                i = end + run;
                continue;
            }
            text.extend(&chars[i..i + run]);
            i += run;
            continue;
        }
        let image = c == '!' && chars.get(i + 1) == Some(&'[');
        if (c == '[' || image) && style.link.is_none() {
            let open = if image { i + 1 } else { i };
            if let Some((label_end, url, next)) = parse_link(chars, open) {
                flush(&mut text, out);
                let link_style = SpanStyle {
                    link: Some(url),
                    ..style.clone()
                };
                let label = &chars[open + 1..label_end];
                if image {
                    out.push(Span {
                        text: label.iter().collect(),
                        style: link_style,
                    });
                } else {
                    parse_into(label, &link_style, out);
                }
                i = next;
                continue;
            }
        }
        if c == '<'
            && let Some(end) = chars[i + 1..].iter().position(|c| *c == '>')
        {
            let inner: String = chars[i + 1..i + 1 + end].iter().collect();
            if is_autolink(&inner) {
                flush(&mut text, out);
                let url = if inner.contains('@') && !inner.contains("://") {
                    format!("mailto:{inner}")
                } else {
                    inner.clone()
                };
                out.push(Span {
                    text: inner,
                    style: SpanStyle {
                        link: Some(url),
                        ..style.clone()
                    },
                });
                i += end + 2;
                continue;
            }
        }
        if let Some((delimiter, apply)) = delimiter_at(chars, i)
            && can_open(chars, i, delimiter)
            && let Some(close) = find_close(chars, i + delimiter.len(), delimiter)
        {
            flush(&mut text, out);
            let mut inner_style = style.clone();
            apply(&mut inner_style);
            parse_into(&chars[i + delimiter.len()..close], &inner_style, out);
            i = close + delimiter.len();
            continue;
        }
        text.push(c);
        i += 1;
    }
    flush(&mut text, out);
}

type Apply = fn(&mut SpanStyle);

fn delimiter_at(chars: &[char], i: usize) -> Option<(&'static str, Apply)> {
    let starts = |s: &str| {
        s.chars()
            .enumerate()
            .all(|(k, c)| chars.get(i + k) == Some(&c))
    };
    if starts("**") {
        Some(("**", |s| s.bold = true))
    } else if starts("__") {
        Some(("__", |s| s.bold = true))
    } else if starts("~~") {
        Some(("~~", |s| s.strike = true))
    } else if starts("*") {
        Some(("*", |s| s.italic = true))
    } else if starts("_") {
        Some(("_", |s| s.italic = true))
    } else {
        None
    }
}

/// Opening delimiters must be followed by non-space; `_` must not sit inside
/// a word (so `snake_case_name` stays plain).
fn can_open(chars: &[char], i: usize, delimiter: &str) -> bool {
    let after = chars.get(i + delimiter.len());
    if after.is_none_or(|c| c.is_whitespace()) {
        return false;
    }
    if delimiter.starts_with('_') && i > 0 && chars[i - 1].is_alphanumeric() {
        return false;
    }
    true
}

fn find_close(chars: &[char], from: usize, delimiter: &str) -> Option<usize> {
    let d: Vec<char> = delimiter.chars().collect();
    let mut j = from;
    while j + d.len() <= chars.len() {
        if chars[j] == '\\' {
            j += 2;
            continue;
        }
        if chars[j] == '`' {
            let run = run_length(chars, j, '`');
            j = find_backtick_close(chars, j + run, run).map_or(j + run, |end| end + run);
            continue;
        }
        if chars[j..j + d.len()] == d[..] {
            let single = d.len() == 1;
            let doubled = single && chars.get(j + 1) == Some(&d[0]);
            let space_before = chars[j - 1].is_whitespace();
            let word_after =
                d[0] == '_' && chars.get(j + d.len()).is_some_and(|c| c.is_alphanumeric());
            if doubled {
                // `**` inside `*…*`: skip the pair (its own span parses later).
                j += run_length(chars, j, d[0]);
                continue;
            }
            if j > from && !space_before && !word_after {
                return Some(j);
            }
        }
        j += 1;
    }
    None
}

fn run_length(chars: &[char], i: usize, c: char) -> usize {
    chars[i..].iter().take_while(|x| **x == c).count()
}

fn find_backtick_close(chars: &[char], from: usize, run: usize) -> Option<usize> {
    let mut j = from;
    while j < chars.len() {
        if chars[j] == '`' {
            let len = run_length(chars, j, '`');
            if len == run {
                return Some(j);
            }
            j += len;
        } else {
            j += 1;
        }
    }
    None
}

/// `[label](url "title")` starting at `open` (`[`). Returns the label's end
/// (`]` index), the url and the index after `)`.
fn parse_link(chars: &[char], open: usize) -> Option<(usize, String, usize)> {
    let mut depth = 0;
    let mut j = open;
    let label_end = loop {
        match chars.get(j)? {
            '\\' => j += 1,
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    break j;
                }
            }
            _ => {}
        }
        j += 1;
    };
    if chars.get(label_end + 1) != Some(&'(') {
        return None;
    }
    let close = chars[label_end + 2..].iter().position(|c| *c == ')')? + label_end + 2;
    let target: String = chars[label_end + 2..close].iter().collect();
    let url = target
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_string();
    Some((label_end, url, close + 1))
}

fn is_autolink(inner: &str) -> bool {
    if inner.is_empty() || inner.contains(char::is_whitespace) {
        return false;
    }
    let scheme = inner.split_once(':').map(|(s, _)| s);
    let has_scheme = scheme.is_some_and(|s| {
        s.len() >= 2
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
    });
    let email = inner.contains('@') && inner.contains('.') && !inner.starts_with('@');
    has_scheme || email
}

fn merge(spans: Vec<Span>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for span in spans {
        match out.last_mut() {
            Some(last) if last.style == span.style => last.text.push_str(&span.text),
            _ => out.push(span),
        }
    }
    out
}
