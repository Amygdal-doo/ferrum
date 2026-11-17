//! A small markdown renderer, shared by FerrumChat's streamed assistant
//! text and FerrumLauncher's release notes: headings, paragraphs, bullet
//! and numbered lists, block quotes, rules, fenced code blocks (with a copy
//! button), and inline `code`, **bold**, *italic* and [links](…).
//! Incomplete markup while streaming renders as plain text.

use std::ops::Range;

use crate::prelude::*;
use crate::theme::theme_settings;
use gpui::{ClipboardItem, FontStyle, FontWeight, HighlightStyle, StyledText};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdBlock {
    Heading(usize, String),
    Paragraph(String),
    Item {
        marker: String,
        text: String,
        depth: usize,
    },
    Quote(String),
    Code {
        lang: String,
        code: String,
    },
    Rule,
}

pub fn parse(text: &str) -> Vec<MdBlock> {
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let mut code: Option<(String, String)> = None;
    let flush = |paragraph: &mut String, blocks: &mut Vec<MdBlock>| {
        if !paragraph.trim().is_empty() {
            blocks.push(MdBlock::Paragraph(paragraph.trim().to_string()));
        }
        paragraph.clear();
    };
    for line in text.lines() {
        if let Some((lang, body)) = code.as_mut() {
            if line.trim_start().starts_with("```") {
                blocks.push(MdBlock::Code {
                    lang: std::mem::take(lang),
                    code: body.trim_end_matches('\n').to_string(),
                });
                code = None;
            } else {
                body.push_str(line);
                body.push('\n');
            }
            continue;
        }
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if let Some(lang) = trimmed.strip_prefix("```") {
            flush(&mut paragraph, &mut blocks);
            code = Some((lang.trim().to_string(), String::new()));
        } else if trimmed.is_empty() {
            flush(&mut paragraph, &mut blocks);
        } else if let Some(level) = heading_level(trimmed) {
            flush(&mut paragraph, &mut blocks);
            blocks.push(MdBlock::Heading(level, trimmed[level..].trim().to_string()));
        } else if matches!(trimmed, "---" | "***" | "___") {
            flush(&mut paragraph, &mut blocks);
            blocks.push(MdBlock::Rule);
        } else if let Some(rest) = trimmed.strip_prefix("> ").or(trimmed.strip_prefix('>')) {
            flush(&mut paragraph, &mut blocks);
            blocks.push(MdBlock::Quote(rest.to_string()));
        } else if let Some((marker, rest)) = list_marker(trimmed) {
            flush(&mut paragraph, &mut blocks);
            blocks.push(MdBlock::Item {
                marker,
                text: rest.to_string(),
                depth: indent / 2,
            });
        } else {
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(trimmed);
        }
    }
    flush(&mut paragraph, &mut blocks);
    if let Some((lang, body)) = code {
        // Still streaming: show what we have.
        blocks.push(MdBlock::Code {
            lang,
            code: body.trim_end_matches('\n').to_string(),
        });
    }
    blocks
}

fn heading_level(line: &str) -> Option<usize> {
    let level = line.chars().take_while(|c| *c == '#').count();
    ((1..=6).contains(&level) && line[level..].starts_with(' ')).then_some(level)
}

fn list_marker(line: &str) -> Option<(String, &str)> {
    for bullet in ["- [ ] ", "- [x] ", "- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            let marker = match bullet {
                "- [ ] " => "☐",
                "- [x] " => "☑",
                _ => "•",
            };
            return Some((marker.into(), rest));
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        return Some((line[..digits + 1].to_string(), &line[digits + 2..]));
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inline {
    Code,
    Bold,
    Italic,
    Link,
}

/// Plain text with styled ranges (markers removed).
pub fn inline(text: &str) -> (String, Vec<(Range<usize>, Inline)>) {
    let mut out = String::new();
    let mut spans = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('`')
            && let Some(end) = after.find('`')
        {
            let start = out.len();
            out.push_str(&after[..end]);
            spans.push((start..out.len(), Inline::Code));
            rest = &after[end + 1..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("**")
            && let Some(end) = after.find("**")
            && end > 0
        {
            let start = out.len();
            out.push_str(&after[..end]);
            spans.push((start..out.len(), Inline::Bold));
            rest = &after[end + 2..];
            continue;
        }
        if let Some(after) = rest.strip_prefix('*')
            && !after.starts_with(' ')
            && let Some(end) = after.find('*')
            && end > 0
        {
            let start = out.len();
            out.push_str(&after[..end]);
            spans.push((start..out.len(), Inline::Italic));
            rest = &after[end + 1..];
            continue;
        }
        if let Some(after) = rest.strip_prefix('[')
            && let Some(close) = after.find("](")
            && let Some(end) = after[close + 2..].find(')')
        {
            let start = out.len();
            out.push_str(&after[..close]);
            spans.push((start..out.len(), Inline::Link));
            rest = &after[close + 2 + end + 1..];
            continue;
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            out.push(c);
        }
        rest = chars.as_str();
    }
    (out, spans)
}

fn styled(text: &str, cx: &App) -> StyledText {
    let (plain, spans) = inline(text);
    let colors = cx.theme().colors();
    let code_bg = colors.element_background;
    let accent = colors.text_accent;
    let highlights: Vec<(Range<usize>, HighlightStyle)> = spans
        .into_iter()
        .map(|(range, kind)| {
            let style = match kind {
                Inline::Code => HighlightStyle {
                    background_color: Some(code_bg),
                    color: Some(accent),
                    ..Default::default()
                },
                Inline::Bold => HighlightStyle {
                    font_weight: Some(FontWeight::BOLD),
                    ..Default::default()
                },
                Inline::Italic => HighlightStyle {
                    font_style: Some(FontStyle::Italic),
                    ..Default::default()
                },
                Inline::Link => HighlightStyle {
                    color: Some(accent),
                    ..Default::default()
                },
            };
            (range, style)
        })
        .collect();
    StyledText::new(plain).with_highlights(highlights)
}

/// Renders `text` as markdown; `id` keeps element ids unique.
pub fn render(id: impl Into<SharedString>, text: &str, cx: &App) -> AnyElement {
    let id: SharedString = id.into();
    let colors = cx.theme().colors();
    let mono = theme_settings(cx).buffer_font.family.clone();
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1p5()
        .text_sm()
        .children(parse(text).into_iter().enumerate().map(|(i, block)| {
            match block {
                MdBlock::Heading(level, text) => div()
                    .pt_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .when(level <= 2, |d| d.text_base())
                    .child(styled(&text, cx))
                    .into_any_element(),
                MdBlock::Paragraph(text) => div().child(styled(&text, cx)).into_any_element(),
                MdBlock::Item {
                    marker,
                    text,
                    depth,
                } => h_flex()
                    .items_start()
                    .gap_1p5()
                    .pl(px(depth as f32 * 14.))
                    .child(
                        div()
                            .flex_none()
                            .text_color(colors.text_muted)
                            .child(marker),
                    )
                    .child(div().flex_1().min_w_0().child(styled(&text, cx)))
                    .into_any_element(),
                MdBlock::Quote(text) => div()
                    .pl_2()
                    .border_l_2()
                    .border_color(colors.border)
                    .text_color(colors.text_muted)
                    .child(styled(&text, cx))
                    .into_any_element(),
                MdBlock::Rule => div().h(px(1.)).bg(colors.border_variant).into_any_element(),
                MdBlock::Code { lang, code } => {
                    let copy = code.clone();
                    v_flex()
                        .w_full()
                        .rounded_md()
                        .border_1()
                        .border_color(colors.border_variant)
                        .bg(colors.editor_background)
                        .child(
                            h_flex()
                                .px_2()
                                .py_0p5()
                                .justify_between()
                                .border_b_1()
                                .border_color(colors.border_variant)
                                .child(
                                    Label::new(if lang.is_empty() {
                                        "code".to_string()
                                    } else {
                                        lang
                                    })
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                                )
                                .child(
                                    IconButton::new(
                                        SharedString::from(format!("{id}-copy-{i}")),
                                        IconName::Copy,
                                    )
                                    .icon_size(IconSize::XSmall)
                                    .tooltip(Tooltip::text("Copy"))
                                    .on_click(
                                        move |_, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                copy.clone(),
                                            ))
                                        },
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("{id}-code-{i}")))
                                .overflow_x_scroll()
                                .px_2()
                                .py_1p5()
                                .font_family(mono.clone())
                                .text_xs()
                                .child(code),
                        )
                        .into_any_element()
                }
            }
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blocks_and_inline_spans() {
        let blocks = parse(
            "# Title\nSome *text* here\ncontinued.\n\n- one\n  - nested\n1. first\n> quote\n---\n```rust\nfn x() {}\n```\n```py\nopen",
        );
        assert_eq!(
            blocks,
            [
                MdBlock::Heading(1, "Title".into()),
                MdBlock::Paragraph("Some *text* here continued.".into()),
                MdBlock::Item {
                    marker: "•".into(),
                    text: "one".into(),
                    depth: 0
                },
                MdBlock::Item {
                    marker: "•".into(),
                    text: "nested".into(),
                    depth: 1
                },
                MdBlock::Item {
                    marker: "1.".into(),
                    text: "first".into(),
                    depth: 0
                },
                MdBlock::Quote("quote".into()),
                MdBlock::Rule,
                MdBlock::Code {
                    lang: "rust".into(),
                    code: "fn x() {}".into()
                },
                MdBlock::Code {
                    lang: "py".into(),
                    code: "open".into()
                },
            ]
        );
        let (plain, spans) = inline("use `cargo` **now**, *maybe* see [docs](https://x.y) * not");
        assert_eq!(plain, "use cargo now, maybe see docs * not");
        let kinds: Vec<Inline> = spans.iter().map(|s| s.1).collect();
        assert_eq!(
            kinds,
            [Inline::Code, Inline::Bold, Inline::Italic, Inline::Link]
        );
        assert_eq!(&plain[spans[0].0.clone()], "cargo");
        let (plain, spans) = inline("unclosed `tick and **bold");
        assert_eq!(plain, "unclosed `tick and **bold");
        assert!(spans.is_empty());
    }
}
