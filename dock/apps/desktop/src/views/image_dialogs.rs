//! Image tag and image history (layers) dialogs.

use crate::app::FerrumContainers;
use crate::commands::resources::image_label;
use crate::views::widgets::muted;
use ferrum_ui::Modal;
use ferrumdock_engine::{Image, format};
use gpui::{AnyElement, Context, div, prelude::*, px};
use theme::ActiveTheme;
use ui::{
    Button, ButtonCommon, ButtonStyle, Clickable, Color, Label, LabelCommon, LabelSize,
    StyledTypography, h_flex, v_flex,
};

fn close(
    cx: &mut Context<FerrumContainers>,
) -> impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static {
    cx.listener(|app, _, _, cx| {
        app.state.dialog = None;
        cx.notify();
    })
}

pub fn tag(
    app: &FerrumContainers,
    image: &Image,
    cx: &mut Context<FerrumContainers>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    Modal::new(format!("Tag {}", image_label(image)))
        .width(px(520.))
        .on_dismiss(close(cx))
        .child(
            Label::new("New reference, repository[:tag] (a missing tag means latest)")
                .size(LabelSize::Small)
                .color(Color::Muted),
        )
        .child(
            div()
                .px_2()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(colors.border)
                .bg(colors.editor_background)
                .child(app.inputs.tag.clone()),
        )
        .child(muted(
            "The image keeps its other tags. Pushing is not supported yet.",
        ))
        .footer(Button::new("tag-cancel", "Cancel").on_click(close(cx)))
        .footer(
            Button::new("tag-ok", "Tag")
                .style(ButtonStyle::Filled)
                .on_click(cx.listener(|app, _, _, cx| app.submit_tag(cx))),
        )
        .into_any_element()
}

pub fn history(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let now = chrono::Utc::now().timestamp();
    let (title, body): (String, AnyElement) = match app.state.layers.as_ref() {
        None => (String::new(), div().into_any_element()),
        Some(state) => {
            let body = match &state.layers {
                None => muted("Loading layers…").into_any_element(),
                Some(Err(error)) => Label::new(error.clone())
                    .size(LabelSize::Small)
                    .color(Color::Error)
                    .into_any_element(),
                Some(Ok(layers)) => {
                    let total: u64 = layers.iter().map(|l| l.size).sum();
                    v_flex()
                        .gap_1()
                        .child(muted(format!(
                            "{} layers · {} · newest first",
                            layers.len(),
                            format::bytes(total)
                        )))
                        .child(
                            v_flex()
                                .id("layers")
                                .max_h(px(420.))
                                .overflow_y_scroll()
                                .font_buffer(cx)
                                .children(layers.iter().enumerate().map(|(i, layer)| {
                                    h_flex()
                                        .id(("layer", i))
                                        .py_1()
                                        .gap_3()
                                        .border_b_1()
                                        .border_color(colors.border_variant)
                                        .child(
                                            div()
                                                .flex_none()
                                                .w(px(70.))
                                                .child(muted(format::bytes(layer.size))),
                                        )
                                        .child(
                                            div()
                                                .flex_none()
                                                .w(px(90.))
                                                .child(muted(format::age(layer.created, now))),
                                        )
                                        .child(
                                            div().flex_1().min_w_0().child(
                                                Label::new(layer.created_by.clone())
                                                    .size(LabelSize::XSmall)
                                                    .truncate(),
                                            ),
                                        )
                                })),
                        )
                        .into_any_element()
                }
            };
            (format!("History of {}", state.label), body)
        }
    };
    Modal::new(title)
        .width(px(760.))
        .on_dismiss(close(cx))
        .child(body)
        .footer(Button::new("history-close", "Close").on_click(close(cx)))
        .into_any_element()
}
