//! "Run container" dialog: image, name, ports, env, volumes, command,
//! restart policy and detached.

use crate::app::FerrumContainers;
use crate::views::widgets::segmented;
use ferrum_ui::components::{
    Button, ButtonCommon, ButtonStyle, Checkbox, Clickable, Color, Disableable, Label, LabelCommon,
    LabelSize, ToggleState, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrum_ui::{Modal, TextInput};
use ferrumdock_engine::create::RestartPolicy;
use gpui::{AnyElement, Context, Entity, div, prelude::*, px};

fn field(
    title: &'static str,
    input: &Entity<TextInput>,
    cx: &mut Context<FerrumContainers>,
) -> AnyElement {
    let colors = cx.theme().colors();
    v_flex()
        .gap_0p5()
        .child(Label::new(title).size(LabelSize::Small).color(Color::Muted))
        .child(
            div()
                .px_2()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(colors.border)
                .bg(colors.editor_background)
                .child(input.clone()),
        )
        .into_any_element()
}

pub fn render(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let run = &app.state.run;
    let inputs = &app.inputs.run;
    let restart = RestartPolicy::ALL.map(|r| (r.label(), run.restart == r, false, r));
    let status = match (&run.error, &run.status) {
        (Some(error), _) => Some(
            Label::new(error.clone())
                .size(LabelSize::Small)
                .color(Color::Error),
        ),
        (None, Some(status)) => Some(
            Label::new(status.clone())
                .size(LabelSize::Small)
                .color(Color::Muted),
        ),
        (None, None) => None,
    };
    Modal::new("Run container")
        .width(px(620.))
        .on_dismiss(cx.listener(|app, _, _, cx| {
            app.state.dialog = None;
            cx.notify();
        }))
        .child(field(
            "Image (pulled first when missing)",
            &inputs.image,
            cx,
        ))
        .child(field("Name", &inputs.name, cx))
        .child(field(
            "Ports  host:container[/udp], comma separated",
            &inputs.ports,
            cx,
        ))
        .child(field("Environment", &inputs.env, cx))
        .child(field(
            "Volumes  source:/path[:ro], comma separated",
            &inputs.volumes,
            cx,
        ))
        .child(field("Command", &inputs.command, cx))
        .child(
            h_flex()
                .gap_4()
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Label::new("Restart")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                        .child(segmented("restart", &restart, cx, |app, r, cx| {
                            app.set_restart(r, cx)
                        })),
                )
                .child(
                    Checkbox::new("run-detached", ToggleState::from(run.detached))
                        .label("Detached (otherwise open its logs)")
                        .label_size(LabelSize::Small)
                        .on_click(
                            cx.listener(|app, _: &ToggleState, _, cx| app.toggle_detached(cx)),
                        ),
                ),
        )
        .children(status)
        .footer(
            Button::new("run-cancel", if run.running { "Hide" } else { "Cancel" }).on_click(
                cx.listener(|app, _, _, cx| {
                    app.state.dialog = None;
                    cx.notify();
                }),
            ),
        )
        .footer(
            Button::new("run-start", "Run")
                .style(ButtonStyle::Filled)
                .disabled(run.running)
                .on_click(cx.listener(|app, _, _, cx| app.submit_run(cx))),
        )
        .into_any_element()
}
