//! Small building blocks shared by the tool views.

use ferrum_ui::gpui::{AnyElement, ClipboardItem, Div, ElementId, Hsla, Pixels};
use ferrum_ui::prelude::*;

/// Events a tool view sends to the root view.
pub enum KitEvent {
    Notice(Notice),
}

/// An editable, monospace, multi-line field.
pub fn editor(
    placeholder: &'static str,
    lines: usize,
    window: &mut Window,
    cx: &mut App,
) -> Entity<TextInput> {
    cx.new(|cx| {
        TextInput::new(window, cx)
            .multiline(lines)
            .monospace()
            .placeholder(placeholder)
    })
}

/// A single-line field.
pub fn line_input(
    placeholder: &'static str,
    monospace: bool,
    window: &mut Window,
    cx: &mut App,
) -> Entity<TextInput> {
    cx.new(|cx| {
        let input = TextInput::new(window, cx).placeholder(placeholder);
        if monospace { input.monospace() } else { input }
    })
}

/// A read-only, selectable, monospace, multi-line field for results.
pub fn output(lines: usize, window: &mut Window, cx: &mut App) -> Entity<TextInput> {
    cx.new(|cx| {
        TextInput::new(window, cx)
            .multiline(lines)
            .monospace()
            .read_only(true)
    })
}

/// Calls `on_change` whenever the user edits `input`.
pub fn on_change<V: 'static>(
    input: &Entity<TextInput>,
    cx: &mut Context<V>,
    on_change: impl Fn(&mut V, &mut Context<V>) + 'static,
) -> Subscription {
    cx.subscribe(input, move |this, _, event, cx| {
        if *event == TextInputEvent::Changed {
            on_change(this, cx);
            cx.notify();
        }
    })
}

pub fn set_text(input: &Entity<TextInput>, text: impl Into<String>, cx: &mut App) {
    let text = text.into();
    input.update(cx, |input, cx| input.set_text(text, cx));
}

pub fn text_of(input: &Entity<TextInput>, cx: &App) -> String {
    input.read(cx).text().to_string()
}

/// A copy-to-clipboard icon button. Copies `text` and shows a toast.
pub fn copy_button<V: EventEmitter<KitEvent>>(
    id: impl Into<ElementId>,
    text: String,
    cx: &mut Context<V>,
) -> AnyElement {
    let empty = text.is_empty();
    IconButton::new(id, IconName::Copy)
        .icon_size(IconSize::Small)
        .disabled(empty)
        .tooltip(Tooltip::text("Copy"))
        .on_click(cx.listener(move |_, _, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            cx.emit(KitEvent::Notice(Notice::info("Copied to clipboard")));
        }))
        .into_any_element()
}

/// A row of mutually exclusive buttons; `current` is highlighted.
pub fn choice<V: 'static, T: Copy + PartialEq + 'static>(
    id: &'static str,
    options: &[(T, &'static str)],
    current: T,
    on_select: impl Fn(&mut V, T, &mut Window, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    h_flex()
        .gap_0p5()
        .children(options.iter().enumerate().map(|(i, (value, label))| {
            let value = *value;
            let on_select = on_select.clone();
            Button::new(SharedString::from(format!("{id}-{i}")), *label)
                .style(ButtonStyle::Subtle)
                .size(ButtonSize::Compact)
                .toggle_state(value == current)
                .selected_style(ButtonStyle::Tinted(
                    ferrum_ui::components::TintColor::Accent,
                ))
                .on_click(cx.listener(move |this, _, window, cx| {
                    on_select(this, value, window, cx);
                    cx.notify();
                }))
        }))
        .into_any_element()
}

/// A labelled column: small caption with optional trailing controls, then
/// `body` filling the remaining height in a scroll container.
pub fn panel(
    id: impl Into<SharedString>,
    title: impl Into<SharedString>,
    trailing: Option<AnyElement>,
    body: impl IntoElement,
) -> AnyElement {
    let id = id.into();
    v_flex()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .gap_1()
        .child(
            h_flex()
                .h_6()
                .justify_between()
                .child(caption(title))
                .children(trailing),
        )
        .child(
            div()
                .id(id)
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(body),
        )
        .into_any_element()
}

pub fn caption(text: impl Into<SharedString>) -> AnyElement {
    Label::new(text.into())
        .size(LabelSize::Small)
        .color(Color::Muted)
        .into_any_element()
}

/// A one-line message under the controls: error (red) or success (green).
pub fn status_line(result: Result<String, String>, cx: &App) -> AnyElement {
    let status = cx.theme().status();
    let (color, text) = match result {
        Ok(text) => (status.success, text),
        Err(text) => (status.error, text),
    };
    div()
        .text_sm()
        .text_color(color)
        .child(text)
        .into_any_element()
}

/// A `label | value | copy` row for key/value results.
pub fn value_row<V: EventEmitter<KitEvent>>(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    value: String,
    cx: &mut Context<V>,
) -> AnyElement {
    value_row_with(id, label, value.clone(), value, cx)
}

/// Like [`value_row`], but copies `copy` instead of the displayed text.
pub fn value_row_with<V: EventEmitter<KitEvent>>(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    display: String,
    copy: String,
    cx: &mut Context<V>,
) -> AnyElement {
    let id = id.into();
    let colors = cx.theme().colors();
    h_flex()
        .gap_3()
        .px_2()
        .py_1()
        .rounded_sm()
        .bg(colors.editor_background)
        .border_1()
        .border_color(colors.border_variant)
        .child(div().w(px(150.)).flex_none().child(caption(label)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_buffer(cx)
                .text_sm()
                .child(display),
        )
        .child(copy_button(
            SharedString::from(format!("copy-{id}")),
            copy,
            cx,
        ))
        .into_any_element()
}

/// A toolbar row of controls above a tool's panes.
pub fn controls() -> Div {
    h_flex().flex_wrap().gap_3().items_center()
}

/// Muted explanatory text.
pub fn hint(text: impl Into<SharedString>) -> AnyElement {
    Label::new(text.into())
        .size(LabelSize::Small)
        .color(Color::Placeholder)
        .into_any_element()
}

/// A coloured dot used for the colour swatch and status chips.
pub fn swatch(color: Hsla, size: Pixels, cx: &App) -> AnyElement {
    div()
        .size(size)
        .flex_none()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().colors().border)
        .bg(color)
        .into_any_element()
}
