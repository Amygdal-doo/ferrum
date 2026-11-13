use crate::components::{Color, Label, LabelCommon, LabelSize, StyledTypography, h_flex, v_flex};
use crate::theme::ActiveTheme;
use gpui::{App, Div, Hsla, IntoElement, SharedString, div, prelude::*, px};

/// Top bar of a tool window: app name on the left, `children` after it.
pub fn toolbar(cx: &App) -> Div {
    let colors = cx.theme().colors();
    h_flex()
        .flex_none()
        .h(px(40.))
        .px_3()
        .gap_2()
        .bg(colors.title_bar_background)
        .border_b_1()
        .border_color(colors.border)
}

/// Bottom status line; add left/right groups as children.
pub fn status_bar(cx: &App) -> Div {
    let colors = cx.theme().colors();
    h_flex()
        .flex_none()
        .h(px(26.))
        .px_3()
        .gap_3()
        .justify_between()
        .bg(colors.status_bar_background)
        .border_t_1()
        .border_color(colors.border)
        .text_ui_sm(cx)
        .text_color(colors.text_muted)
}

/// Small uppercase heading used above sidebar/panel sections.
pub fn section_header(title: impl Into<SharedString>) -> impl IntoElement {
    h_flex().px_3().py_1().child(
        Label::new(title.into().to_uppercase())
            .size(LabelSize::XSmall)
            .color(Color::Muted),
    )
}

/// A compact pill, e.g. an HTTP method, a status code or a branch name.
pub fn badge(text: impl Into<SharedString>, color: Hsla, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    div()
        .flex_none()
        .px_1p5()
        .rounded_sm()
        .border_1()
        .border_color(colors.border_variant)
        .bg(color.opacity(0.12))
        .text_color(color)
        .text_ui_xs(cx)
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(text.into())
}

/// A keyboard shortcut hint, e.g. `kbd("⌘↵", cx)`.
pub fn kbd(keys: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    div()
        .flex_none()
        .px_1()
        .rounded_sm()
        .border_1()
        .border_color(colors.border)
        .bg(colors.element_background)
        .text_color(colors.text_muted)
        .text_ui_xs(cx)
        .font_buffer(cx)
        .child(keys.into())
}

/// Centered placeholder for empty panels.
pub fn empty_state(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
) -> impl IntoElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_1()
        .child(Label::new(title.into()).color(Color::Muted))
        .child(
            Label::new(detail.into())
                .size(LabelSize::Small)
                .color(Color::Placeholder),
        )
}
