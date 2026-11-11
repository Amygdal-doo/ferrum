//! The completion menu under the caret and the hover popup (problems and
//! the server's type / docs) above the word, drawn over the editor.

use ferrumeditor_core::lsp::Severity;
use gpui::{
    Anchor, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Point, SharedString, StatefulInteractiveElement, Styled, Window,
    anchored, deferred, div, point, prelude::FluentBuilder as _, px,
};

use super::EditorView;
use super::completion::MENU_ROWS;
use crate::settings::Settings;
use crate::theme_apply::ui;

const MENU_WIDTH: Pixels = px(480.);
const HOVER_MAX_WIDTH: Pixels = px(560.);
const HOVER_MAX_HEIGHT: Pixels = px(320.);
/// Longest hover text shown, in lines.
const HOVER_LINES: usize = 40;

pub(super) fn severity_token(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Information => "git_modified",
        Severity::Hint => "text_muted",
    }
}

impl EditorView {
    pub(super) fn render_popups(&self, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = Vec::new();
        let Some(layout) = self.last_layout.as_ref() else {
            return out;
        };
        let settings = cx.global::<Settings>();
        let theme = settings.theme.clone();
        let font = SharedString::from(settings.config.font_family.clone());
        let font_size = px(settings.font_size());
        let line_height = layout.line_height;
        let viewport = window.viewport_size();
        let surface = ui(&theme, "surface");
        let border = ui(&theme, "border");
        let text = ui(&theme, "text");
        let muted = ui(&theme, "text_muted");

        if let Some(menu) = &self.completion
            && let Some(origin) = layout.point_for_offset(menu.anchor)
        {
            let row_height = (font_size * 1.6).round();
            let visible = menu.matches.len().min(MENU_ROWS);
            let doc = menu.selected_entry().and_then(|e| {
                let mut parts = Vec::new();
                if let Some(detail) = e.detail.as_deref().filter(|d| !d.is_empty()) {
                    parts.push(detail.to_string());
                }
                if let Some(doc) = e.documentation.as_deref() {
                    parts.push(doc.lines().take(8).collect::<Vec<_>>().join("\n"));
                }
                (!parts.is_empty()).then(|| parts.join("\n"))
            });
            let height =
                row_height * visible as f32 + if doc.is_some() { px(120.) } else { px(8.) };
            let below = origin.y + line_height + height < viewport.height;
            let x = origin.x - px(4.);
            let (anchor, position) = if below {
                (Anchor::TopLeft, point(x, origin.y + line_height))
            } else {
                (Anchor::BottomLeft, point(x, origin.y))
            };
            let selected_bg = ui(&theme, "element_selected");
            let accent = ui(&theme, "accent");
            let rows = menu.matches
                [menu.scroll_top..(menu.scroll_top + visible).min(menu.matches.len())]
                .iter()
                .enumerate()
                .map(|(i, &index)| {
                    let row = menu.scroll_top + i;
                    let entry = &menu.entries[index];
                    let selected = row == menu.selected;
                    div()
                        .id(("completion", row))
                        .h(row_height)
                        .px_2()
                        .flex()
                        .items_center()
                        .gap_2()
                        .when(selected, |d| d.bg(selected_bg))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.accept_completion_at(row, cx);
                            }),
                        )
                        .child(
                            div()
                                .w(px(52.))
                                .flex_none()
                                .text_size(font_size * 0.8)
                                .text_color(if selected { accent } else { muted })
                                .child(entry.kind),
                        )
                        .child(
                            div()
                                .flex_none()
                                .max_w(px(260.))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_color(text)
                                .child(entry.label.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_right()
                                .text_size(font_size * 0.85)
                                .text_color(muted)
                                .child(
                                    entry
                                        .detail
                                        .clone()
                                        .unwrap_or_default()
                                        .lines()
                                        .next()
                                        .unwrap_or_default()
                                        .to_string(),
                                ),
                        )
                });
            let body = div()
                .id("completion-menu")
                .occlude()
                .w(MENU_WIDTH)
                .bg(surface)
                .border_1()
                .border_color(border)
                .rounded_md()
                .shadow_md()
                .py_1()
                .font_family(font.clone())
                .text_size(font_size)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(rows)
                .when_some(doc, |d, doc| {
                    d.child(
                        div()
                            .mt_1()
                            .px_2()
                            .pt_1()
                            .border_t_1()
                            .border_color(border)
                            .max_h(px(110.))
                            .overflow_hidden()
                            .text_size(font_size * 0.85)
                            .text_color(muted)
                            .child(doc),
                    )
                });
            out.push(popup(anchor, position, body.into_any_element()));
        }

        if let Some(hover) = &self.lsp.hover
            && self.completion.is_none()
            && let Some(origin) = layout.point_for_offset(hover.anchor)
        {
            let above = origin.y - layout.bounds.top() > px(160.);
            let (anchor, position) = if above {
                (
                    Anchor::BottomLeft,
                    point(origin.x - px(4.), origin.y - px(2.)),
                )
            } else {
                (
                    Anchor::TopLeft,
                    point(origin.x - px(4.), origin.y + line_height + px(2.)),
                )
            };
            let problems = hover.diagnostics.iter().map(|(severity, message)| {
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui(&theme, severity_token(*severity)))
                            .child(severity.label()),
                    )
                    .child(div().text_color(text).child(message.clone()))
            });
            let docs = hover.text.as_ref().map(|t| {
                let mut lines: Vec<&str> = t.lines().take(HOVER_LINES).collect();
                if t.lines().count() > HOVER_LINES {
                    lines.push("…");
                }
                div()
                    .when(!hover.diagnostics.is_empty(), |d| {
                        d.mt_1().pt_1().border_t_1().border_color(border)
                    })
                    .font_family(font.clone())
                    .text_color(text)
                    .children(
                        lines
                            .into_iter()
                            .map(|l| div().min_h(font_size * 1.3).child(l.to_string())),
                    )
            });
            let body = div()
                .id("hover-popup")
                .occlude()
                .max_w(HOVER_MAX_WIDTH)
                .max_h(HOVER_MAX_HEIGHT)
                .overflow_y_scroll()
                .bg(surface)
                .border_1()
                .border_color(border)
                .rounded_md()
                .shadow_md()
                .px_2()
                .py_1()
                .text_size(font_size * 0.92)
                .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                    this.lsp.pointer_in_hover = *hovered;
                    if !hovered {
                        this.hover_left(cx);
                    }
                }))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(problems)
                .children(docs);
            out.push(popup(anchor, position, body.into_any_element()));
        }
        out
    }
}

fn popup(anchor: Anchor, position: Point<Pixels>, body: AnyElement) -> AnyElement {
    deferred(
        anchored()
            .anchor(anchor)
            .position(position)
            .snap_to_window_with_margin(px(8.))
            .child(body),
    )
    .with_priority(1)
    .into_any_element()
}
