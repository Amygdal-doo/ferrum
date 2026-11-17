//! Engine picker: every engine present on this machine (DOCKER_HOST,
//! Docker contexts, Docker Desktop / OrbStack / Colima sockets) and remote
//! `ssh://` contexts; the CLI's Docker contexts (use, remove) and a form to
//! add a remote engine over SSH.

use crate::app::FerrumContainers;
use crate::state::EngineStatus;
use crate::views::widgets::{button, muted};
use ferrum_ui::Modal;
use ferrum_ui::components::{
    Button, ButtonCommon, ButtonStyle, Clickable, Color, Disableable, Icon, IconName, IconSize,
    Label, LabelCommon, LabelSize, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumdock_engine::Endpoint;
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

pub fn render(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let current = match &app.status {
        EngineStatus::Connected(c) => Some(c.candidate.endpoint.clone()),
        _ => None,
    };
    let picker = &app.state.engines;
    let rows = picker.candidates.iter().enumerate().map(|(i, candidate)| {
        let selected = current.as_ref() == Some(&candidate.endpoint);
        let pick = candidate.clone();
        let remote = matches!(candidate.endpoint, Endpoint::Ssh(_));
        h_flex()
            .id(SharedString::from(format!("engine-{i}")))
            .px_2()
            .py_1()
            .gap_2()
            .rounded_md()
            .cursor_pointer()
            .hover(|s| s.bg(colors.element_hover))
            .when(selected, |d| d.bg(colors.element_selected))
            .child(
                Icon::new(if selected {
                    IconName::Check
                } else {
                    IconName::Server
                })
                .size(IconSize::Small)
                .color(if selected {
                    Color::Success
                } else {
                    Color::Muted
                }),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(Label::new(candidate.label()).size(LabelSize::Small))
                    .child(muted(candidate.endpoint.to_string()).truncate()),
            )
            .when(remote && !selected, |d| {
                d.child(muted("remote · connects over SSH"))
            })
            .on_click(cx.listener(move |app, _, _, cx| app.pick_engine(&pick, cx)))
    });
    let rows: Vec<_> = rows.collect();
    let contexts = picker.contexts.iter().enumerate().map(|(i, ctx)| {
        let name = ctx.name.clone();
        let remove = ctx.name.clone();
        h_flex()
            .id(SharedString::from(format!("context-{i}")))
            .px_2()
            .py_0p5()
            .gap_2()
            .child(
                div().flex_none().w(px(130.)).child(
                    Label::new(ctx.name.clone())
                        .size(LabelSize::Small)
                        .truncate(),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(muted(ctx.host.clone()).truncate()),
            )
            .when(ctx.current, |d| {
                d.child(
                    Label::new("CLI current")
                        .size(LabelSize::XSmall)
                        .color(Color::Success),
                )
            })
            .when(!ctx.current, |d| {
                d.child(button(
                    SharedString::from(format!("context-use-{i}")),
                    "Use",
                    picker.busy || ctx.endpoint.is_none(),
                    cx,
                    move |app, cx| app.use_context(name.clone(), cx),
                ))
                .child(button(
                    SharedString::from(format!("context-rm-{i}")),
                    "Remove",
                    picker.busy,
                    cx,
                    move |app, cx| app.confirm_remove_context(remove.clone(), cx),
                ))
            })
    });
    let contexts: Vec<_> = contexts.collect();
    let unsupported = picker.unsupported.iter().map(|c| {
        muted(format!(
            "context {} ({}): not supported yet",
            c.name, c.host
        ))
        .color(Color::Placeholder)
    });
    let field = |input: gpui::Entity<ferrum_ui::TextInput>, width: f32| {
        div()
            .w(px(width))
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(colors.border)
            .bg(colors.editor_background)
            .child(input)
    };
    let empty = !picker.loading && picker.candidates.is_empty();
    let error = picker
        .error
        .clone()
        .map(|e| Label::new(e).size(LabelSize::Small).color(Color::Error));
    Modal::new("Engine")
        .width(px(600.))
        .on_dismiss(cx.listener(|app, _, _, cx| {
            app.state.dialog = None;
            cx.notify();
        }))
        .child(muted(
            "Engines found on this machine. The one you pick is tried first from now on.",
        ))
        .when(picker.loading, |m| m.child(muted("Looking…")))
        .when(empty, |m| {
            m.child(muted("No engine socket or context found."))
        })
        .child(v_flex().gap_0p5().children(rows))
        .when(!picker.contexts.is_empty(), |m| {
            m.child(Label::new("Docker contexts").weight(gpui::FontWeight::SEMIBOLD))
                .child(muted(
                    "Use makes it the docker CLI's current context (docker context use) and connects to it.",
                ))
                .child(v_flex().children(contexts))
        })
        .child(div().children(unsupported))
        .child(Label::new("Add a remote engine over SSH").weight(gpui::FontWeight::SEMIBOLD))
        .child(
            h_flex()
                .gap_2()
                .child(field(app.inputs.context_name.clone(), 170.))
                .child(div().flex_1().child(field(app.inputs.context_host.clone(), 300.)))
                .child(
                    Button::new("context-add", "Add and connect")
                        .style(ButtonStyle::Filled)
                        .disabled(picker.busy)
                        .on_click(cx.listener(|app, _, _, cx| app.add_ssh_context(cx))),
                ),
        )
        .child(muted(
            "Creates a Docker context (docker context create --docker host=ssh://…). \
             FerrumContainers reaches the remote /var/run/docker.sock through ssh: \
             your keys or SSH agent must log in without a prompt.",
        ))
        .children(error)
        .footer(
            Button::new("engines-close", "Close").on_click(cx.listener(|app, _, _, cx| {
                app.state.dialog = None;
                cx.notify();
            })),
        )
        .into_any_element()
}
