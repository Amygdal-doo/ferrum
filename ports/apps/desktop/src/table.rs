//! The listener table: header, project group rows, listener rows.

use ferrum_ui::prelude::*;
use ferrumports_core::{Exposure, Listener, Row};

use crate::app::FerrumPorts;

/// (title, width); `None` is the flexible command column.
const COLUMNS: [(&str, Option<f32>); 7] = [
    ("Port", Some(78.)),
    ("Proto", Some(52.)),
    ("Address", Some(150.)),
    ("Process", Some(130.)),
    ("PID", Some(66.)),
    ("Project / container", Some(140.)),
    ("Command", None),
];
const ACTIONS_WIDTH: f32 = 56.;

fn cell(width: Option<f32>, child: impl IntoElement) -> Div {
    let base = match width {
        Some(w) => div().flex_none().w(px(w)),
        None => div().flex_1().min_w_0(),
    };
    base.overflow_hidden().pr_2().child(child)
}

fn text(text: impl Into<SharedString>, color: Color) -> Label {
    Label::new(text.into())
        .size(LabelSize::Small)
        .color(color)
        .truncate()
}

fn header(cx: &App) -> Div {
    let colors = cx.theme().colors();
    h_flex()
        .flex_none()
        .px_3()
        .py_1()
        .border_b_1()
        .border_color(colors.border_variant)
        .children(
            COLUMNS
                .iter()
                .map(|(title, width)| cell(*width, text(*title, Color::Muted))),
        )
        .child(div().flex_none().w(px(ACTIONS_WIDTH)))
}

pub fn render(app: &FerrumPorts, cx: &mut Context<FerrumPorts>) -> AnyElement {
    let rows = app.rows(cx);
    let body = if !app.loaded {
        let asking = match &app.target {
            Some(host) => format!("Asking {host} over ssh which ports are open"),
            None => "Asking the system which ports are open".to_string(),
        };
        empty_state("Scanning…", asking).into_any_element()
    } else if app.listeners.is_empty() && app.error.is_some() {
        empty_state(
            "Could not list ports",
            app.error.clone().unwrap_or_default(),
        )
        .into_any_element()
    } else if app.listeners.is_empty() {
        empty_state(
            "Nothing is listening",
            "Start a dev server and it shows up here within 2 seconds",
        )
        .into_any_element()
    } else if rows.is_empty() {
        empty_state(
            "No listeners match",
            "Clear the search or the project filter",
        )
        .into_any_element()
    } else {
        div()
            .id("listener-rows")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(rows.into_iter().map(|row| match row {
                Row::Group { name, summary, .. } => group_row(name, summary, cx),
                Row::Listener { index, grouped } => {
                    listener_row(&app.listeners[index], index, grouped, app, cx)
                }
            }))
            .into_any_element()
    };
    v_flex()
        .flex_1()
        .min_w_0()
        .h_full()
        .child(header(cx))
        .child(v_flex().flex_1().min_h_0().child(body))
        .into_any_element()
}

fn group_row(name: String, summary: String, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    h_flex()
        .px_3()
        .py_1()
        .gap_2()
        .bg(colors.panel_background)
        .border_b_1()
        .border_color(colors.border_variant)
        .child(
            Icon::new(IconName::Folder)
                .size(IconSize::Small)
                .color(Color::Muted),
        )
        .child(
            Label::new(name)
                .size(LabelSize::Small)
                .weight(gpui::FontWeight::SEMIBOLD),
        )
        .child(text(format!("— {summary}"), Color::Muted))
        .into_any_element()
}

fn listener_row(
    l: &Listener,
    index: usize,
    grouped: bool,
    app: &FerrumPorts,
    cx: &mut Context<FerrumPorts>,
) -> AnyElement {
    let colors = cx.theme().colors();
    let key = l.key();
    let selected = app.selected == Some(key);
    let address_color = match l.exposure() {
        Exposure::AllInterfaces => Color::Warning,
        _ => Color::Default,
    };
    let port = h_flex()
        .gap_1()
        .child(
            Label::new(l.port.to_string())
                .size(LabelSize::Small)
                .weight(gpui::FontWeight::SEMIBOLD),
        )
        .when_some(l.hint(), |d, hint| d.child(text(hint.short, Color::Muted)));
    let open = l.clone();
    let kill = l.clone();
    // A remote listener's URL is not reachable as localhost.
    let local = app.target.is_none();
    h_flex()
        .id(("listener", index))
        .px_3()
        .when(grouped, |d| d.pl_6())
        .py_1()
        .cursor_pointer()
        .border_b_1()
        .border_color(colors.border_variant)
        .hover(|s| s.bg(colors.element_hover))
        .when(selected, |d| d.bg(colors.element_selected))
        .on_click(cx.listener(move |app, _, _, cx| app.select(key, cx)))
        .child(cell(COLUMNS[0].1, port))
        .child(cell(COLUMNS[1].1, text(l.protocol.label(), Color::Muted)))
        .child(cell(COLUMNS[2].1, text(l.address_label(), address_color)))
        .child(cell(COLUMNS[3].1, text(l.process.clone(), Color::Default)))
        .child(cell(
            COLUMNS[4].1,
            text(
                l.pid.map_or("—".to_string(), |p| p.to_string()),
                Color::Muted,
            ),
        ))
        .child(cell(
            COLUMNS[5].1,
            // A container's port shows the container (a box before its name).
            match &l.container {
                Some(container) => h_flex()
                    .gap_1()
                    .child(
                        Icon::new(IconName::Box)
                            .size(IconSize::XSmall)
                            .color(Color::Info),
                    )
                    .child(text(container.name.clone(), Color::Info))
                    .into_any_element(),
                None => {
                    text(l.project_name().unwrap_or_default(), Color::Accent).into_any_element()
                }
            },
        ))
        .child(cell(
            COLUMNS[6].1,
            text(l.command_or_name().to_string(), Color::Muted),
        ))
        .child(
            h_flex()
                .flex_none()
                .w(px(ACTIONS_WIDTH))
                .justify_end()
                .when(local, |d| {
                    d.child(
                        IconButton::new(("open", index), IconName::ArrowUpRight)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text(format!("Open {}", l.url())))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                cx.stop_propagation();
                                app.open_in_browser(&open, cx);
                            })),
                    )
                })
                .child(
                    IconButton::new(("kill", index), IconName::Trash)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Stop this process…"))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            app.ask_kill(kill.clone(), cx);
                        })),
                ),
        )
        .into_any_element()
}
