//! Table card header and footer, kill dialog, notices and small shared widgets.

use ferrum_ui::components::TintColor;
use ferrum_ui::prelude::*;

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

/// A two-state button in the table header.
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
        .label_size(LabelSize::Small)
        .toggle_state(on)
        .on_click(cx.listener(move |app, _, _, cx| {
            on_click(app, cx);
            cx.notify();
        }))
}

/// A small text field in the table header.
fn field(cx: &App) -> Div {
    let colors = cx.theme().colors();
    h_flex()
        .h(px(26.))
        .px_2()
        .gap_1p5()
        .rounded(px(7.))
        .bg(colors.text.opacity(0.06))
        .text_ui_sm(cx)
}

/// The table card's header row (no sidebar, so it starts after the traffic
/// lights and drags the window): Settings, project, search, view toggles,
/// then the port check, pause and refresh.
pub fn table_header(
    app: &FerrumPorts,
    window: &Window,
    cx: &mut Context<FerrumPorts>,
) -> AnyElement {
    let has_project = app.project(cx).is_some();
    let project_tip = if has_project {
        "Only ports of the current project"
    } else {
        "Pick a project first"
    };
    ferrum_ui::title_row("table-header", true, window)
        .gap_1()
        .border_b_1()
        .border_color(cx.theme().colors().border_variant)
        // First after the traffic lights, where the other apps put it
        // (after their sidebar toggle; FerrumPorts has no sidebar).
        .child(ferrum_ui::settings_button("open-settings"))
        .child(crate::remote::host_picker(app, cx))
        .child(div().flex_none().w(px(260.)).child(app.project_bar.clone()))
        .child(
            field(cx)
                .flex_1()
                .min_w(px(140.))
                .max_w(px(320.))
                .child(
                    Icon::new(IconName::MagnifyingGlass)
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                )
                .child(div().flex_1().min_w_0().child(app.search.clone())),
        )
        .child(
            toggle("group", "Group", app.grouped, cx, |app, _| {
                app.grouped = !app.grouped;
            })
            .tooltip(Tooltip::text("Group by project")),
        )
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
        .child(
            field(cx)
                .w(px(96.))
                .child(div().flex_1().min_w_0().child(app.port_input.clone())),
        )
        .child(
            ferrum_ui::title_button("check-port", IconName::Check, "Is this port free?")
                .on_click(cx.listener(|app, _, _, cx| app.check_port(cx))),
        )
        .child(
            ferrum_ui::title_button(
                "pause",
                if app.paused {
                    IconName::PlayFilled
                } else {
                    IconName::DebugPause
                },
                if app.paused {
                    "Resume auto-refresh (⇧⌘P)"
                } else {
                    "Pause auto-refresh (⇧⌘P)"
                },
            )
            .on_click(cx.listener(|app, _, _, cx| app.toggle_pause(cx))),
        )
        .child(
            ferrum_ui::title_button("refresh", IconName::ArrowCircle, "Refresh (⌘R)")
                .on_click(cx.listener(|app, _, _, cx| app.refresh(cx))),
        )
        // The Ferrum account (Sign in / avatar) at the top right; in the
        // detail card's header while that card is open.
        .when(app.selected_listener().is_none(), |row| {
            row.children(ferrum_account_ui::account_button(cx))
        })
        .into_any_element()
}

/// The quiet status line at the bottom of the table card.
pub fn table_footer(app: &FerrumPorts, cx: &mut Context<FerrumPorts>) -> AnyElement {
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
    let (machine, every) = match &app.target {
        Some(host) => (format!("{host} (ssh) · "), crate::commands::REMOTE_POLL),
        None => (String::new(), crate::commands::POLL),
    };
    let refresh = match (&app.last_refresh, app.paused) {
        (Some(t), true) => format!("{machine}Paused · last refresh {}", t.format("%H:%M:%S")),
        (Some(t), false) => format!(
            "{machine}Auto-refresh {} s · last {}",
            every.as_secs(),
            t.format("%H:%M:%S")
        ),
        (None, _) => format!("{machine}Scanning…"),
    };
    let problem = app.error.clone().or_else(|| app.warnings.first().cloned());
    let colors = cx.theme().colors();
    h_flex()
        .flex_none()
        .h(px(24.))
        .px_3()
        .gap_3()
        .justify_between()
        .border_t_1()
        .border_color(colors.border_variant)
        .text_ui_xs(cx)
        .text_color(colors.text_muted)
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
    // On a remote machine the signal goes out over ssh.
    let (on, via) = match &dialog.host {
        Some(host) => (format!(" on {}", host.name), " over ssh"),
        None => (String::new(), ""),
    };
    let (title, intro, confirm) = match dialog.stage {
        KillStage::Term => (
            format!("Stop {} on port {}{on}?", l.process, l.port),
            format!("Sends SIGTERM to pid {pid}{on}{via}, asking it to exit cleanly."),
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
