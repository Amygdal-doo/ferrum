//! Container detail panel: the Inspect tab (env, mounts, networks, labels).

use crate::app::FerrumContainers;
use crate::views::widgets::{icon_button, muted};
use ferrumdock_engine::ContainerDetail;
use gpui::{AnyElement, App, Context, SharedString, div, prelude::*, px};
use ui::{Color, IconName, Label, LabelCommon, LabelSize, StyledTypography, h_flex, v_flex};

pub fn render(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let Some(detail) = app.state.detail.as_ref() else {
        return div().into_any_element();
    };
    let content: AnyElement = match &detail.inspect {
        None => ferrum_ui::empty_state("Loading…", "").into_any_element(),
        Some(Err(error)) => {
            ferrum_ui::empty_state("Inspect failed", error.clone()).into_any_element()
        }
        Some(Ok(d)) => inspect_body(d, cx),
    };
    v_flex()
        .flex_1()
        .min_h_0()
        .child(
            h_flex()
                .flex_none()
                .px_3()
                .py_1()
                .child(div().flex_1())
                .child(icon_button(
                    "inspect-reload",
                    IconName::RotateCw,
                    "Reload",
                    false,
                    cx,
                    |app, cx| app.load_inspect(cx),
                )),
        )
        .child(content)
        .into_any_element()
}

fn inspect_body(d: &ContainerDetail, cx: &App) -> AnyElement {
    let section = |title: &str, rows: Vec<(String, String)>| {
        let empty = rows.is_empty();
        v_flex()
            .gap_0p5()
            .child(ferrum_ui::section_header(title.to_owned()))
            .when(empty, |s| s.child(div().px_3().child(muted("None"))))
            .children(rows.into_iter().map(|(k, v)| kv(k, v, cx)))
    };
    let mut overview = vec![
        ("ID".to_owned(), d.id.clone()),
        ("Image".to_owned(), d.image.clone()),
        ("State".to_owned(), d.state.label().to_owned()),
        ("Command".to_owned(), d.command.clone()),
        ("Created".to_owned(), d.created.clone()),
        ("Restarts".to_owned(), d.restart_count.to_string()),
        (
            "TTY".to_owned(),
            if d.tty { "yes" } else { "no" }.to_owned(),
        ),
    ];
    if let Some(started) = &d.started_at {
        overview.push(("Started".to_owned(), started.clone()));
    }
    if let Some(dir) = &d.working_dir {
        overview.push(("Working dir".to_owned(), dir.clone()));
    }
    let mounts = d
        .mounts
        .iter()
        .map(|m| {
            (
                m.destination.clone(),
                format!(
                    "{} {}{}",
                    m.kind,
                    m.source,
                    if m.read_write { "" } else { " (read-only)" }
                ),
            )
        })
        .collect();
    let networks = d
        .networks
        .iter()
        .map(|n| {
            let aliases = if n.aliases.is_empty() {
                String::new()
            } else {
                format!(" · aliases {}", n.aliases.join(", "))
            };
            (
                n.network.clone(),
                format!("{} gw {}{aliases}", n.ip_address, n.gateway),
            )
        })
        .collect();
    v_flex()
        .id("inspect-body")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .pb_4()
        .gap_3()
        .child(section("Overview", overview))
        .child(section("Environment", d.env.clone()))
        .child(section("Mounts", mounts))
        .child(section("Networks", networks))
        .child(section(
            "Labels",
            d.labels
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ))
        .into_any_element()
}

fn kv(key: String, value: String, cx: &App) -> impl IntoElement {
    h_flex()
        .px_3()
        .gap_3()
        .items_start()
        .child(
            div().flex_none().w(px(170.)).overflow_hidden().child(
                Label::new(SharedString::from(key))
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .truncate(),
            ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_buffer(cx)
                .text_ui_sm(cx)
                .child(value),
        )
}
