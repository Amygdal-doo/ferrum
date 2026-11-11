//! Toolbar, sidebar, status bar, notices and the connecting / offline screens.

use crate::app::FerrumContainers;
use crate::state::{EngineStatus, Section};
use crate::views::widgets::{button, muted};
use ferrum_ui::{Notice, badge, render_notice, section_header};
use gpui::{AnyElement, Context, div, prelude::*, px};
use theme::ActiveTheme;
use ui::{Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, h_flex, v_flex};

pub fn toolbar(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let theme = cx.theme().clone();
    let status = theme.status();
    let engine_badge = match &app.status {
        EngineStatus::Connected(c) => {
            let name = c
                .info
                .platform
                .clone()
                .unwrap_or_else(|| c.candidate.source.to_owned());
            badge(
                format!("{name} · Engine {}", c.info.version),
                status.success,
                cx,
            )
            .into_any_element()
        }
        EngineStatus::Connecting => badge("Connecting…", status.info, cx).into_any_element(),
        EngineStatus::Offline(_) => badge("No engine", status.error, cx).into_any_element(),
    };
    let connected = matches!(app.status, EngineStatus::Connected(_));
    let context = match &app.status {
        EngineStatus::Connected(c) => c.candidate.context.clone(),
        _ => None,
    };
    ferrum_ui::toolbar(cx)
        .child(
            Icon::new(IconName::Box)
                .size(IconSize::Small)
                .color(Color::Accent),
        )
        .child(Label::new("FerrumContainers").weight(gpui::FontWeight::SEMIBOLD))
        .child(engine_badge)
        .when_some(context, |bar, name| {
            bar.child(muted(format!("context {name}")))
        })
        .when(!matches!(app.status, EngineStatus::Connecting), |bar| {
            bar.child(button("pick-engine", "Engine…", false, cx, |app, cx| {
                app.open_engine_picker(cx)
            }))
        })
        .child(div().flex_1())
        .when(connected, |bar| {
            bar.child(
                div()
                    .w(px(260.))
                    .px_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.colors().border)
                    .bg(theme.colors().editor_background)
                    .child(app.inputs.filter.clone()),
            )
            .child(button("refresh", "Refresh", false, cx, |app, cx| {
                app.refresh(app.state.section, cx);
            }))
        })
        .into_any_element()
}

pub fn sidebar(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let (hover, selected_bg) = (colors.element_hover, colors.element_selected);
    let items = Section::ALL.into_iter().map(|section| {
        let (icon, count) = match section {
            Section::Containers => (IconName::Box, app.state.containers.len()),
            Section::Images => (IconName::Server, app.state.images.len()),
            Section::Volumes => (IconName::Folder, app.state.volumes.len()),
            Section::Networks => (IconName::Link, app.state.networks.len()),
        };
        let selected = app.state.section == section;
        let loaded = app.state.loaded.contains(&section);
        h_flex()
            .id(section.title())
            .mx_2()
            .px_2()
            .py_1()
            .gap_2()
            .rounded_md()
            .cursor_pointer()
            .hover(|s| s.bg(hover))
            .when(selected, |d| d.bg(selected_bg))
            .child(Icon::new(icon).size(IconSize::Small).color(Color::Muted))
            .child(Label::new(section.title()).size(LabelSize::Small))
            .child(div().flex_1())
            .when(loaded, |d| d.child(muted(count.to_string())))
            .on_click(cx.listener(move |app, _, _, cx| app.select_section(section, cx)))
    });
    v_flex()
        .flex_none()
        .w(px(190.))
        .h_full()
        .pt_2()
        .gap_0p5()
        .bg(colors.panel_background)
        .border_r_1()
        .border_color(colors.border)
        .child(section_header("Engine"))
        .children(items)
        .into_any_element()
}

pub fn status_bar(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let left = match &app.status {
        EngineStatus::Connected(c) => format!(
            "{} · API {} · {}/{}",
            c.candidate.endpoint, c.info.api_version, c.info.os, c.info.arch
        ),
        EngineStatus::Connecting => "Looking for a Docker engine…".to_owned(),
        EngineStatus::Offline(_) => "Not connected".to_owned(),
    };
    let mut right = Vec::new();
    if let Some(pull) = app.state.pull.as_ref().filter(|p| p.running) {
        right.push(format!(
            "Pulling {} · {}",
            pull.reference,
            pull.progress.summary()
        ));
    }
    if app.state.loaded.contains(&Section::Containers) {
        right.push(format!(
            "{} running / {} containers",
            app.state.running_count(),
            app.state.containers.len()
        ));
    }
    ferrum_ui::status_bar(cx)
        .child(div().truncate().child(left))
        .child(div().truncate().child(right.join("   ")))
        .into_any_element()
}

pub fn notice(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> Option<AnyElement> {
    let n = app.state.notice.as_ref()?;
    let notice = if n.error {
        Notice::error(n.message.clone())
    } else {
        Notice::info(n.message.clone())
    };
    Some(render_notice(
        &notice,
        cx.listener(|app, _, _, cx| {
            app.state.notice = None;
            cx.notify();
        }),
        cx,
    ))
}

pub fn connecting() -> AnyElement {
    ferrum_ui::empty_state(
        "Connecting…",
        "Looking for Docker Desktop, OrbStack, Colima or DOCKER_HOST",
    )
    .into_any_element()
}

/// Shown when no engine answers: what to do, the error, and Retry.
pub fn offline(error: &str, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let detail = if error == "no container engine found" {
        "No Docker socket was found (DOCKER_HOST, ~/.docker/run, ~/.orbstack/run, ~/.colima/default, /var/run).".to_owned()
    } else {
        error.to_owned()
    };
    v_flex()
        .flex_1()
        .size_full()
        .items_center()
        .justify_center()
        .gap_3()
        .p_8()
        .child(
            Icon::new(IconName::Box)
                .size(IconSize::XLarge)
                .color(Color::Muted),
        )
        .child(Label::new("No container engine is running").size(LabelSize::Large))
        .child(
            Label::new("Start Docker Desktop, OrbStack or Colima, then Retry.").color(Color::Muted),
        )
        .child(div().max_w(px(560.)).child(muted(detail)))
        .child(button("retry", "Retry", false, cx, |app, cx| {
            app.connect(false, cx)
        }))
        .child(
            muted("FerrumContainers also retries on its own every few seconds.")
                .color(Color::Placeholder),
        )
        .into_any_element()
}
