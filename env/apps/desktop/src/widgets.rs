//! Small building blocks shared by the views and dialogs.

use ferrum_ui::prelude::*;

use crate::app::EnvApp;

/// A labelled form row.
pub fn field(label: impl Into<SharedString>, input: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(
            Label::new(label.into())
                .size(LabelSize::Small)
                .color(Color::Muted),
        )
        .child(input)
}

pub fn error_text(message: impl Into<SharedString>) -> impl IntoElement {
    Label::new(message.into())
        .size(LabelSize::Small)
        .color(Color::Error)
}

pub fn muted(text: impl Into<SharedString>) -> Label {
    Label::new(text.into())
        .size(LabelSize::Small)
        .color(Color::Muted)
}

/// A compact icon button with a tooltip.
pub fn icon_button(
    id: impl Into<ElementId>,
    icon: IconName,
    tooltip: &'static str,
    cx: &mut Context<EnvApp>,
    on_click: impl Fn(&mut EnvApp, &mut Window, &mut Context<EnvApp>) + 'static,
) -> IconButton {
    IconButton::new(id, icon)
        .icon_size(IconSize::Small)
        .tooltip(Tooltip::text(tooltip))
        .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
}

/// A small text button.
pub fn text_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &mut Context<EnvApp>,
    on_click: impl Fn(&mut EnvApp, &mut Window, &mut Context<EnvApp>) + 'static,
) -> Button {
    Button::new(id, label)
        .style(ButtonStyle::Subtle)
        .size(ButtonSize::Compact)
        .label_size(LabelSize::Small)
        .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
}

/// The primary (filled) button of a dialog.
pub fn primary_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &mut Context<EnvApp>,
    on_click: impl Fn(&mut EnvApp, &mut Window, &mut Context<EnvApp>) + 'static,
) -> Button {
    Button::new(id, label)
        .style(ButtonStyle::Filled)
        .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
}
