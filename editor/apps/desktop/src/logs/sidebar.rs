//! Left panel: the project's sources (status, counters, start/stop, show
//! or hide, edit), suggestions for a project without sources, and the
//! recent projects.

use crate::logs::widgets::{count_pill, dot, health_color, source_color};
use crate::logs::{LogsEvent, LogsView};
use ferrum_ui::gpui::AnyElement;
use ferrum_ui::prelude::*;
use ferrum_ui::project;
use ferrumlogs_core::{SourceConfig, SourceState};

/// Recent projects listed under the sources.
const RECENT_SHOWN: usize = 12;

pub fn render(app: &LogsView, cx: &mut Context<LogsView>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let mut list = v_flex()
        .id("sidebar-scroll")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .pb_2();
    if let Some(session) = app.session() {
        let hidden = &session.filter().hidden;
        list = list.child(sources_header(session.running_count() > 0, cx));
        for state in &session.sources {
            list = list.child(source_row(state, hidden.contains(&state.slot), cx));
        }
        if session.sources.is_empty() {
            list = list.child(
                div().px_3().py_1().child(
                    Label::new("No sources yet. Add a file, a command or a container.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
            );
        }
        if !app.suggestions.is_empty() {
            list = list.child(suggestions(&app.suggestions, cx));
        }
    } else {
        list = list.child(
            div().px_3().py_2().child(
                Label::new("Open a project folder (⌥⌘O) to see its logs.")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            ),
        );
    }
    list = list.child(projects(app, cx));
    v_flex()
        .id("sidebar")
        .w(px(272.))
        .flex_none()
        .h_full()
        .bg(colors.panel_background)
        .border_r_1()
        .border_color(colors.border)
        .child(list)
        .into_any_element()
}

fn sources_header(any_running: bool, cx: &mut Context<LogsView>) -> impl IntoElement {
    h_flex()
        .pr_2()
        .pt_1()
        .justify_between()
        .child(section_header("Sources"))
        .child(
            h_flex()
                .gap_0p5()
                .child(
                    IconButton::new("start-all", IconName::PlayFilled)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Start all"))
                        .on_click(cx.listener(|app, _, _, cx| app.start_all(cx))),
                )
                .child(
                    IconButton::new("stop-all", IconName::Stop)
                        .icon_size(IconSize::Small)
                        .disabled(!any_running)
                        .tooltip(Tooltip::text("Stop all"))
                        .on_click(cx.listener(|app, _, _, cx| app.stop_all(cx))),
                )
                .child(
                    IconButton::new("add-source", IconName::Plus)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Add source"))
                        .on_click(cx.listener(|app, _, window, cx| {
                            app.open_dialog(None, window, cx);
                        })),
                ),
        )
}

fn source_row(state: &SourceState, hidden: bool, cx: &mut Context<LogsView>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let status = cx.theme().status();
    let id = state.config.id.clone();
    let slot = state.slot;
    let running = state.is_running();
    let element_id = |what: &str| SharedString::from(format!("source-{what}-{id}"));
    let toggle_id = id.clone();
    let edit_id = id.clone();
    let mut detail = format!("{} · {}", state.config.kind.label(), state.status_text());
    let target = state.config.kind.target();
    if !target.is_empty() && target != state.config.name {
        detail = format!("{} · {target}", detail);
    }
    v_flex()
        .id(element_id("row"))
        .group("source-row")
        .mx_1()
        .px_2()
        .py_1()
        .rounded_sm()
        .hover(|s| s.bg(colors.element_hover))
        .when(hidden, |d| d.opacity(0.5))
        .child(
            h_flex()
                .gap_1p5()
                .child(dot(health_color(state.health(), cx)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_sm()
                        .text_color(source_color(slot, cx))
                        .child(state.config.name.clone()),
                )
                .when(state.errors > 0, |d| {
                    d.child(count_pill(state.errors, status.error, cx))
                })
                .when(state.warns > 0, |d| {
                    d.child(count_pill(state.warns, status.warning, cx))
                })
                .child(
                    IconButton::new(
                        element_id("eye"),
                        if hidden {
                            IconName::EyeOff
                        } else {
                            IconName::Eye
                        },
                    )
                    .icon_size(IconSize::XSmall)
                    .tooltip(Tooltip::text(if hidden {
                        "Show its lines"
                    } else {
                        "Hide its lines"
                    }))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.update_filter(cx, |f| {
                            if !f.hidden.remove(&slot) {
                                f.hidden.insert(slot);
                            }
                        });
                    })),
                )
                .child(
                    IconButton::new(
                        element_id("run"),
                        if running {
                            IconName::Stop
                        } else {
                            IconName::PlayFilled
                        },
                    )
                    .icon_size(IconSize::XSmall)
                    .tooltip(Tooltip::text(if running { "Stop" } else { "Start" }))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        if running {
                            app.stop_source(&toggle_id, cx);
                        } else {
                            app.start_source(&toggle_id, cx);
                        }
                    })),
                )
                .child(
                    IconButton::new(element_id("edit"), IconName::Pencil)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text("Edit source"))
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.open_dialog(Some(&edit_id), window, cx);
                        })),
                ),
        )
        .child(
            div().pl(px(13.)).child(
                Label::new(detail)
                    .size(LabelSize::XSmall)
                    .color(if state.health() == ferrumlogs_core::Health::Failed {
                        Color::Error
                    } else {
                        Color::Muted
                    })
                    .truncate(),
            ),
        )
        .into_any_element()
}

fn suggestions(list: &[SourceConfig], cx: &mut Context<LogsView>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let all = list.to_vec();
    v_flex()
        .pt_2()
        .child(
            h_flex()
                .pr_2()
                .justify_between()
                .child(section_header("Suggested"))
                .child(
                    Button::new("add-all-suggestions", "Add all")
                        .style(ButtonStyle::Subtle)
                        .size(ButtonSize::Compact)
                        .label_size(LabelSize::XSmall)
                        .on_click(cx.listener(move |app, _, _, cx| {
                            for config in all.clone() {
                                app.save_source(config, false, cx);
                            }
                        })),
                ),
        )
        .children(list.iter().enumerate().map(|(index, config)| {
            let add = config.clone();
            h_flex()
                .id(("suggestion", index))
                .mx_1()
                .px_2()
                .py_0p5()
                .gap_2()
                .rounded_sm()
                .hover(|s| s.bg(colors.element_hover))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            Label::new(config.name.clone())
                                .size(LabelSize::Small)
                                .truncate(),
                        )
                        .child(
                            Label::new(format!(
                                "{} · {}",
                                config.kind.label(),
                                config
                                    .kind
                                    .command_line()
                                    .unwrap_or_else(|| config.kind.target().to_owned())
                            ))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted)
                            .truncate(),
                        ),
                )
                .child(
                    div()
                        .debug_selector(move || format!("add-suggestion-{index}"))
                        .child(
                            Button::new(("add-suggestion", index), "Add")
                                .style(ButtonStyle::Outlined)
                                .size(ButtonSize::Compact)
                                .label_size(LabelSize::XSmall)
                                .tooltip(Tooltip::text("Add this source and start it"))
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.save_source(add.clone(), true, cx);
                                })),
                        ),
                )
        }))
}

fn projects(app: &LogsView, cx: &mut Context<LogsView>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    // The current project first, then other open ones, then recent ones.
    paths.extend(app.current.clone());
    let mut open: Vec<_> = app.sessions.keys().cloned().collect();
    open.sort();
    paths.extend(open);
    paths.extend(app.recent.iter().map(|p| p.path.clone()));
    let mut seen = std::collections::HashSet::new();
    paths.retain(|p| seen.insert(p.clone()));
    paths.truncate(RECENT_SHOWN);
    v_flex()
        .pt_2()
        .child(section_header("Projects"))
        .children(paths.into_iter().enumerate().map(|(index, path)| {
            let current = app.project_path() == Some(path.as_path());
            let running = app.sessions.get(&path).map_or(0, |s| s.running_count());
            let open_path = path.clone();
            h_flex()
                .id(("project", index))
                .mx_1()
                .px_2()
                .py_0p5()
                .gap_2()
                .rounded_sm()
                .cursor_pointer()
                .when(current, |d| d.bg(colors.element_selected))
                .hover(|s| s.bg(colors.element_hover))
                .tooltip(Tooltip::text(path.display().to_string()))
                .on_click(cx.listener(move |app, _, _, cx| {
                    if app.project_path() != Some(open_path.as_path()) {
                        // The editor opens it, and this page follows.
                        cx.emit(LogsEvent::OpenProject(open_path.clone()));
                    }
                }))
                .child(
                    Icon::new(IconName::Folder)
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    div().flex_1().min_w_0().child(
                        Label::new(project::project_name(&path))
                            .size(LabelSize::Small)
                            .truncate(),
                    ),
                )
                .when(running > 0, |d| {
                    d.child(
                        h_flex()
                            .gap_1()
                            .child(dot(cx.theme().status().success))
                            .child(
                                Label::new(running.to_string())
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            ),
                    )
                })
        }))
}
