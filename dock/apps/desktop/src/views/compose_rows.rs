//! Containers list headers: compose project groups (Logs, ps, Up, Restart,
//! Down) and compose files of the open project that have no containers yet.

use crate::app::FerrumContainers;
use crate::state::DetailTab;
use crate::views::widgets::{button, muted};
use ferrum_ui::components::{Color, Icon, IconName, IconSize, Label, LabelCommon, h_flex};
use ferrum_ui::theme::ActiveTheme;
use ferrumdock_engine::compose::{ComposeAction, ComposeProject};
use ferrumdock_engine::project::ComposeFile;
use gpui::{AnyElement, Context, SharedString, div, prelude::*};

pub fn group_row(
    app: &FerrumContainers,
    project: &str,
    running: usize,
    total: usize,
    collapsed: bool,
    compose: Option<ComposeProject>,
    cx: &mut Context<FerrumContainers>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let busy = app.state.busy.contains(&format!("compose:{project}"));
    let name = project.to_owned();
    let mut actions: Vec<AnyElement> = Vec::new();
    if let Some(compose) = compose {
        for (label, tab) in [("Logs", DetailTab::Logs), ("ps", DetailTab::Services)] {
            let compose = compose.clone();
            actions.push(
                button(
                    SharedString::from(format!("compose-{label}-{project}")),
                    label,
                    false,
                    cx,
                    move |app, cx| app.open_project_panel(compose.clone(), tab, cx),
                )
                .into_any_element(),
            );
        }
        for (label, action) in [
            ("Up", ComposeAction::Up),
            ("Restart", ComposeAction::Restart),
            ("Down", ComposeAction::Down),
        ] {
            let compose = compose.clone();
            actions.push(
                button(
                    SharedString::from(format!("compose-{label}-{project}")),
                    label,
                    busy,
                    cx,
                    move |app, cx| {
                        app.compose(compose.clone(), action, cx);
                    },
                )
                .into_any_element(),
            );
        }
    }
    h_flex()
        .id(SharedString::from(format!("group-{project}")))
        .flex_none()
        .px_3()
        .py_1()
        .gap_2()
        .bg(colors.surface_background)
        .border_b_1()
        .border_color(colors.border_variant)
        .cursor_pointer()
        .child(
            Icon::new(if collapsed {
                IconName::ChevronRight
            } else {
                IconName::ChevronDown
            })
            .size(IconSize::Small)
            .color(Color::Muted),
        )
        .child(
            Icon::new(IconName::FileTree)
                .size(IconSize::Small)
                .color(Color::Muted),
        )
        .child(Label::new(project.to_owned()).weight(gpui::FontWeight::SEMIBOLD))
        .child(muted(format!("{running}/{total} running")))
        .when(busy, |d| d.child(muted("working…")))
        .child(div().flex_1())
        .children(actions)
        .on_click(cx.listener(move |app, _, _, cx| app.toggle_group(&name, cx)))
        .into_any_element()
}

/// A compose file of the open project that has no containers: Up creates them.
pub fn file_row(
    app: &FerrumContainers,
    file: &ComposeFile,
    cx: &mut Context<FerrumContainers>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let busy = app
        .state
        .busy
        .contains(&format!("compose:{}", file.project));
    let file_name = file
        .path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let detail = match &file.error {
        Some(error) => error.clone(),
        None if file.services.is_empty() => "no services".to_owned(),
        None => file.services.join(", "),
    };
    let up = file.clone();
    h_flex()
        .id(SharedString::from(format!("file-{}", file.path.display())))
        .flex_none()
        .px_3()
        .py_1()
        .gap_2()
        .bg(colors.surface_background)
        .border_b_1()
        .border_color(colors.border_variant)
        .child(
            Icon::new(IconName::File)
                .size(IconSize::Small)
                .color(Color::Muted),
        )
        .child(Label::new(file.project.clone()).weight(gpui::FontWeight::SEMIBOLD))
        .child(muted(format!("{file_name} · not created")))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .child(muted(detail).truncate()),
        )
        .when(busy, |d| d.child(muted("working…")))
        .child(div().flex_1())
        .child(button(
            SharedString::from(format!("file-up-{}", file.path.display())),
            "Up",
            busy || file.error.is_some(),
            cx,
            move |app, cx| app.compose_up_file(&up, cx),
        ))
        .into_any_element()
}
