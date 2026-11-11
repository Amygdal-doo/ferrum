//! Small building blocks: buttons, table cells, captions.

use crate::app::FerrumContainers;
use gpui::{AnyElement, App, Context, Div, ElementId, Hsla, SharedString, div, prelude::*, px};
use std::rc::Rc;
use theme::ActiveTheme;
use ui::{
    Button, ButtonCommon, ButtonSize, ButtonStyle, Clickable, Color, Disableable, IconButton,
    IconName, IconSize, Label, LabelCommon, LabelSize, Toggleable, Tooltip, h_flex,
};

/// Compact outlined button bound to the root view.
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    disabled: bool,
    cx: &mut Context<FerrumContainers>,
    on_click: impl Fn(&mut FerrumContainers, &mut Context<FerrumContainers>) + 'static,
) -> Button {
    Button::new(id, label)
        .style(ButtonStyle::Outlined)
        .size(ButtonSize::Compact)
        .disabled(disabled)
        .on_click(cx.listener(move |app, _, _, cx| {
            // Buttons sit inside clickable rows (compose group headers).
            cx.stop_propagation();
            on_click(app, cx);
        }))
}

/// Small icon button with a tooltip.
pub fn icon_button(
    id: impl Into<ElementId>,
    icon: IconName,
    tooltip: &'static str,
    disabled: bool,
    cx: &mut Context<FerrumContainers>,
    on_click: impl Fn(&mut FerrumContainers, &mut Context<FerrumContainers>) + 'static,
) -> IconButton {
    IconButton::new(id, icon)
        .icon_size(IconSize::Small)
        .tooltip(Tooltip::text(tooltip))
        .disabled(disabled)
        .on_click(cx.listener(move |app, _, _, cx| {
            cx.stop_propagation();
            on_click(app, cx);
        }))
}

/// A fixed-width table cell with truncated text.
pub fn cell(width: f32, text: impl Into<SharedString>, color: Color) -> Div {
    div()
        .flex_none()
        .w(px(width))
        .overflow_hidden()
        .pr_2()
        .child(
            Label::new(text.into())
                .size(LabelSize::Small)
                .color(color)
                .truncate(),
        )
}

/// The flexible first column.
pub fn grow_cell(child: impl IntoElement) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .pr_2()
        .child(child)
}

/// Column header row; `columns` are (title, width) with `None` for the flexible one.
pub fn header_row(columns: &[(&'static str, Option<f32>)], cx: &App) -> Div {
    let colors = cx.theme().colors();
    h_flex()
        .flex_none()
        .px_3()
        .py_1()
        .border_b_1()
        .border_color(colors.border_variant)
        .children(columns.iter().map(|(title, width)| {
            let label = Label::new(*title)
                .size(LabelSize::XSmall)
                .color(Color::Muted);
            match width {
                Some(w) => div().flex_none().w(px(*w)).pr_2().child(label),
                None => div().flex_1().min_w_0().pr_2().child(label),
            }
        }))
}

/// A list row with hover background; `selected` highlights it.
pub fn row(id: impl Into<ElementId>, selected: bool, cx: &App) -> gpui::Stateful<Div> {
    let colors = cx.theme().colors();
    h_flex()
        .id(id)
        .flex_none()
        .px_3()
        .py_1()
        .min_h(px(30.))
        .border_b_1()
        .border_color(colors.border_variant.opacity(0.5))
        .hover(|s| s.bg(colors.element_hover))
        .when(selected, |d| d.bg(colors.element_selected))
}

/// Coloured dot for a container state.
pub fn state_dot(color: Hsla) -> Div {
    div().flex_none().size(px(8.)).rounded_full().bg(color)
}

/// Section header: title, count, then actions on the right.
pub fn section_bar(title: &str, count: usize, actions: Vec<AnyElement>, cx: &App) -> Div {
    let colors = cx.theme().colors();
    h_flex()
        .flex_none()
        .h(px(40.))
        .px_3()
        .gap_2()
        .border_b_1()
        .border_color(colors.border)
        .child(Label::new(title.to_owned()).size(LabelSize::Large))
        .child(Label::new(count.to_string()).color(Color::Muted))
        .child(div().flex_1())
        .children(actions)
}

pub fn muted(text: impl Into<SharedString>) -> Label {
    Label::new(text.into())
        .size(LabelSize::Small)
        .color(Color::Muted)
}

/// A row of small toggle buttons; `options` are (label, selected, disabled, value).
pub fn segmented<T: Copy + 'static>(
    id: &'static str,
    options: &[(&'static str, bool, bool, T)],
    cx: &mut Context<FerrumContainers>,
    on_pick: impl Fn(&mut FerrumContainers, T, &mut Context<FerrumContainers>) + 'static,
) -> Div {
    let on_pick = Rc::new(on_pick);
    h_flex()
        .flex_none()
        .gap_0p5()
        .children(
            options
                .iter()
                .enumerate()
                .map(|(i, &(label, selected, disabled, value))| {
                    let on_pick = on_pick.clone();
                    Button::new((id, i), label)
                        .style(ButtonStyle::Subtle)
                        .size(ButtonSize::Compact)
                        .label_size(LabelSize::XSmall)
                        .toggle_state(selected)
                        .disabled(disabled)
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            on_pick(app, value, cx);
                        }))
                }),
        )
}
