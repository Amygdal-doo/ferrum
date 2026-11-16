//! Card header and footer, sidebar, notices and the connecting / offline screens.

use crate::app::FerrumContainers;
use crate::state::{EngineStatus, Section};
use crate::views::widgets::{button, muted};
use ferrum_ui::components::Clickable;
use ferrum_ui::components::{
    Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrum_ui::{Notice, badge, render_notice};
use gpui::{AnyElement, Context, Window, div, prelude::*, px};

/// ◧, ⌘ and + : in the sidebar's title row, or at the start of the card
/// header while the sidebar is hidden.
pub fn title_controls(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let connected = matches!(app.status, EngineStatus::Connected(_));
    h_flex()
        .flex_none()
        .gap_0p5()
        .child(
            ferrum_ui::sidebar_toggle("toggle-sidebar", app.sidebar_open)
                .on_click(cx.listener(|app, _, _, cx| app.toggle_sidebar(cx))),
        )
        .child(ferrum_ui::settings_button("open-settings"))
        .child(
            ferrum_ui::command_button("switch-project", "Switch project (⌥⌘O)").on_click(
                cx.listener(|app, _, window, cx| {
                    app.project_bar
                        .update(cx, |bar, cx| bar.toggle_switcher(window, cx));
                }),
            ),
        )
        .when(connected, |row| {
            row.child(
                ferrum_ui::title_button("new-container", IconName::Plus, "Run a container…")
                    .on_click(cx.listener(|app, _, _, cx| app.open_run_dialog(None, cx))),
            )
        })
        .into_any_element()
}

/// A card's 40px header row (it drags the window too): (window controls
/// and project while the sidebar is hidden), the section's title and count,
/// the engine (badge, context, picker, stop VM), then the section's
/// `actions`, filter and Refresh.
pub fn card_header(
    app: &FerrumContainers,
    title: &str,
    count: Option<usize>,
    actions: Vec<AnyElement>,
    window: &Window,
    cx: &mut Context<FerrumContainers>,
) -> AnyElement {
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
    let sidebar = app.sidebar_shown();
    let context = match &app.status {
        EngineStatus::Connected(c) => c.candidate.context.clone(),
        _ => None,
    };
    ferrum_ui::title_row("card-header", !sidebar, window)
        .gap_2()
        .pr_2()
        .border_b_1()
        .border_color(theme.colors().border_variant)
        .when(!sidebar, |row| {
            row.child(title_controls(app, cx)).child(
                div()
                    .flex_none()
                    .max_w(px(260.))
                    .child(app.project_bar.clone()),
            )
        })
        .when(sidebar, |row| row.pl_3())
        .child(
            Label::new(title.to_owned())
                .weight(gpui::FontWeight::SEMIBOLD)
                .into_any_element(),
        )
        .when_some(count, |row, n| {
            row.child(Label::new(n.to_string()).color(Color::Muted))
        })
        .child(div().w(px(4.)))
        .child(engine_badge)
        .when_some(context, |bar, name| bar.child(muted(name)))
        .when(!matches!(app.status, EngineStatus::Connecting), |bar| {
            bar.child(
                ferrum_ui::title_button("pick-engine", IconName::Server, "Choose engine…")
                    .on_click(cx.listener(|app, _, _, cx| app.open_engine_picker(cx))),
            )
        })
        .when_some(app.connected_vm(), |bar, vm| {
            bar.child(
                ferrum_ui::title_button(
                    "stop-vm",
                    IconName::Stop,
                    format!("Stop {} (fec stop)", vm.name()),
                )
                .on_click(cx.listener(move |app, _, _, cx| app.stop_vm(vm, None, cx))),
            )
        })
        .child(div().flex_1())
        .children(actions)
        .when(connected, |bar| {
            bar.child(
                h_flex()
                    .flex_none()
                    .w(px(200.))
                    .h(px(26.))
                    .px_2()
                    .gap_1p5()
                    .rounded(px(7.))
                    .bg(ferrum_ui::hover_fill(cx))
                    .child(
                        Icon::new(IconName::MagnifyingGlass)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(div().flex_1().min_w_0().child(app.inputs.filter.clone())),
            )
            .child(
                ferrum_ui::title_button("refresh", IconName::RotateCw, "Refresh (⌘R)")
                    .on_click(cx.listener(|app, _, _, cx| app.refresh(app.state.section, cx))),
            )
        })
        // The Ferrum account (Sign in / avatar) at the top right; in the
        // detail card's header while that card is open.
        .when(!app.detail_open(), |bar| {
            bar.children(ferrum_account_ui::account_button(cx))
        })
        .into_any_element()
}

/// The sidebar, on the window backdrop: title row (traffic lights, ◧ ⌘ +),
/// project, then the engine's sections.
pub fn sidebar(
    app: &FerrumContainers,
    window: &Window,
    cx: &mut Context<FerrumContainers>,
) -> AnyElement {
    let items = Section::ALL.into_iter().map(|section| {
        let (icon, count) = match section {
            Section::Containers => (IconName::Box, app.state.containers.len()),
            Section::Images => (IconName::Server, app.state.images.len()),
            Section::Volumes => (IconName::Folder, app.state.volumes.len()),
            Section::Networks => (IconName::Link, app.state.networks.len()),
        };
        let selected = app.state.section == section;
        let loaded = app.state.loaded.contains(&section);
        ferrum_ui::sidebar_item(section.title(), selected, cx)
            .child(Icon::new(icon).size(IconSize::Small).color(if selected {
                Color::Default
            } else {
                Color::Muted
            }))
            .child(div().flex_1().child(section.title()))
            .when(loaded, |d| d.child(muted(count.to_string())))
            .on_click(cx.listener(move |app, _, _, cx| app.select_section(section, cx)))
    });
    let items: Vec<_> = items.collect();
    let controls = title_controls(app, cx);
    v_flex()
        .flex_none()
        .w(px(SIDEBAR_WIDTH))
        .h_full()
        .gap_0p5()
        .child(
            ferrum_ui::title_row("sidebar-title", true, window)
                .child(div().flex_1())
                .child(controls),
        )
        .child(div().px_1().child(app.project_bar.clone()))
        .child(ferrum_ui::sidebar_label("Engine", cx))
        .children(items)
        .into_any_element()
}

pub const SIDEBAR_WIDTH: f32 = 248.;

/// The quiet status line at the bottom of the section's card: socket, API,
/// platform, pulls and the running count.
pub fn card_footer(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
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
    let colors = cx.theme().colors();
    h_flex()
        .flex_none()
        .h(px(26.))
        .px_3()
        .gap_3()
        .border_t_1()
        .border_color(colors.border_variant)
        .justify_between()
        .text_xs()
        .text_color(colors.text_muted)
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
        .child(Label::new("Start a VM below (or `fec start` in a terminal).").color(Color::Muted))
        .child(div().max_w(px(560.)).child(muted(detail)))
        .child(
            h_flex()
                .gap_2()
                .children(ferrumdock_engine::vm::installed().into_iter().map(|vm| {
                    button(
                        gpui::SharedString::from(format!("start-{}", vm.key())),
                        format!("Start {}", vm.name()),
                        false,
                        cx,
                        move |app, cx| app.start_vm(vm, cx),
                    )
                }))
                .child(button("retry", "Retry", false, cx, |app, cx| {
                    app.connect(false, cx)
                })),
        )
        .child(
            muted("FerrumContainers also retries on its own every few seconds.")
                .color(Color::Placeholder),
        )
        .into_any_element()
}
