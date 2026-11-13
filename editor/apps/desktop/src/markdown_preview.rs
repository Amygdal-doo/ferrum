//! Markdown preview pane item: renders the blocks parsed by
//! `ferrumeditor_core::markdown` from an editor's text, re-parsing (debounced)
//! as the editor changes.

use std::ops::Range;
use std::time::Duration;

use ferrum_ui::components::StyledTypography;
use ferrum_ui::prelude::*;
use ferrumeditor_core::markdown::{self, Block, ListMarker, Span};
use gpui::{
    FontStyle, FontWeight, HighlightStyle, Hsla, InteractiveText, StrikethroughStyle, StyledText,
    UnderlineStyle,
};

use crate::editor::{EditorEvent, EditorView};

const REPARSE_DELAY: Duration = Duration::from_millis(150);

pub struct MarkdownPreview {
    editor: Entity<EditorView>,
    blocks: Vec<Block>,
    focus_handle: FocusHandle,
    reparse: Option<Task<()>>,
    _subscription: Subscription,
}

impl Focusable for MarkdownPreview {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl MarkdownPreview {
    pub fn new(editor: Entity<EditorView>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subscription = cx.subscribe(&editor, |this: &mut Self, _, event: &EditorEvent, cx| {
            if *event == EditorEvent::Edited {
                this.schedule_reparse(cx);
            }
        });
        let blocks = markdown::parse(&editor.read(cx).state.text());
        Self {
            editor,
            blocks,
            focus_handle: cx.focus_handle(),
            reparse: None,
            _subscription: subscription,
        }
    }

    pub fn title(&self, cx: &App) -> SharedString {
        format!("Preview {}", self.editor.read(cx).title()).into()
    }

    fn schedule_reparse(&mut self, cx: &mut Context<Self>) {
        self.reparse = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REPARSE_DELAY).await;
            this.update(cx, |this, cx| {
                this.blocks = markdown::parse(&this.editor.read(cx).state.text());
                cx.notify();
            })
            .ok();
        }));
    }
}

/// Colours used while rendering one frame.
#[derive(Clone, Copy)]
struct Palette {
    text: Hsla,
    muted: Hsla,
    accent: Hsla,
    border: Hsla,
    code_bg: Hsla,
}

/// One wrapped paragraph of styled spans; links are clickable.
fn rich_text(id: impl Into<ElementId>, spans: &[Span], palette: Palette) -> AnyElement {
    let mut text = String::new();
    let mut highlights: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
    let mut links: Vec<(Range<usize>, String)> = Vec::new();
    for span in spans {
        let start = text.len();
        text.push_str(&span.text);
        let range = start..text.len();
        let style = &span.style;
        let mut highlight = HighlightStyle::default();
        if style.bold {
            highlight.font_weight = Some(FontWeight::BOLD);
        }
        if style.italic {
            highlight.font_style = Some(FontStyle::Italic);
        }
        if style.code {
            highlight.background_color = Some(palette.code_bg);
        }
        if style.strike {
            highlight.strikethrough = Some(StrikethroughStyle {
                thickness: px(1.),
                color: Some(palette.text),
            });
        }
        if let Some(url) = &style.link {
            highlight.color = Some(palette.accent);
            highlight.underline = Some(UnderlineStyle {
                color: Some(palette.accent),
                thickness: px(1.),
                wavy: false,
            });
            links.push((range.clone(), url.clone()));
        }
        if highlight != HighlightStyle::default() && !range.is_empty() {
            highlights.push((range, highlight));
        }
    }
    let styled = StyledText::new(text).with_highlights(highlights);
    if links.is_empty() {
        return div().child(styled).into_any_element();
    }
    let ranges: Vec<Range<usize>> = links.iter().map(|(r, _)| r.clone()).collect();
    let urls: Vec<String> = links.into_iter().map(|(_, url)| url).collect();
    InteractiveText::new(id.into(), styled)
        .on_click(ranges, move |index, _, cx| {
            if let Some(url) = urls.get(index)
                && ["http://", "https://", "mailto:"]
                    .iter()
                    .any(|p| url.starts_with(p))
            {
                cx.open_url(url);
            }
        })
        .into_any_element()
}

fn render_block(block: &Block, id: usize, palette: Palette, cx: &App) -> AnyElement {
    match block {
        Block::Heading { level, spans } => {
            let heading = div().font_weight(FontWeight::BOLD).text_color(palette.text);
            let heading = match level {
                1 => heading
                    .text_3xl()
                    .pb_1()
                    .border_b_1()
                    .border_color(palette.border),
                2 => heading
                    .text_2xl()
                    .pb_1()
                    .border_b_1()
                    .border_color(palette.border),
                3 => heading.text_xl(),
                4 => heading.text_lg(),
                _ => heading.text_base(),
            };
            heading
                .mt_2()
                .child(rich_text(("md-h", id), spans, palette))
                .into_any_element()
        }
        Block::Paragraph(spans) => div()
            .child(rich_text(("md-p", id), spans, palette))
            .into_any_element(),
        Block::Code { text, .. } => v_flex()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(palette.border)
            .bg(palette.code_bg)
            .font_buffer(cx)
            .text_buffer(cx)
            .children(text.split('\n').map(|line| {
                // Keep blank lines and leading spaces visible.
                let line = if line.is_empty() {
                    " ".to_string()
                } else {
                    line.replace('\t', "    ")
                };
                div().whitespace_nowrap().child(line)
            }))
            .into_any_element(),
        Block::Quote(blocks) => v_flex()
            .gap_2()
            .pl_3()
            .border_l_4()
            .border_color(palette.border)
            .text_color(palette.muted)
            .children(
                blocks
                    .iter()
                    .enumerate()
                    .map(|(i, b)| render_block(b, id * 1000 + i, palette, cx)),
            )
            .into_any_element(),
        Block::ListItem {
            depth,
            marker,
            checked,
            spans,
        } => {
            let bullet = match (checked, marker) {
                (Some(true), _) => "☑".to_string(),
                (Some(false), _) => "☐".to_string(),
                (None, ListMarker::Bullet) => "•".to_string(),
                (None, ListMarker::Number(n)) => format!("{n}."),
            };
            h_flex()
                .items_start()
                .gap_2()
                .pl(px(20. * *depth as f32 + 4.))
                .child(
                    div()
                        .flex_none()
                        .min_w(px(16.))
                        .text_color(palette.muted)
                        .child(bullet),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(rich_text(("md-li", id), spans, palette)),
                )
                .into_any_element()
        }
        Block::Rule => div().h(px(1.)).my_2().bg(palette.border).into_any_element(),
        Block::Table { header, rows } => {
            let cell = |spans: &[Span], key: usize, bold: bool| {
                div()
                    .flex_1()
                    .min_w_0()
                    .px_2()
                    .py_1()
                    .border_r_1()
                    .border_color(palette.border)
                    .when(bold, |d| d.font_weight(FontWeight::BOLD))
                    .child(rich_text(("md-td", key), spans, palette))
            };
            let row = |cells: &[Vec<Span>], key: usize, bold: bool| {
                h_flex().border_b_1().border_color(palette.border).children(
                    cells
                        .iter()
                        .enumerate()
                        .map(|(i, c)| cell(c, key * 100 + i, bold)),
                )
            };
            v_flex()
                .border_1()
                .border_color(palette.border)
                .child(row(header, id * 1000, true))
                .children(
                    rows.iter()
                        .enumerate()
                        .map(|(i, r)| row(r, id * 1000 + i + 1, false)),
                )
                .into_any_element()
        }
    }
}

impl Render for MarkdownPreview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let palette = Palette {
            text: colors.text,
            muted: colors.text_muted,
            accent: crate::theme_apply::ui(
                &cx.global::<crate::settings::Settings>().theme,
                "accent",
            ),
            border: colors.border,
            code_bg: colors.element_background,
        };
        let background = colors.editor_background;
        let blocks: Vec<AnyElement> = self
            .blocks
            .iter()
            .enumerate()
            .map(|(i, block)| render_block(block, i, palette, cx))
            .collect();
        div()
            .id("markdown-preview")
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_y_scroll()
            .bg(background)
            .text_color(palette.text)
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(760.))
                    .mx_auto()
                    .px_6()
                    .py_5()
                    .gap_3()
                    .when(blocks.is_empty(), |this| {
                        this.child(Label::new("Nothing to preview").color(Color::Muted))
                    })
                    .children(blocks),
            )
    }
}
