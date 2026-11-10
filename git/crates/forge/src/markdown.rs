//! Basic Markdown → display blocks for PR descriptions and comments:
//! headings, paragraphs, lists (with task boxes), quotes, code blocks and
//! rules. Inline markup is simplified to plain text; links keep their URL.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading(u8, String),
    Paragraph(String),
    /// Indent level (0-based) and text, with `☐` / `☑` for task items.
    Bullet(usize, String),
    Quote(String),
    Code(String),
    Rule,
}

/// Parses `md` into blocks. HTML comments (PR templates) are dropped.
#[must_use]
pub fn blocks(md: &str) -> Vec<Block> {
    let text = strip_html_comments(&md.replace("\r\n", "\n"));
    let mut out = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let mut code: Option<Vec<String>> = None;
    let flush = |para: &mut Vec<String>, out: &mut Vec<Block>| {
        if !para.is_empty() {
            out.push(Block::Paragraph(inline(&para.join(" "))));
            para.clear();
        }
    };
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            match code.take() {
                Some(lines) => out.push(Block::Code(lines.join("\n"))),
                None => {
                    flush(&mut para, &mut out);
                    code = Some(Vec::new());
                }
            }
            continue;
        }
        if let Some(lines) = code.as_mut() {
            lines.push(line.to_owned());
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut para, &mut out);
            continue;
        }
        if let Some(h) = heading(trimmed) {
            flush(&mut para, &mut out);
            out.push(h);
        } else if is_rule(trimmed) {
            flush(&mut para, &mut out);
            out.push(Block::Rule);
        } else if let Some(item) = bullet(trimmed) {
            flush(&mut para, &mut out);
            let indent = (line.len() - trimmed.len()) / 2;
            out.push(Block::Bullet(indent, inline(&item)));
        } else if let Some(q) = trimmed.strip_prefix('>') {
            flush(&mut para, &mut out);
            let q = inline(q.trim());
            match out.last_mut() {
                Some(Block::Quote(prev)) => {
                    prev.push('\n');
                    prev.push_str(&q);
                }
                _ => out.push(Block::Quote(q)),
            }
        } else {
            para.push(trimmed.trim_end().to_owned());
        }
    }
    if let Some(lines) = code {
        out.push(Block::Code(lines.join("\n")));
    }
    flush(&mut para, &mut out);
    out
}

/// Single-line plain text (for list previews and notices).
#[must_use]
pub fn plain(md: &str) -> String {
    blocks(md)
        .into_iter()
        .map(|b| match b {
            Block::Heading(_, t)
            | Block::Paragraph(t)
            | Block::Bullet(_, t)
            | Block::Quote(t)
            | Block::Code(t) => t,
            Block::Rule => String::new(),
        })
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_html_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("<!--") {
        out.push_str(&rest[..i]);
        match rest[i..].find("-->") {
            Some(j) => rest = &rest[i + j + 3..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

fn heading(line: &str) -> Option<Block> {
    let level = line.bytes().take_while(|b| *b == b'#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &line[level..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    Some(Block::Heading(
        u8::try_from(level).unwrap_or(6),
        inline(rest.trim().trim_end_matches('#').trim()),
    ))
}

fn is_rule(line: &str) -> bool {
    let t: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    t.len() >= 3
        && (t.chars().all(|c| c == '-')
            || t.chars().all(|c| c == '*')
            || t.chars().all(|c| c == '_'))
}

fn bullet(line: &str) -> Option<String> {
    let rest = ["- ", "* ", "+ "]
        .iter()
        .find_map(|p| line.strip_prefix(p))
        .or_else(|| {
            let digits = line.bytes().take_while(u8::is_ascii_digit).count();
            (digits > 0)
                .then(|| &line[digits..])
                .and_then(|r| r.strip_prefix(". ").or_else(|| r.strip_prefix(") ")))
        })?;
    let rest = rest.trim();
    Some(if let Some(t) = rest.strip_prefix("[ ] ") {
        format!("☐ {t}")
    } else if let Some(t) = rest
        .strip_prefix("[x] ")
        .or_else(|| rest.strip_prefix("[X] "))
    {
        format!("☑ {t}")
    } else {
        rest.to_owned()
    })
}

/// Simplifies inline markup: emphasis and code markers removed, links as
/// `text (url)`, images as `[image: alt]`, simple HTML tags dropped.
#[must_use]
pub fn inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let image = c == '!' && chars.get(i + 1) == Some(&'[');
        if c == '[' || image {
            let start = if image { i + 2 } else { i + 1 };
            if let Some((text, url, end)) = link(&chars, start) {
                if image {
                    out.push_str(&format!("[image: {}]", inline(&text)));
                } else if text == url || url.is_empty() {
                    out.push_str(&inline(&text));
                } else {
                    out.push_str(&format!("{} ({url})", inline(&text)));
                }
                i = end;
                continue;
            }
        }
        if c == '<'
            && let Some(len) = html_tag(&chars[i..])
        {
            i += len;
            continue;
        }
        match c {
            '*' | '`' => {}
            '_' if is_emphasis_underscore(&chars, i) => {}
            '~' if chars.get(i + 1) == Some(&'~') => i += 1,
            '\\' if chars.get(i + 1).is_some_and(|n| n.is_ascii_punctuation()) => {
                i += 1;
                out.push(chars[i]);
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

/// `text](url)` starting after `[`; returns text, url and the index after `)`.
fn link(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    let close = start + chars[start..].iter().position(|c| *c == ']')?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let end = close + 2 + chars[close + 2..].iter().position(|c| *c == ')')?;
    let text: String = chars[start..close].iter().collect();
    let url: String = chars[close + 2..end].iter().collect();
    let url = url.split_whitespace().next().unwrap_or_default().to_owned();
    Some((text, url, end + 1))
}

/// Length of `<tag>`, `</tag>` or `<br/>` at the start, for simple tags.
fn html_tag(chars: &[char]) -> Option<usize> {
    let end = chars.iter().position(|c| *c == '>')?;
    let inner: String = chars[1..end].iter().collect();
    let name = inner.trim_start_matches('/');
    let ok = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && !inner.contains('<')
        && end < 200;
    ok.then_some(end + 1)
}

/// `_` delimits emphasis only at word boundaries (`snake_case` keeps it).
fn is_emphasis_underscore(chars: &[char], i: usize) -> bool {
    let before = i.checked_sub(1).and_then(|j| chars.get(j));
    let after = chars.get(i + 1);
    let word = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric());
    !(word(before) && word(after))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_common_pr_markdown() {
        let md = "## Summary\r\n<!-- template hint\nspanning lines -->\nFixes **the** `bug` in\nsnake_case code.\n\n- [x] tests\n- [ ] docs\n  - nested\n1. first\n\n> quoted\n> more\n\n```rust\nlet a = 1;\n```\n---\nSee [the issue](https://x/1) and ![shot](https://x/a.png).<br>";
        assert_eq!(
            blocks(md),
            vec![
                Block::Heading(2, "Summary".into()),
                Block::Paragraph("Fixes the bug in snake_case code.".into()),
                Block::Bullet(0, "☑ tests".into()),
                Block::Bullet(0, "☐ docs".into()),
                Block::Bullet(1, "nested".into()),
                Block::Bullet(0, "first".into()),
                Block::Quote("quoted\nmore".into()),
                Block::Code("let a = 1;".into()),
                Block::Rule,
                Block::Paragraph("See the issue (https://x/1) and [image: shot].".into()),
            ]
        );
    }

    #[test]
    fn inline_edge_cases() {
        assert_eq!(inline("a [b] c"), "a [b] c");
        assert_eq!(inline("x < y and a<b"), "x < y and a<b");
        assert_eq!(inline("_em_ and __strong__"), "em and strong");
        assert_eq!(inline(r"\*literal\*"), "*literal*");
        assert_eq!(inline("~~old~~ new"), "old new");
        assert_eq!(inline("[https://a](https://a)"), "https://a");
        assert_eq!(plain("# T\n\nbody"), "T body");
        assert_eq!(
            blocks("```\nunterminated"),
            vec![Block::Code("unterminated".into())]
        );
        assert_eq!(blocks(""), Vec::new());
    }
}
