//! XML display helpers: pretty-printing and syntax tokens for the response
//! view (SOAP, OData `$metadata`, any XML body). Lenient, never panics.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmlToken {
    /// `<name`, `</name`, `>`, `/>` and the name itself.
    Tag,
    AttrName,
    AttrValue,
    /// Comments, `<?…?>` declarations and `<!DOCTYPE …>`.
    Comment,
    /// `<![CDATA[…]]>`.
    Cdata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node<'a> {
    Open(&'a str),
    Close(&'a str),
    Empty(&'a str),
    Text(&'a str),
    Other(&'a str),
}

/// End index (exclusive) of the markup starting at `start` (`text[start]`
/// is `<`): comments, CDATA and quoted attribute values are skipped whole.
fn markup_end(text: &str, start: usize) -> usize {
    let rest = &text[start..];
    let close = |pat: &str| rest.find(pat).map_or(text.len(), |i| start + i + pat.len());
    if rest.starts_with("<!--") {
        return close("-->");
    }
    if rest.starts_with("<![CDATA[") {
        return close("]]>");
    }
    if rest.starts_with("<?") {
        return close("?>");
    }
    let mut quote: Option<char> = None;
    for (i, c) in rest.char_indices().skip(1) {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return start + i + 1,
            (None, _) => {}
        }
    }
    text.len()
}

fn nodes(text: &str) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        if text[i..].starts_with('<') {
            let end = markup_end(text, i);
            let m = &text[i..end];
            out.push(if m.starts_with("</") {
                Node::Close(m)
            } else if m.starts_with("<!") || m.starts_with("<?") {
                Node::Other(m)
            } else if m.ends_with("/>") {
                Node::Empty(m)
            } else {
                Node::Open(m)
            });
            i = end;
        } else {
            let end = text[i..].find('<').map_or(text.len(), |j| i + j);
            out.push(Node::Text(&text[i..end]));
            i = end;
        }
    }
    out
}

/// Indents XML two spaces per level. Elements holding only text stay on
/// one line. `None` when `text` does not look like XML.
#[must_use]
pub fn pretty_xml(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if !trimmed.starts_with('<') || !trimmed.ends_with('>') {
        return None;
    }
    let nodes: Vec<Node<'_>> = nodes(trimmed)
        .into_iter()
        .filter(|n| !matches!(n, Node::Text(t) if t.trim().is_empty()))
        .collect();
    let mut out = String::with_capacity(trimmed.len() + trimmed.len() / 4);
    let mut depth = 0usize;
    let mut i = 0;
    let pad = |depth: usize| "  ".repeat(depth);
    while i < nodes.len() {
        let line = match (&nodes[i], nodes.get(i + 1), nodes.get(i + 2)) {
            (Node::Open(o), Some(Node::Text(t)), Some(Node::Close(c))) => {
                i += 3;
                format!("{}{o}{}{c}", pad(depth), t.trim())
            }
            (Node::Open(o), Some(Node::Close(c)), _) => {
                i += 2;
                format!("{}{o}{c}", pad(depth))
            }
            (Node::Open(o), _, _) => {
                i += 1;
                depth += 1;
                format!("{}{o}", pad(depth - 1))
            }
            (Node::Close(c), _, _) => {
                i += 1;
                depth = depth.checked_sub(1)?;
                format!("{}{c}", pad(depth))
            }
            (Node::Empty(m) | Node::Other(m), _, _) => {
                i += 1;
                format!("{}{m}", pad(depth))
            }
            (Node::Text(t), _, _) => {
                i += 1;
                format!("{}{}", pad(depth), t.trim())
            }
        };
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&line);
    }
    Some(out)
}

/// Syntax tokens (byte ranges) of `text`, in order. Text content is not
/// tokenized.
#[must_use]
pub fn xml_tokens(text: &str) -> Vec<(Range<usize>, XmlToken)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = text[i..].find('<') {
        let start = i + off;
        let end = markup_end(text, start);
        let m = &text[start..end];
        if m.starts_with("<!--") || m.starts_with("<?") || m.starts_with("<!D") {
            out.push((start..end, XmlToken::Comment));
        } else if m.starts_with("<![CDATA[") {
            out.push((start..end, XmlToken::Cdata));
        } else {
            tag_tokens(text, start, end, &mut out);
        }
        i = end;
    }
    out
}

fn tag_tokens(text: &str, start: usize, end: usize, out: &mut Vec<(Range<usize>, XmlToken)>) {
    let bytes = text.as_bytes();
    let name_end = (start + 1..end)
        .find(|&j| {
            bytes[j].is_ascii_whitespace() || bytes[j] == b'>' || bytes[j] == b'/' && j > start + 1
        })
        .unwrap_or(end);
    out.push((start..name_end, XmlToken::Tag));
    let mut j = name_end;
    while j < end {
        let b = bytes[j];
        if b == b'"' || b == b'\'' {
            let close = (j + 1..end).find(|&k| bytes[k] == b).map_or(end, |k| k + 1);
            out.push((j..close, XmlToken::AttrValue));
            j = close;
        } else if b == b'>' || b == b'/' {
            out.push((j..end, XmlToken::Tag));
            break;
        } else if b.is_ascii_whitespace() || b == b'=' {
            j += 1;
        } else {
            let a_end = (j..end)
                .find(|&k| matches!(bytes[k], b'=' | b'>' | b'/') || bytes[k].is_ascii_whitespace())
                .unwrap_or(end);
            out.push((j..a_end, XmlToken::AttrName));
            j = a_end.max(j + 1);
        }
    }
}

/// Whether a body looks like XML (by content type or first character).
#[must_use]
pub fn looks_like_xml(content_type: Option<&str>, text: &str) -> bool {
    content_type.is_some_and(|c| c.contains("xml")) || text.trim_start().starts_with("<?xml")
}

#[cfg(test)]
#[path = "xml_tests.rs"]
mod tests;
