//! Services tab of a compose project: `docker compose ps --all`.

use crate::app::FerrumDock;
use crate::views::containers::state_color;
use crate::views::widgets::{button, cell, grow_cell, header_row, muted, row, state_dot};
use gpui::{AnyElement, Context, SharedString, div, prelude::*};
use ui::{Color, Label, LabelCommon, LabelSize, h_flex, v_flex};

const COLUMNS: [(&str, Option<f32>); 4] = [
    ("SERVICE", None),
    ("STATE", Some(150.)),
    ("PORTS", Some(130.)),
    ("IMAGE", Some(140.)),
];

pub fn render(app: &FerrumDock, cx: &mut Context<FerrumDock>) -> AnyElement {
    let Some(detail) = app.state.detail.as_ref() else {
        return div().into_any_element();
    };
    let toolbar = h_flex()
        .flex_none()
        .px_3()
        .py_1()
        .gap_2()
        .child(muted("docker compose ps --all"))
        .child(div().flex_1())
        .child(button("ps-refresh", "Refresh", false, cx, |app, cx| {
            app.load_services(cx)
        }));
    let body: AnyElement = match &detail.services {
        None => ferrum_ui::empty_state("Running docker compose ps…", "").into_any_element(),
        Some(Err(error)) => {
            ferrum_ui::empty_state("compose ps failed", error.clone()).into_any_element()
        }
        Some(Ok(services)) if services.is_empty() => ferrum_ui::empty_state(
            "No services",
            "compose reports no containers for this project.",
        )
        .into_any_element(),
        Some(Ok(services)) => v_flex()
            .id("services-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(services.iter().enumerate().map(|(i, s)| {
                let name = h_flex()
                    .gap_2()
                    .child(state_dot(state_color(s.state, cx)))
                    .child(Label::new(s.service.clone()).size(LabelSize::Small))
                    .child(muted(s.name.clone()));
                row(SharedString::from(format!("service-{i}")), false, cx)
                    .child(grow_cell(name))
                    .child(cell(150., s.status.clone(), Color::Default))
                    .child(cell(130., s.ports.join(", "), Color::Muted))
                    .child(cell(140., s.image.clone(), Color::Muted))
            }))
            .into_any_element(),
    };
    v_flex()
        .flex_1()
        .min_h_0()
        .child(toolbar)
        .child(header_row(&COLUMNS, cx))
        .child(body)
        .into_any_element()
}
