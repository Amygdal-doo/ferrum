//! Container detail panel: live logs and inspect.

use crate::app::FerrumDock;
use crate::state::{DetailState, DetailTab};
use crate::views::containers::state_color;
use crate::views::widgets::{icon_button, muted, state_dot};
use ferrumdock_engine::LogStream;
use ferrumdock_engine::ansi::{self, AnsiColor};
use ferrumdock_engine::logs::display_timestamp;
use gpui::{
    AnyElement, App, Context, FontWeight, HighlightStyle, Hsla, ListSizingBehavior, StyledText,
    Window, div, prelude::*, px, uniform_list,
};
use std::ops::Range;
use theme::ActiveTheme;
use ui::{
    Checkbox, IconName, Label, LabelCommon, LabelSize, StyledTypography, Tab, TabBar, ToggleState,
    Toggleable, h_flex, v_flex,
};

pub fn render(
    app: &mut FerrumDock,
    window: &mut Window,
    cx: &mut Context<FerrumDock>,
) -> AnyElement {
    let Some(detail) = app.state.detail.as_ref() else {
        return div().into_any_element();
    };
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let container = app.state.container(&detail.id).cloned();
    let title = container
        .as_ref()
        .map_or_else(|| detail.id.clone(), |c| c.name.clone());
    let header = h_flex()
        .flex_none()
        .h(px(40.))
        .px_3()
        .gap_2()
        .border_b_1()
        .border_color(colors.border)
        .when_some(container.as_ref(), |d, c| {
            d.child(state_dot(state_color(c.state, cx)))
        })
        .child(Label::new(title).weight(FontWeight::SEMIBOLD).truncate())
        .when_some(container.as_ref(), |d, c| d.child(muted(c.status.clone())))
        .child(div().flex_1())
        .when_some(container.clone(), |d, c| {
            let up = c.state.is_up();
            d.child(icon_button(
                "detail-shell",
                IconName::Terminal,
                "Open shell",
                !up,
                cx,
                move |app, cx| {
                    app.open_shell(&c, cx);
                },
            ))
        })
        .child(icon_button(
            "detail-close",
            IconName::Close,
            "Close (Esc)",
            false,
            cx,
            |app, cx| app.close_detail(cx),
        ));

    let tab = detail.tab;
    let tabs = TabBar::new("detail-tabs")
        .child(tab_button("Logs", DetailTab::Logs, tab, cx))
        .child(tab_button("Inspect", DetailTab::Inspect, tab, cx));
    let content = match tab {
        DetailTab::Logs => logs(app, window, cx),
        DetailTab::Inspect => super::inspect::render(app, cx),
    };
    v_flex()
        .size_full()
        .bg(colors.panel_background)
        .child(header)
        .child(tabs)
        .child(content)
        .into_any_element()
}

fn tab_button(
    label: &'static str,
    tab: DetailTab,
    current: DetailTab,
    cx: &mut Context<FerrumDock>,
) -> Tab {
    Tab::new(label)
        .toggle_state(tab == current)
        .child(Label::new(label).size(LabelSize::Small))
        .on_click(cx.listener(move |app, _, _, cx| app.set_detail_tab(tab, cx)))
}

fn checkbox(
    id: &'static str,
    label: &'static str,
    checked: bool,
    cx: &mut Context<FerrumDock>,
    toggle: impl Fn(&mut DetailState) + 'static,
) -> Checkbox {
    Checkbox::new(id, ToggleState::from(checked))
        .label(label)
        .label_size(LabelSize::Small)
        .on_click(cx.listener(move |app, _: &ToggleState, _, cx| app.update_detail(&toggle, cx)))
}

fn logs(app: &FerrumDock, _window: &mut Window, cx: &mut Context<FerrumDock>) -> AnyElement {
    let Some(detail) = app.state.detail.as_ref() else {
        return div().into_any_element();
    };
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let count = detail.visible_count();
    let summary = match (&detail.matches, detail.streaming) {
        (Some(m), _) => format!("{} of {} lines", m.len(), detail.logs.len()),
        (None, true) => format!("{} lines · live", detail.logs.len()),
        (None, false) => format!("{} lines", detail.logs.len()),
    };
    let follow = detail.follow;
    let controls = h_flex()
        .flex_none()
        .px_3()
        .py_1()
        .gap_3()
        .border_b_1()
        .border_color(colors.border_variant)
        .child(
            div()
                .flex_1()
                .min_w(px(120.))
                .px_2()
                .rounded_md()
                .border_1()
                .border_color(colors.border)
                .bg(colors.editor_background)
                .child(app.inputs.log_search.clone()),
        )
        .child(
            Checkbox::new("log-follow", ToggleState::from(follow))
                .label("Follow")
                .label_size(LabelSize::Small)
                .on_click(cx.listener(|app, _: &ToggleState, _, cx| app.toggle_follow(cx))),
        )
        .child(checkbox(
            "log-ts",
            "Timestamps",
            detail.timestamps,
            cx,
            |d| d.timestamps = !d.timestamps,
        ))
        .child(checkbox("log-colors", "Colours", detail.colors, cx, |d| {
            d.colors = !d.colors
        }))
        .child(muted(summary));

    let body: AnyElement = if count == 0 {
        let text = match (&detail.log_error, detail.streaming, &detail.matches) {
            (Some(e), _, _) => e.clone(),
            (None, true, None) => "Waiting for output…".to_owned(),
            (None, _, Some(_)) => "No lines match the search.".to_owned(),
            (None, false, None) => "No log output.".to_owned(),
        };
        ferrum_ui::empty_state(text, "").into_any_element()
    } else {
        uniform_list(
            "log-lines",
            count,
            cx.processor(|app, range: Range<usize>, _, cx| {
                let Some(detail) = app.state.detail.as_ref() else {
                    return Vec::new();
                };
                range
                    .filter_map(|row| log_row(detail, row, cx))
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&app.log_scroll)
        .with_sizing_behavior(ListSizingBehavior::Infer)
        .flex_1()
        .into_any_element()
    };
    v_flex()
        .flex_1()
        .min_h_0()
        .child(controls)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .font_buffer(cx)
                .text_buffer(cx)
                .child(body),
        )
        .into_any_element()
}

fn log_row(detail: &DetailState, row: usize, cx: &App) -> Option<AnyElement> {
    let line = detail.logs.lines().get(detail.line_index(row)?)?;
    let theme = cx.theme();
    let colors = theme.colors();
    let marker = if line.stream == LogStream::Stderr {
        theme.status().error
    } else {
        gpui::transparent_black()
    };
    let text: AnyElement = if detail.colors && line.raw.contains('\u{1b}') {
        let styled = ansi::parse(&line.raw);
        let highlights = styled
            .spans
            .iter()
            .map(|(range, style)| {
                let highlight = HighlightStyle {
                    color: style.fg.map(|c| ansi_color(c, cx)),
                    font_weight: style.bold.then_some(FontWeight::BOLD),
                    fade_out: style.dim.then_some(0.4),
                    ..HighlightStyle::default()
                };
                (range.clone(), highlight)
            })
            .collect::<Vec<_>>();
        StyledText::new(styled.text)
            .with_highlights(highlights)
            .into_any_element()
    } else {
        div().child(line.plain.clone()).into_any_element()
    };
    Some(
        h_flex()
            .id(("log", row))
            .w_full()
            .px_3()
            .gap_2()
            .whitespace_nowrap()
            .overflow_hidden()
            .border_l_2()
            .border_color(marker)
            .when_some(
                line.timestamp.as_ref().filter(|_| detail.timestamps),
                |d, ts| {
                    d.child(
                        div()
                            .flex_none()
                            .text_color(colors.text_muted)
                            .child(display_timestamp(ts)),
                    )
                },
            )
            .child(text)
            .into_any_element(),
    )
}

fn ansi_color(color: AnsiColor, cx: &App) -> Hsla {
    let c = cx.theme().colors();
    match color.0 {
        0 => c.terminal_ansi_black,
        1 => c.terminal_ansi_red,
        2 => c.terminal_ansi_green,
        3 => c.terminal_ansi_yellow,
        4 => c.terminal_ansi_blue,
        5 => c.terminal_ansi_magenta,
        6 => c.terminal_ansi_cyan,
        7 => c.terminal_ansi_white,
        8 => c.terminal_ansi_bright_black,
        9 => c.terminal_ansi_bright_red,
        10 => c.terminal_ansi_bright_green,
        11 => c.terminal_ansi_bright_yellow,
        12 => c.terminal_ansi_bright_blue,
        13 => c.terminal_ansi_bright_magenta,
        14 => c.terminal_ansi_bright_cyan,
        _ => c.terminal_ansi_bright_white,
    }
}
