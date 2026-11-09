//! Collection runner progress: per-request status, time and pass/fail, with
//! Stop while running and Run again afterwards. Closing the dialog keeps the
//! run going (the status bar reopens it).

use super::modal;
use crate::app::FerrumApi;
use crate::components::{hint, method_label, status_color};
use crate::state::runner::{RowStatus, RunRow, RunnerState};
use ferrumapi_application::RunOutcome;
use ferrumapi_domain::text::human_duration_ms;
use gpui::{AnyElement, Context, Hsla, SharedString, div, prelude::*, px};
use ui::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonStyle, Clickable as _, TintColor, Tooltip,
    h_flex, v_flex,
};

/// Short status label and its colour.
fn badge_for(status: &RowStatus, cx: &Context<FerrumApi>) -> (&'static str, Hsla) {
    let colors = cx.theme().colors();
    let s = cx.theme().status();
    match status {
        RowStatus::Pending => ("Pending", colors.text_muted),
        RowStatus::Running => ("Running…", s.info),
        RowStatus::Skipped => ("Skipped", colors.text_muted),
        RowStatus::Done(o) if o.passed() => ("Pass", s.success),
        RowStatus::Done(_) => ("Fail", s.error),
    }
}

fn summary(runner: &RunnerState) -> String {
    let counts = runner.counts();
    let total = runner.rows.len();
    let tally = format!("{} passed, {} failed", counts.passed, counts.failed);
    if runner.running {
        format!(
            "Running {} of {total} · {tally}",
            (counts.done + 1).min(total)
        )
    } else if runner.stopped {
        format!("Stopped after {} of {total} · {tally}", counts.done)
    } else {
        format!(
            "Finished {total} requests · {tally} · {}",
            human_duration_ms(counts.time_ms)
        )
    }
}

impl FerrumApi {
    fn render_run_row(&self, index: usize, row: &RunRow, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let (label, color) = badge_for(&row.status, cx);
        let (code, time, error) = match &row.status {
            RowStatus::Done(RunOutcome::Response {
                status, time_ms, ..
            }) => (
                Some((status.to_string(), status_color(*status, cx))),
                human_duration_ms(*time_ms),
                None,
            ),
            RowStatus::Done(RunOutcome::Error(e)) => (None, String::new(), Some(e.clone())),
            _ => (None, String::new(), None),
        };
        let origin = row.origin;
        h_flex()
            .id(SharedString::from(format!("run-row-{index}")))
            .gap_3()
            .px_2()
            .py_1()
            .rounded_md()
            .items_start()
            .cursor_pointer()
            .hover(|s| s.bg(colors.element_hover))
            .tooltip(Tooltip::text("Open this request in a tab"))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_saved(origin);
                cx.notify();
            }))
            .child(
                div()
                    .w(px(64.))
                    .flex_none()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(color)
                    .child(label),
            )
            .child(
                div()
                    .w(px(56.))
                    .flex_none()
                    .child(method_label(row.method, cx)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().truncate().child(row.name.clone()))
                    .when_some(error, |this, e| {
                        this.child(div().text_xs().text_color(colors.text_muted).child(e))
                    }),
            )
            .child(
                div()
                    .w(px(40.))
                    .flex_none()
                    .text_xs()
                    .when_some(code, |this, (code, color)| {
                        this.text_color(color).child(code)
                    }),
            )
            .child(
                div()
                    .w(px(64.))
                    .flex_none()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .child(time),
            )
            .into_any_element()
    }

    pub(super) fn render_runner_dialog(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(runner) = self.state.runner.clone() else {
            self.state.dialog = None;
            return div().into_any_element();
        };
        let rows: Vec<AnyElement> = runner
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| self.render_run_row(i, row, cx))
            .collect();
        let target = (runner.collection, runner.folder);
        let action = if runner.running {
            Button::new("runner-stop", "Stop")
                .style(ButtonStyle::Tinted(TintColor::Error))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.stop_run();
                    cx.notify();
                }))
        } else {
            Button::new("runner-again", "Run again")
                .style(ButtonStyle::Filled)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.start_run(target.0, target.1, cx);
                    cx.notify();
                }))
        };

        modal(format!("Run “{}”", runner.title), px(720.), cx)
            .child(
                v_flex()
                    .gap_2()
                    .child(div().text_sm().child(summary(&runner)))
                    .child(hint("Requests run one at a time in collection order. A 2xx status passes. Runs use the cookie jar and are not added to history."))
                    .child(v_flex().children(rows)),
            )
            .footer(
                Button::new("runner-close", "Close")
                    .style(ButtonStyle::Subtle)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.dialog = None;
                        cx.notify();
                    })),
            )
            .footer(action)
            .into_any_element()
    }
}
