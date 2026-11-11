//! Containers list grouped by compose project, with live CPU / memory.

use crate::app::FerrumDock;
use crate::state::{Row, container_rows};
use crate::views::compose_rows;
use crate::views::widgets::{
    button, cell, grow_cell, header_row, icon_button, muted, row, section_bar, state_dot,
};
use ferrumdock_engine::{Container, ContainerAction, ContainerState, format, ops};
use gpui::{AnyElement, App, Context, Hsla, SharedString, div, prelude::*, px};
use theme::ActiveTheme;
use ui::{
    Button, ButtonCommon, ButtonSize, Clickable, Color, IconName, Label, LabelCommon, LabelSize,
    h_flex, v_flex,
};

const COLUMNS: [(&str, Option<f32>); 7] = [
    ("NAME", None),
    ("IMAGE", Some(180.)),
    ("STATUS", Some(150.)),
    ("PORTS", Some(150.)),
    ("CPU", Some(64.)),
    ("MEMORY", Some(140.)),
    ("", Some(150.)),
];

pub fn state_color(state: ContainerState, cx: &App) -> Hsla {
    let status = cx.theme().status();
    match state {
        ContainerState::Running => status.success,
        ContainerState::Paused | ContainerState::Restarting => status.warning,
        ContainerState::Dead => status.error,
        _ => cx.theme().colors().text_disabled,
    }
}

pub fn render(app: &FerrumDock, cx: &mut Context<FerrumDock>) -> AnyElement {
    let rows = container_rows(
        &app.state.containers,
        &app.state.filter,
        &app.state.collapsed,
        app.state.project.as_ref(),
    );
    let body: AnyElement = if !app
        .state
        .loaded
        .contains(&crate::state::Section::Containers)
    {
        ferrum_ui::empty_state("Loading containers…", "").into_any_element()
    } else if app.state.containers.is_empty() && rows.is_empty() {
        ferrum_ui::empty_state(
            "No containers",
            "Run one with `docker run` or `docker compose up`.",
        )
        .into_any_element()
    } else if rows.is_empty() {
        ferrum_ui::empty_state("No matches", "Nothing matches the filter.").into_any_element()
    } else {
        let elements: Vec<AnyElement> = rows
            .into_iter()
            .map(|r| match r {
                Row::Group {
                    project,
                    running,
                    total,
                    collapsed,
                    compose,
                } => compose_rows::group_row(app, &project, running, total, collapsed, compose, cx),
                Row::Container { container, grouped } => {
                    container_row(app, &container, grouped, cx)
                }
                Row::ComposeFile(file) => compose_rows::file_row(app, &file, cx),
            })
            .collect();
        v_flex()
            .id("containers-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(elements)
            .into_any_element()
    };
    let mut actions: Vec<AnyElement> = Vec::new();
    if let Some(project) = app.state.project.as_ref() {
        let name = project
            .root
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let files = project.files.len();
        actions.push(
            muted(format!(
                "{} in {name} · {files} compose file{}",
                project.count(&app.state.containers),
                if files == 1 { "" } else { "s" }
            ))
            .into_any_element(),
        );
        let label = if project.only {
            "Show all"
        } else {
            "This project only"
        };
        actions.push(
            button("project-scope", label, false, cx, |app, cx| {
                app.toggle_project_scope(cx)
            })
            .into_any_element(),
        );
    }
    actions.push(
        button("run-container", "Run…", false, cx, |app, cx| {
            app.open_run_dialog(None, cx)
        })
        .into_any_element(),
    );
    v_flex()
        .size_full()
        .child(section_bar(
            "Containers",
            app.state.containers.len(),
            actions,
            cx,
        ))
        .child(header_row(&COLUMNS, cx))
        .child(body)
        .into_any_element()
}

fn container_row(
    app: &FerrumDock,
    c: &Container,
    grouped: bool,
    cx: &mut Context<FerrumDock>,
) -> AnyElement {
    let selected = app.state.detail.as_ref().is_some_and(|d| d.id == c.id);
    let busy = app.state.busy.contains(&c.id);
    let stats = app.state.stats.get(&c.id).filter(|_| c.state.is_up());
    let id = c.id.clone();
    let title = c
        .service()
        .map_or_else(|| c.name.clone(), |s| format!("{s}  ·  {}", c.name));
    let name = h_flex()
        .gap_2()
        .when(grouped, |d| d.pl_5())
        .child(state_dot(state_color(c.state, cx)))
        .child(Label::new(title).size(LabelSize::Small).truncate())
        .child(muted(c.short_id().to_owned()));
    row(
        SharedString::from(format!("container-{}", c.id)),
        selected,
        cx,
    )
    .cursor_pointer()
    .child(grow_cell(name))
    .child(cell(180., c.image.clone(), Color::Muted))
    .child(cell(150., c.status.clone(), Color::Default))
    .child(ports_cell(c, cx))
    .child(cell(
        64.,
        stats.map_or_else(|| "–".to_owned(), |s| format::cpu(s.cpu_percent)),
        Color::Default,
    ))
    .child(cell(
        140.,
        stats.map_or_else(
            || "–".to_owned(),
            |s| format::memory(s.mem_usage, s.mem_limit),
        ),
        Color::Default,
    ))
    .child(actions(c, busy, cx))
    .on_click(cx.listener(move |app, _, _, cx| app.select_container(&id, cx)))
    .into_any_element()
}

/// Published ports as clickable `:8080` links (opens the browser), else the exposed ports.
fn ports_cell(c: &Container, cx: &mut Context<FerrumDock>) -> AnyElement {
    let urls = format::browser_urls(&c.ports);
    if urls.is_empty() {
        return cell(150., format::ports(&c.ports), Color::Muted).into_any_element();
    }
    let links = urls.into_iter().take(3).enumerate().map(|(i, url)| {
        let port = url.rsplit(':').next().unwrap_or_default().to_owned();
        let open = url.clone();
        Button::new(
            SharedString::from(format!("port-{}-{i}", c.id)),
            format!(":{port}"),
        )
        .size(ButtonSize::Compact)
        .color(Color::Accent)
        .tooltip(ui::Tooltip::text(format!("Open {url}")))
        .on_click(cx.listener(move |app, _, _, cx| {
            cx.stop_propagation();
            app.open_url(&open, cx);
        }))
    });
    div()
        .flex_none()
        .w(px(150.))
        .overflow_hidden()
        .child(h_flex().gap_0p5().children(links))
        .into_any_element()
}

fn actions(c: &Container, busy: bool, cx: &mut Context<FerrumDock>) -> AnyElement {
    let mut buttons: Vec<AnyElement> = Vec::new();
    for action in ops::lifecycle_actions(c.state) {
        let (icon, tip) = match action {
            ContainerAction::Start => (IconName::PlayFilled, "Start"),
            ContainerAction::Stop => (IconName::Stop, "Stop"),
            ContainerAction::Restart => (IconName::RotateCw, "Restart"),
            ContainerAction::Pause => (IconName::DebugPause, "Pause"),
            ContainerAction::Unpause => (IconName::DebugContinue, "Resume"),
            ContainerAction::Remove { .. } => continue,
        };
        let target = c.clone();
        buttons.push(
            icon_button(
                SharedString::from(format!("{tip}-{}", c.id)),
                icon,
                tip,
                busy,
                cx,
                move |app, cx| {
                    app.container_action(&target, action, cx);
                },
            )
            .into_any_element(),
        );
    }
    let shell_target = c.clone();
    buttons.push(
        icon_button(
            SharedString::from(format!("shell-{}", c.id)),
            IconName::Terminal,
            "Open shell",
            !c.state.is_up(),
            cx,
            move |app, cx| {
                app.open_shell(&shell_target, cx);
            },
        )
        .into_any_element(),
    );
    let remove_target = c.clone();
    buttons.push(
        icon_button(
            SharedString::from(format!("remove-{}", c.id)),
            IconName::Trash,
            "Remove",
            busy,
            cx,
            move |app, cx| {
                app.confirm_remove_container(&remove_target, cx);
            },
        )
        .into_any_element(),
    );
    h_flex()
        .flex_none()
        .w(px(150.))
        .gap_0p5()
        .justify_end()
        .children(buttons)
        .into_any_element()
}
