//! Confirm and pull dialogs; dispatch to the run, tag, history and engine dialogs.

use crate::app::FerrumContainers;
use crate::state::Dialog;
use ferrum_ui::Modal;
use gpui::{AnyElement, Context, Window, div, prelude::*, px, relative};
use theme::ActiveTheme;
use ui::{
    Button, ButtonCommon, ButtonStyle, Clickable, Color, Disableable, Label, LabelCommon,
    LabelSize, TintColor, v_flex,
};

pub fn render(
    app: &FerrumContainers,
    _window: &mut Window,
    cx: &mut Context<FerrumContainers>,
) -> Option<AnyElement> {
    let close = cx.listener(|app: &mut FerrumContainers, _, _, cx| {
        app.state.dialog = None;
        cx.notify();
    });
    match app.state.dialog.as_ref()? {
        Dialog::Confirm(confirm) => Some(
            Modal::new(confirm.title.clone())
                .on_dismiss(close)
                .child(Label::new(confirm.body.clone()))
                .footer(
                    Button::new("confirm-cancel", "Cancel").on_click(cx.listener(
                        |app, _, _, cx| {
                            app.state.dialog = None;
                            cx.notify();
                        },
                    )),
                )
                .footer(
                    Button::new("confirm-ok", confirm.confirm_label.clone())
                        .style(ButtonStyle::Tinted(TintColor::Error))
                        .on_click(cx.listener(|app, _, _, cx| app.accept_confirm(cx))),
                )
                .into_any_element(),
        ),
        Dialog::Pull => Some(pull(app, cx)),
        Dialog::Run => Some(super::run_dialog::render(app, cx)),
        Dialog::Tag(image) => Some(super::image_dialogs::tag(app, image, cx)),
        Dialog::History => Some(super::image_dialogs::history(app, cx)),
        Dialog::Engines => Some(super::engines::render(app, cx)),
    }
}

fn pull(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let status = theme.status();
    let pull = app.state.pull.as_ref();
    let running = pull.is_some_and(|p| p.running);
    let progress = pull.map(|p| {
        let fraction = p.progress.fraction().unwrap_or(0.0).clamp(0.0, 1.0);
        let (text, color) = match (&p.error, p.running) {
            (Some(error), _) => (error.clone(), Color::Error),
            (None, true) => (
                format!("{} · {}", p.reference, p.progress.summary()),
                Color::Muted,
            ),
            (None, false) => (format!("Pulled {}", p.reference), Color::Success),
        };
        let bar_color = if p.error.is_some() {
            status.error
        } else {
            colors.text_accent
        };
        v_flex()
            .gap_1()
            .child(
                div()
                    .h(px(6.))
                    .w_full()
                    .rounded_full()
                    .bg(colors.element_background)
                    .child(
                        div()
                            .h_full()
                            .rounded_full()
                            .bg(bar_color)
                            .w(relative(fraction)),
                    ),
            )
            .child(Label::new(text).size(LabelSize::Small).color(color))
    });
    Modal::new("Pull image")
        .width(px(520.))
        .on_dismiss(cx.listener(|app, _, _, cx| {
            app.state.dialog = None;
            cx.notify();
        }))
        .child(
            Label::new("Image reference (a missing tag means latest)")
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
                .child(app.inputs.pull.clone()),
        )
        .children(progress)
        .footer(
            Button::new("pull-close", if running { "Hide" } else { "Close" }).on_click(
                cx.listener(|app, _, _, cx| {
                    app.state.dialog = None;
                    cx.notify();
                }),
            ),
        )
        .footer(
            Button::new("pull-start", "Pull")
                .style(ButtonStyle::Filled)
                .disabled(running)
                .on_click(cx.listener(|app, _, _, cx| app.start_pull(cx))),
        )
        .into_any_element()
}
