//! Toolbar, status bar, kill dialog, notices and small shared widgets.

use ferrum_ui::prelude::*;
use ferrum_ui::ui::TintColor;

use crate::app::{FerrumPorts, KillStage};

/// Compact outlined button bound to the root view.
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &mut Context<FerrumPorts>,
    on_click: impl Fn(&mut FerrumPorts, &mut Context<FerrumPorts>) + 'static,
) -> Button {
    Button::new(id, label)
        .style(ButtonStyle::Outlined)
        .size(ButtonSize::Compact)
        .on_click(cx.listener(move |app, _, _, cx| on_click(app, cx)))
}

/// A two-state toolbar button.
fn toggle(
    id: &'static str,
    label: &'static str,
    on: bool,
    cx: &mut Context<FerrumPorts>,
    on_click: impl Fn(&mut FerrumPorts, &mut Context<FerrumPorts>) + 'static,
) -> Button {
    Button::new(id, label)
        .style(ButtonStyle::Subtle)
        .size(ButtonSize::Compact)
        .toggle_state(on)
        .on_click(cx.listener(move |app, _, _, cx| {
            on_click(app, cx);
            cx.notify();
        }))
}

fn field(input: AnyElement, width: f32, cx: &App) -> Div {
    let colors = cx.theme().colors();
    div()
        .w(px(width))
        .px_2()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .bg(colors.editor_background)
        .child(input)
}

pub fn toolbar(app: &FerrumPorts, cx: &mut Context<FerrumPorts>) -> AnyElement {
    let has_project = app.project(cx).is_some();
    let project_tip = if has_project {
        "Only ports of the project in the strip above"
    } else {
        "Pick a project in the strip above first"
    };
    ferrum_ui::toolbar(cx)
        .child(
            Icon::new(IconName::Server)
                .size(IconSize::Small)
                .color(Color::Accent),
        )
        .child(Label::new("FerrumPorts").weight(gpui::FontWeight::SEMIBOLD))
        .child(field(app.search.clone().into_any_element(), 280., cx))
        .child(kbd("⌘F", cx))
        .child(toggle(
            "group",
            "Group by project",
            app.grouped,
            cx,
            |app, _| {
                app.grouped = !app.grouped;
            },
        ))
        .child(
            toggle(
                "only-project",
                "This project",
                app.only_project && has_project,
                cx,
                |app, _| app.only_project = !app.only_project,
            )
            .disabled(!has_project)
            .tooltip(Tooltip::text(project_tip)),
        )
        .child(
            toggle("udp", "UDP", app.udp, cx, |app, cx| app.toggle_udp(cx))
                .tooltip(Tooltip::text("Also list bound UDP sockets")),
        )
        .child(div().flex_1())
        .child(field(app.port_input.clone().into_any_element(), 110., cx))
        .child(button("check-port", "Check", cx, |app, cx| {
            app.check_port(cx)
        }))
        .child(button(
            "pause",
            if app.paused { "Resume" } else { "Pause" },
            cx,
            |app, cx| app.toggle_pause(cx),
        ))
        .child(button("refresh", "Refresh", cx, |app, cx| app.refresh(cx)))
        .into_any_element()
}

pub fn status_bar(app: &FerrumPorts, cx: &mut Context<FerrumPorts>) -> AnyElement {
    let status = cx.theme().status();
    let shown = app
        .rows(cx)
        .iter()
        .filter(|r| matches!(r, ferrumports_core::Row::Listener { .. }))
        .count();
    let mut left = format!("{} listeners", app.listeners.len());
    if shown != app.listeners.len() {
        left.push_str(&format!(" · {shown} shown"));
    }
    let refresh = match (&app.last_refresh, app.paused) {
        (Some(t), true) => format!("Paused · last refresh {}", t.format("%H:%M:%S")),
        (Some(t), false) => format!("Auto-refresh 2 s · last {}", t.format("%H:%M:%S")),
        (None, _) => "Scanning…".to_string(),
    };
    let problem = app.error.clone().or_else(|| app.warnings.first().cloned());
    ferrum_ui::status_bar(cx)
        .child(div().truncate().child(left))
        .when_some(problem, |bar, problem| {
            bar.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(status.error)
                    .child(problem),
            )
        })
        .child(div().flex_none().child(refresh))
        .into_any_element()
}

pub fn kill_dialog(app: &FerrumPorts, cx: &mut Context<FerrumPorts>) -> Option<AnyElement> {
    let dialog = app.dialog.as_ref()?;
    let colors = cx.theme().colors();
    let l = &dialog.listener;
    let pid = l.pid.unwrap_or_default();
    let (title, intro, confirm) = match dialog.stage {
        KillStage::Term => (
            format!("Stop {} on port {}?", l.process, l.port),
            format!("Sends SIGTERM to pid {pid}, asking it to exit cleanly."),
            "Stop (SIGTERM)",
        ),
        KillStage::Force => (
            format!("{} is still running", l.process),
            format!(
                "Pid {pid} did not exit after SIGTERM. SIGKILL ends it at once, without cleanup."
            ),
            "Force kill (SIGKILL)",
        ),
    };
    let close = |app: &mut FerrumPorts, cx: &mut Context<FerrumPorts>| {
        app.dialog = None;
        cx.notify();
    };
    Some(
        Modal::new(title)
            .width(px(560.))
            .on_dismiss(cx.listener(move |app, _, _, cx| close(app, cx)))
            .child(Label::new(intro))
            .child(
                div()
                    .id("kill-command")
                    .max_h(px(140.))
                    .overflow_y_scroll()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.editor_background)
                    .font_buffer(cx)
                    .text_ui_sm(cx)
                    .child(l.command_or_name().to_string()),
            )
            .when_some(l.user.clone(), |modal, user| {
                modal.child(
                    Label::new(format!("Owner: {user}"))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
            })
            .footer(
                Button::new("kill-cancel", "Cancel")
                    .on_click(cx.listener(move |app, _, _, cx| close(app, cx))),
            )
            .footer(
                Button::new(
                    "kill-confirm",
                    if dialog.busy { "Waiting…" } else { confirm },
                )
                .style(ButtonStyle::Tinted(TintColor::Error))
                .disabled(dialog.busy)
                .on_click(cx.listener(|app, _, _, cx| app.confirm_kill(cx))),
            )
            .into_any_element(),
    )
}

pub fn notice(app: &FerrumPorts, cx: &mut Context<FerrumPorts>) -> Option<AnyElement> {
    let notice = app.notice.as_ref()?;
    Some(render_notice(
        notice,
        cx.listener(|app, _, _, cx| {
            app.notice = None;
            cx.notify();
        }),
        cx,
    ))
}
