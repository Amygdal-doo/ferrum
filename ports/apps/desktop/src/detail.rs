//! Detail pane for the selected listener: full command, cwd, user, the
//! process's other ports, and the row actions.

use ferrum_ui::prelude::*;
use ferrum_ui::project::{FerrumApp, display_path, home_dir};
use ferrumports_core::{Listener, table};

use crate::app::FerrumPorts;
use crate::chrome::button;

const WIDTH: f32 = 380.;

fn field(title: &'static str, value: impl IntoElement) -> Div {
    v_flex()
        .gap_0p5()
        .child(
            Label::new(title)
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        )
        .child(value)
}

fn value(text: impl Into<SharedString>) -> Label {
    Label::new(text.into()).size(LabelSize::Small)
}

pub fn render(app: &FerrumPorts, cx: &mut Context<FerrumPorts>) -> Option<AnyElement> {
    let l = app.selected_listener()?.clone();
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let home = home_dir();
    let show = |path: &std::path::Path| display_path(path, home.as_deref());
    let title = match l.hint() {
        Some(hint) => format!("{} · {}", l.port, hint.description),
        None => format!("{} · {}", l.port, l.process),
    };
    let others: Vec<Listener> = l
        .pid
        .map(|pid| table::ports_of(&app.listeners, pid))
        .unwrap_or_default()
        .into_iter()
        .cloned()
        .collect();

    let url = l.url();
    let mut actions = h_flex().flex_wrap().gap_1();
    {
        let (l1, url1) = (l.clone(), url.clone());
        actions = actions
            .child(button("d-open", "Open in browser", cx, move |app, cx| {
                app.open_in_browser(&l1, cx)
            }))
            .child(button("d-copy-url", "Copy URL", cx, move |app, cx| {
                app.copy(url1.clone(), "URL", cx)
            }));
    }
    if let Some(pid) = l.pid {
        actions = actions.child(button("d-copy-pid", "Copy PID", cx, move |app, cx| {
            app.copy(pid.to_string(), "PID", cx)
        }));
    }
    if let Some(command) = l.command.clone() {
        actions = actions.child(button("d-copy-cmd", "Copy command", cx, move |app, cx| {
            app.copy(command.clone(), "command", cx)
        }));
    }
    if let Some(cwd) = l.cwd.clone() {
        actions = actions.child(button("d-reveal", "Reveal folder", cx, move |app, cx| {
            app.reveal(&cwd, cx)
        }));
    }
    let open_in = l.project.clone().map(|project| {
        h_flex()
            .gap_1()
            .children(FerrumApp::ALL.into_iter().map(|ferrum| {
                let project = project.clone();
                button(
                    SharedString::from(format!("d-open-{}", ferrum.label())),
                    format!("Open in {}", ferrum.label()),
                    cx,
                    move |app, cx| app.open_project_in(ferrum, &project, cx),
                )
            }))
    });
    let kill = l.clone();

    Some(
        v_flex()
            .id("detail")
            .flex_none()
            .w(px(WIDTH))
            .h_full()
            .overflow_y_scroll()
            .p_3()
            .gap_3()
            .bg(colors.panel_background)
            .border_l_1()
            .border_color(colors.border)
            .child(
                h_flex()
                    .justify_between()
                    .child(Headline::new(title).size(HeadlineSize::XSmall))
                    .child(
                        IconButton::new("detail-close", IconName::Close).on_click(cx.listener(
                            |app, _, _, cx| {
                                app.selected = None;
                                cx.notify();
                            },
                        )),
                    ),
            )
            .child(actions)
            .children(open_in)
            .child(field(
                "Command",
                div()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.editor_background)
                    .font_buffer(cx)
                    .text_ui_sm(cx)
                    .child(l.command_or_name().to_string()),
            ))
            .child(field(
                "Process",
                value(match l.pid {
                    Some(pid) => format!("{} · pid {pid}", l.process),
                    None => format!("{} · pid hidden (another user)", l.process),
                }),
            ))
            .child(field(
                "Listening on",
                value(format!(
                    "{} {} · {}",
                    l.protocol.label(),
                    l.address_label(),
                    l.exposure().label()
                )),
            ))
            .when_some(l.cwd.as_deref().map(show), |d, cwd| {
                d.child(field("Working directory", value(cwd)))
            })
            .when_some(l.project.as_deref().map(show), |d, project| {
                d.child(field("Project", value(project).color(Color::Accent)))
            })
            .when_some(l.user.clone(), |d, user| {
                d.child(field("User", value(user)))
            })
            .when_some(l.started, |d, started| {
                d.child(field(
                    "Started",
                    value(started.format("%Y-%m-%d %H:%M:%S").to_string()),
                ))
            })
            .when(others.len() > 1, |d| {
                d.child(field(
                    "All ports of this process",
                    v_flex().gap_0p5().children(others.into_iter().map(|o| {
                        let key = o.key();
                        let selected = key == l.key();
                        div()
                            .id(SharedString::from(format!(
                                "other-{}-{}",
                                o.protocol.label(),
                                o.port
                            )))
                            .cursor_pointer()
                            .child(
                                value(format!(
                                    "{} {} · {}",
                                    o.protocol.label(),
                                    o.port,
                                    o.address_label()
                                ))
                                .color(if selected {
                                    Color::Accent
                                } else {
                                    Color::Default
                                }),
                            )
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.selected = Some(key);
                                cx.notify();
                            }))
                    })),
                ))
            })
            .child(
                Button::new("d-kill", "Stop process…")
                    .style(ButtonStyle::Tinted(ferrum_ui::ui::TintColor::Error))
                    .on_click(cx.listener(move |app, _, _, cx| app.ask_kill(kill.clone(), cx))),
            )
            .into_any_element(),
    )
}
