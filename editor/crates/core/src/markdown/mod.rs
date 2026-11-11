//! A small Markdown parser for the preview pane: CommonMark-ish blocks and
//! GFM tables / task lists / strikethrough, flattened into a simple model a
//! renderer can walk without recursion into inline trees.

mod inline;
#[cfg(test)]
mod tests;

pub use inline::parse_inline;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpanStyle {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// May contain `\n` for hard line breaks.
    pub text: String,
    pub style: SpanStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMarker {
    Bullet,
    Number(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8,
        spans: Vec<Span>,
    },
    Paragraph(Vec<Span>),
    Code {
        lang: Option<String>,
        text: String,
    },
    Quote(Vec<Block>),
    /// One list item; nesting is expressed by `depth` (0 = top level).
    ListItem {
        depth: usize,
        marker: ListMarker,
        checked: Option<bool>,
        spans: Vec<Span>,
    },
    Rule,
    Table {
        header: Vec<Vec<Span>>,
        rows: Vec<Vec<Vec<Span>>>,
    },
}

pub fn parse(src: &str) -> Vec<Block> {
    let lines: Vec<String> = src
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).replace('\t', "    "))
        .collect();
    let mut blocks = Vec::new();
    let mut list_indents: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        let indent = indent_of(line);
        let trimmed = line.trim_start();

        if let Some((fence, info)) = fence_start(line) {
            list_indents.clear();
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !is_fence_close(&lines[i], &fence) {
                code.push(strip_indent(&lines[i], indent));
                i += 1;
            }
            i += 1;
            let lang = info.split_whitespace().next().map(str::to_string);
            blocks.push(Block::Code {
                lang,
                text: code.join("\n"),
            });
            continue;
        }
        if indent >= 4 && list_indents.is_empty() {
            let mut code = Vec::new();
            while i < lines.len() && (indent_of(&lines[i]) >= 4 || lines[i].trim().is_empty()) {
                code.push(strip_indent(&lines[i], 4));
                i += 1;
            }
            while code.last().is_some_and(|l| l.trim().is_empty()) {
                code.pop();
            }
            blocks.push(Block::Code {
                lang: None,
                text: code.join("\n"),
            });
            continue;
        }
        if let Some(block) = atx_heading(trimmed) {
            list_indents.clear();
            blocks.push(block);
            i += 1;
            continue;
        }
        if is_rule(trimmed) {
            list_indents.clear();
            blocks.push(Block::Rule);
            i += 1;
            continue;
        }
        if trimmed.starts_with('>') {
            list_indents.clear();
            let mut inner = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let rest = &lines[i].trim_start()[1..];
                inner.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
                i += 1;
            }
            blocks.push(Block::Quote(parse(&inner.join("\n"))));
            continue;
        }
        if let Some((marker, content)) = list_marker(trimmed) {
            while list_indents.last().is_some_and(|top| *top > indent) {
                list_indents.pop();
            }
            if list_indents.last() != Some(&indent) {
                list_indents.push(indent);
            }
            let depth = list_indents.len() - 1;
            let mut text = content.to_string();
            i += 1;
            while i < lines.len() && is_continuation(&lines[i]) {
                text.push(' ');
                text.push_str(lines[i].trim());
                i += 1;
            }
            let (checked, text) = task_state(&text);
            blocks.push(Block::ListItem {
                depth,
                marker,
                checked,
                spans: parse_inline(text),
            });
            continue;
        }
        list_indents.clear();
        if let Some((table, next)) = table_at(&lines, i) {
            blocks.push(table);
            i = next;
            continue;
        }
        // Paragraph (possibly a setext heading).
        let mut text = String::new();
        while i < lines.len() {
            let current = &lines[i];
            if current.trim().is_empty() {
                break;
            }
            if !text.is_empty() {
                if let Some(level) = setext_level(current) {
                    blocks.push(Block::Heading {
                        level,
                        spans: parse_inline(text.trim_end()),
                    });
                    text.clear();
                    i += 1;
                    break;
                }
                if interrupts_paragraph(current) {
                    break;
                }
                let hard = text.ends_with("  ") || text.ends_with('\\');
                let trimmed_len = text.trim_end_matches([' ', '\\']).len();
                text.truncate(trimmed_len);
                text.push(if hard { '\n' } else { ' ' });
            }
            text.push_str(current.trim_start());
            i += 1;
        }
        if !text.is_empty() {
            blocks.push(Block::Paragraph(parse_inline(text.trim_end())));
        }
    }
    blocks
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn strip_indent(line: &str, n: usize) -> String {
    let strip = indent_of(line).min(n);
    line[strip..].to_string()
}

fn fence_start(line: &str) -> Option<(String, String)> {
    if indent_of(line) > 3 {
        return None;
    }
    let trimmed = line.trim_start();
    let c = trimmed.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let len = trimmed.chars().take_while(|x| *x == c).count();
    if len < 3 {
        return None;
    }
    let info = trimmed[len..].trim();
    if c == '`' && info.contains('`') {
        return None;
    }
    Some((trimmed[..len].to_string(), info.to_string()))
}

fn is_fence_close(line: &str, fence: &str) -> bool {
    let trimmed = line.trim();
    let c = fence.chars().next().unwrap_or('`');
    trimmed.len() >= fence.len() && trimmed.chars().all(|x| x == c)
}

fn atx_heading(trimmed: &str) -> Option<Block> {
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &trimmed[level..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    let mut text = rest.trim();
    let without_closing = text.trim_end_matches('#');
    if without_closing.is_empty() || without_closing.ends_with(' ') {
        text = without_closing.trim_end();
    }
    Some(Block::Heading {
        level: level as u8,
        spans: parse_inline(text),
    })
}

fn is_rule(trimmed: &str) -> bool {
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    let Some(first) = compact.chars().next() else {
        return false;
    };
    matches!(first, '-' | '*' | '_') && compact.len() >= 3 && compact.chars().all(|c| c == first)
}

fn setext_level(line: &str) -> Option<u8> {
    let trimmed = line.trim();
    if indent_of(line) > 3 || trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().all(|c| c == '=') {
        Some(1)
    } else if trimmed.chars().all(|c| c == '-') {
        Some(2)
    } else {
        None
    }
}

/// `- x`, `* x`, `+ x`, `1. x`, `1) x` → marker and content.
fn list_marker(trimmed: &str) -> Option<(ListMarker, &str)> {
    let mut chars = trimmed.chars();
    let first = chars.next()?;
    if matches!(first, '-' | '*' | '+') {
        let rest = &trimmed[1..];
        return (rest.is_empty() || rest.starts_with(' '))
            .then(|| (ListMarker::Bullet, rest.trim_start()));
    }
    let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 9 {
        return None;
    }
    let after = &trimmed[digits..];
    let rest = after
        .strip_prefix('.')
        .or_else(|| after.strip_prefix(')'))?;
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    let number = trimmed[..digits].parse().ok()?;
    Some((ListMarker::Number(number), rest.trim_start()))
}

fn task_state(text: &str) -> (Option<bool>, &str) {
    for (prefix, state) in [("[ ]", false), ("[x]", true), ("[X]", true)] {
        if let Some(rest) = text.strip_prefix(prefix)
            && (rest.is_empty() || rest.starts_with(' '))
        {
            return (Some(state), rest.trim_start());
        }
    }
    (None, text)
}

fn is_continuation(line: &str) -> bool {
    !line.trim().is_empty()
        && !interrupts_paragraph(line)
        && list_marker(line.trim_start()).is_none()
}

fn interrupts_paragraph(line: &str) -> bool {
    let trimmed = line.trim_start();
    fence_start(line).is_some()
        || atx_heading(trimmed).is_some()
        || is_rule(trimmed)
        || trimmed.starts_with('>')
        || matches!(list_marker(trimmed), Some((ListMarker::Bullet, c)) if !c.is_empty())
        || matches!(list_marker(trimmed), Some((ListMarker::Number(1), c)) if !c.is_empty())
}

fn table_at(lines: &[String], i: usize) -> Option<(Block, usize)> {
    let header_line = &lines[i];
    let delimiter = lines.get(i + 1)?;
    if !header_line.contains('|') || !is_table_delimiter(delimiter) {
        return None;
    }
    let header = split_row(header_line);
    let mut rows = Vec::new();
    let mut next = i + 2;
    while next < lines.len() && lines[next].contains('|') && !lines[next].trim().is_empty() {
        let mut row = split_row(&lines[next]);
        row.resize_with(header.len(), Vec::new);
        rows.push(row);
        next += 1;
    }
    Some((Block::Table { header, rows }, next))
}

fn is_table_delimiter(line: &str) -> bool {
    let cells = row_cells(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let cell = cell.trim();
            let core = cell.trim_start_matches(':').trim_end_matches(':');
            !core.is_empty() && core.chars().all(|c| c == '-')
        })
}

fn row_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix('|').unwrap_or(trimmed);
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut chars = trimmed.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'|') {
            current.push('|');
            chars.next();
        } else if c == '|' {
            cells.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    cells.push(current);
    cells
}

fn split_row(line: &str) -> Vec<Vec<Span>> {
    row_cells(line)
        .iter()
        .map(|cell| parse_inline(cell.trim()))
        .collect()
}
