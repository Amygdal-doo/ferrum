//! Registries dialog: the registries signed in to (Sign out) and the
//! sign-in form (Docker Hub, GHCR, Amazon ECR with a token, any registry).

use crate::app::FerrumContainers;
use crate::state::RegistryPreset;
use crate::views::widgets::{button, muted, segmented};
use ferrum_ui::Modal;
use ferrum_ui::components::{
    Button, ButtonCommon, ButtonStyle, Clickable, Color, Disableable, Label, LabelCommon,
    LabelSize, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumdock_engine::registry;
use gpui::{AnyElement, Context, Entity, SharedString, div, prelude::*, px};

fn field(
    label: &'static str,
    input: Entity<ferrum_ui::TextInput>,
    cx: &Context<FerrumContainers>,
) -> impl IntoElement {
    let colors = cx.theme().colors();
    v_flex()
        .gap_0p5()
        .child(Label::new(label).size(LabelSize::Small).color(Color::Muted))
        .child(
            div()
                .px_2()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(colors.border)
                .bg(colors.editor_background)
                .child(input),
        )
}

pub fn render(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let state = &app.state.registries;
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let signed_in: AnyElement = match &state.signed_in {
        None => muted("Reading stored credentials…").into_any_element(),
        Some(Err(error)) => Label::new(error.clone())
            .size(LabelSize::Small)
            .color(Color::Error)
            .into_any_element(),
        Some(Ok(list)) if list.is_empty() => {
            muted("Not signed in to any registry. Pulls are anonymous.").into_any_element()
        }
        Some(Ok(list)) => v_flex()
            .gap_0p5()
            .children(list.iter().enumerate().map(|(i, entry)| {
                let server = entry.server.clone();
                h_flex()
                    .id(SharedString::from(format!("registry-{i}")))
                    .px_2()
                    .py_1()
                    .gap_2()
                    .rounded_md()
                    .border_1()
                    .border_color(colors.border_variant)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                Label::new(registry::display_name(&entry.server))
                                    .size(LabelSize::Small),
                            )
                            .child(muted(format!("{} · {}", entry.username, entry.store))),
                    )
                    .child(button(
                        SharedString::from(format!("sign-out-{i}")),
                        "Sign out",
                        state.busy,
                        cx,
                        move |app, cx| app.sign_out(server.clone(), cx),
                    ))
            }))
            .into_any_element(),
    };
    let presets = RegistryPreset::ALL.map(|p| (p.label(), state.preset == p, false, p));
    let error = state
        .error
        .clone()
        .map(|e| Label::new(e).size(LabelSize::Small).color(Color::Error));
    let inputs = &app.inputs.registry;
    Modal::new("Registries")
        .width(px(560.))
        .on_dismiss(cx.listener(|app, _, _, cx| {
            app.state.dialog = None;
            cx.notify();
        }))
        .child(signed_in)
        .child(div().h(px(6.)))
        .child(Label::new("Sign in").weight(gpui::FontWeight::SEMIBOLD))
        .child(segmented(
            "registry-preset",
            &presets,
            cx,
            |app, preset, cx| app.pick_registry_preset(preset, cx),
        ))
        .child(field("Registry", inputs.server.clone(), cx))
        .child(field("Username", inputs.username.clone(), cx))
        .child(field("Password or token", inputs.secret.clone(), cx))
        .child(muted(state.preset.secret_hint()))
        .child(muted(
            "The engine checks the credentials first. They are stored like `docker login` does: \
             in the credential helper of ~/.docker/config.json, else the OS keychain. \
             Pulls from that registry then use them.",
        ))
        .children(error)
        .footer(
            Button::new("registries-close", "Close").on_click(cx.listener(|app, _, _, cx| {
                app.state.dialog = None;
                cx.notify();
            })),
        )
        .footer(
            Button::new(
                "registries-sign-in",
                if state.busy {
                    "Signing in…"
                } else {
                    "Sign in"
                },
            )
            .style(ButtonStyle::Filled)
            .disabled(state.busy || app.engine().is_none())
            .on_click(cx.listener(|app, _, _, cx| app.sign_in(cx))),
        )
        .into_any_element()
}
