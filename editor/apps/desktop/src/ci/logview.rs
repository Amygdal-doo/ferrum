//! The job log: folding groups, coloured text, highlighted errors and search.

use crate::ci::CiView;
use ferrum_ui::components::{Icon, IconSize};
use ferrum_ui::prelude::*;
use ferrum_ui::theme::ThemeColors;
use ferrumci_core::RunState;
use ferrumci_core::ansi::AnsiColor;
use ferrumci_core::logs::{JobLog, LineKind};
use gpui::{FontWeight, Hsla, Rgba};
use std::collections::HashSet;
use std::sync::Arc;

fn ansi_color(c: AnsiColor, t: &ThemeColors) -> Hsla {
    match c {
        AnsiColor::Rgb(r, g, b) => Rgba {
            r: f32::from(r) / 255.,
            g: f32::from(g) / 255.,
            b: f32::from(b) / 255.,
            a: 1.,
        }
        .into(),
        AnsiColor::Indexed(i) => match i {
            0 => t.terminal_ansi_black,
            1 => t.terminal_ansi_red,
            2 => t.terminal_ansi_green,
            3 => t.terminal_ansi_yellow,
            4 => t.terminal_ansi_blue,
            5 => t.terminal_ansi_magenta,
            6 => t.terminal_ansi_cyan,
            7 => t.terminal_ansi_white,
            8 => t.terminal_ansi_bright_black,
            9 => t.terminal_ansi_bright_red,
            10 => t.terminal_ansi_bright_green,
            11 => t.terminal_ansi_bright_yellow,
            12 => t.terminal_ansi_bright_blue,
            13 => t.terminal_ansi_bright_magenta,
            14 => t.terminal_ansi_bright_cyan,
            _ => t.terminal_ansi_bright_white,
        },
    }
}

fn message(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    div()
        .flex_1()
        .h_full()
        .p_4()
        .text_color(cx.theme().colors().text_muted)
        .child(text.into())
        .into_any_element()
}

pub fn render(app: &mut CiView, cx: &mut Context<CiView>) -> AnyElement {
    let Some(detail) = app.detail.as_ref() else {
        return div().into_any_element();
    };
    let job = detail
        .job
        .and_then(|id| detail.jobs.iter().find(|j| j.id == id));
    let Some(job) = job else {
        return message("Select a job to see its log.", cx);
    };
    let job_url = job.html_url.clone();
    let Some(view) = detail.log.as_ref() else {
        let text = match job.state() {
            RunState::Queued => "The job is queued. Its log appears when it finishes.",
            RunState::InProgress => {
                "The job is running. Its log appears when it finishes (Open on GitHub for the live log)."
            }
            _ => "No log for this job.",
        };
        return v_flex()
            .flex_1()
            .child(message(text, cx))
            .child(
                div().px_4().child(
                    Button::new("open-job-live", "Open on GitHub")
                        .style(ButtonStyle::Outlined)
                        .on_click(move |_, _, cx| cx.open_url(&job_url)),
                ),
            )
            .into_any_element();
    };
    if view.loading {
        return message("Downloading log…", cx);
    }
    if let Some(e) = &view.error {
        return message(format!("Could not load the log: {e}"), cx);
    }
    let Some(log) = view.log.clone() else {
        return message("No log.", cx);
    };
    let count = if view.matches.is_empty() {
        String::new()
    } else {
        format!("{}/{}", view.current + 1, view.matches.len())
    };
    let errors = log.errors().len();
    let toolbar = h_flex()
        .flex_none()
        .gap_1()
        .p_1()
        .border_b_1()
        .border_color(cx.theme().colors().border)
        .child(div().w(px(260.)).child(app.log_search.clone()))
        .child(Label::new(count).size(LabelSize::Small).color(Color::Muted))
        .child(
            IconButton::new("prev-match", IconName::ChevronUp)
                .tooltip(Tooltip::text("Previous match"))
                .on_click(cx.listener(|this, _, _, cx| this.next_match(-1, cx))),
        )
        .child(
            IconButton::new("next-match", IconName::ChevronDown)
                .tooltip(Tooltip::text("Next match (Enter)"))
                .on_click(cx.listener(|this, _, _, cx| this.next_match(1, cx))),
        )
        .child(div().flex_1())
        .when(errors > 0, |el| {
            el.child(
                Label::new(format!("{errors} error lines"))
                    .size(LabelSize::Small)
                    .color(Color::Error),
            )
        })
        .child(
            Button::new("open-job", "Open job on GitHub")
                .style(ButtonStyle::Subtle)
                .on_click(move |_, _, cx| cx.open_url(&job_url)),
        );
    v_flex()
        .flex_1()
        .min_w_0()
        .h_full()
        .child(toolbar)
        .child(render_lines(log, view, cx))
        .into_any_element()
}

fn render_lines(
    log: Arc<JobLog>,
    view: &crate::ci::detail::LogView,
    cx: &mut Context<CiView>,
) -> impl IntoElement {
    let rows = Arc::new(view.visible());
    let collapsed: HashSet<usize> = view.collapsed.clone();
    let matches: HashSet<usize> = view.matches.iter().copied().collect();
    let current = view.matches.get(view.current).copied();
    let colors = cx.theme().colors().clone();
    let status = cx.theme().status().clone();
    let weak = cx.weak_entity();
    let count = rows.len();
    uniform_list("job-log", count, move |range, _, _| {
        range
            .map(|row| {
                let index = rows[row];
                let line = &log.lines[index];
                let group = log.group_at(index);
                let bg = if Some(index) == current {
                    Some(colors.search_active_match_background)
                } else if matches.contains(&index) {
                    Some(colors.search_match_background)
                } else {
                    match line.kind {
                        LineKind::Error => Some(status.error_background),
                        LineKind::Warning => Some(status.warning_background),
                        _ => None,
                    }
                };
                let fg = match line.kind {
                    LineKind::Error => status.error,
                    LineKind::Warning => status.warning,
                    LineKind::Command => colors.text_accent,
                    LineKind::Debug | LineKind::Notice => colors.text_muted,
                    LineKind::Group | LineKind::Plain => colors.text,
                };
                let chevron = group.map(|g| {
                    let icon = if collapsed.contains(&g) {
                        IconName::ChevronRight
                    } else {
                        IconName::ChevronDown
                    };
                    Icon::new(icon).size(IconSize::XSmall).color(Color::Muted)
                });
                let mut text = h_flex().min_w_0().text_color(fg);
                for span in &line.spans {
                    let mut s = div().child(span.text.clone());
                    if let Some(c) = span.fg.filter(|_| line.kind != LineKind::Error) {
                        s = s.text_color(ansi_color(c, &colors));
                    }
                    if span.bold || line.kind == LineKind::Group {
                        s = s.font_weight(FontWeight::BOLD);
                    }
                    text = text.child(s);
                }
                let weak = weak.clone();
                h_flex()
                    .id(("log-line", index))
                    .w_full()
                    .h(px(18.))
                    .px_2()
                    .gap_2()
                    .when_some(bg, |el, bg| el.bg(bg))
                    .when_some(group, |el, g| {
                        el.cursor_pointer().on_click(move |_, _, cx| {
                            weak.update(cx, |this, cx| {
                                if let Some(v) = this.detail.as_mut().and_then(|d| d.log.as_mut()) {
                                    v.toggle_group(g);
                                }
                                cx.notify();
                            })
                            .ok();
                        })
                    })
                    .child(
                        div()
                            .w(px(44.))
                            .flex_none()
                            .text_right()
                            .text_color(colors.editor_line_number)
                            .child(line.number.to_string()),
                    )
                    .child(div().w(px(12.)).flex_none().children(chevron))
                    .child(text)
            })
            .collect()
    })
    .track_scroll(&view.scroll)
    .flex_1()
    .w_full()
    .font_buffer(cx)
    .text_xs()
}
