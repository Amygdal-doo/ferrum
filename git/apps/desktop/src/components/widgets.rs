//! Small app-local building blocks on top of Zed's `ui` crate.

use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, SharedString, Stateful, Window, div, prelude::*,
    px,
};
use theme::ActiveTheme;
use ui::{
    Button, ButtonCommon, ButtonSize, ButtonStyle, Color, Label, LabelCommon, LabelSize, TintColor,
    h_flex, v_flex,
};

/// A labelled form row: small muted caption above `child`.
pub fn field(label: impl Into<SharedString>, child: impl IntoElement) -> Div {
    v_flex()
        .gap_1()
        .child(
            Label::new(label.into())
                .size(LabelSize::Small)
                .color(Color::Muted),
        )
        .child(child)
}

/// Uppercase section caption (dialogs).
pub fn caption(text: &str) -> Label {
    Label::new(text.to_uppercase())
        .size(LabelSize::XSmall)
        .color(Color::Muted)
}

/// Muted helper text.
pub fn hint(text: impl Into<SharedString>) -> Label {
    Label::new(text.into())
        .size(LabelSize::Small)
        .color(Color::Muted)
}

/// Compact secondary button.
pub fn small_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id, label)
        .style(ButtonStyle::Outlined)
        .size(ButtonSize::Compact)
}

/// Compact subtle (ghost) button.
pub fn ghost_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id, label).size(ButtonSize::Compact)
}

/// Primary (filled) button.
pub fn primary_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id, label).style(ButtonStyle::Filled)
}

/// Destructive button.
pub fn danger_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id, label).style(ButtonStyle::Tinted(TintColor::Error))
}

/// Primary or destructive, depending on `destructive`.
pub fn confirm_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    destructive: bool,
) -> Button {
    if destructive {
        danger_button(id, label)
    } else {
        primary_button(id, label)
    }
}

/// A radio choice row with title and hint.
pub fn radio_row(
    id: impl Into<ElementId>,
    selected: bool,
    title: impl Into<SharedString>,
    hint_text: Option<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<Div> {
    let colors = cx.theme().colors();
    h_flex()
        .id(id)
        .gap_3()
        .px_3()
        .py_2()
        .cursor_pointer()
        .hover(|s| s.bg(colors.element_hover))
        .child(radio_dot(selected, cx))
        .child(
            v_flex()
                .min_w_0()
                .child(Label::new(title.into()))
                .children(hint_text.map(|h| hint(h).truncate())),
        )
        .on_click(on_click)
}

pub fn radio_dot(selected: bool, cx: &App) -> Div {
    let colors = cx.theme().colors();
    div()
        .flex_none()
        .size(px(14.))
        .rounded_full()
        .border_1()
        .border_color(if selected {
            colors.text_accent
        } else {
            colors.border
        })
        .flex()
        .items_center()
        .justify_center()
        .when(selected, |d| {
            d.child(div().size(px(6.)).rounded_full().bg(colors.text_accent))
        })
}

/// Bordered card container used by dialogs.
pub fn card(cx: &App) -> Div {
    let colors = cx.theme().colors();
    v_flex()
        .gap_2()
        .p_3()
        .rounded_md()
        .border_1()
        .border_color(colors.border_variant)
}

/// Empty-panel text.
pub fn empty(text: &'static str) -> AnyElement {
    div()
        .flex_1()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p_8()
        .child(Label::new(text).color(Color::Muted))
        .into_any_element()
}
