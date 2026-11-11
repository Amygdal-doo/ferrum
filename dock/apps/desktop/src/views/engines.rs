//! Engine picker: every engine present on this machine (DOCKER_HOST,
//! Docker contexts, Docker Desktop / OrbStack / Colima sockets).

use crate::app::FerrumDock;
use crate::state::EngineStatus;
use crate::views::widgets::muted;
use ferrum_ui::Modal;
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};
use theme::ActiveTheme;
use ui::{
    Button, Clickable, Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, h_flex,
    v_flex,
};

pub fn render(app: &FerrumDock, cx: &mut Context<FerrumDock>) -> AnyElement {
    let colors = cx.theme().colors();
    let current = match &app.status {
        EngineStatus::Connected(c) => Some(c.candidate.endpoint.clone()),
        _ => None,
    };
    let picker = &app.state.engines;
    let rows = picker.candidates.iter().enumerate().map(|(i, candidate)| {
        let selected = current.as_ref() == Some(&candidate.endpoint);
        let pick = candidate.clone();
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
            .on_click(cx.listener(move |app, _, _, cx| app.pick_engine(&pick, cx)))
    });
    let unsupported = picker.unsupported.iter().map(|c| {
        muted(format!(
            "context {} ({}): not supported yet",
            c.name, c.host
        ))
        .color(Color::Placeholder)
    });
    let empty = !picker.loading && picker.candidates.is_empty();
    Modal::new("Engine")
        .width(px(520.))
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
        .child(div().children(unsupported))
        .footer(
            Button::new("engines-close", "Close").on_click(cx.listener(|app, _, _, cx| {
                app.state.dialog = None;
                cx.notify();
            })),
        )
        .into_any_element()
}
