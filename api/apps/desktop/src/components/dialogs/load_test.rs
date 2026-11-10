//! Load test of a tab's request: requests / concurrency / duration, Start
//! and Cancel, live progress, status counts, latency percentiles and a
//! latency histogram.

use super::modal;
use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change};
use crate::components::{hint, status_color};
use crate::state::tools::LoadTestState;
use ferrumapi_application::LoadEnd;
use ferrumapi_domain::load::{
    LoadTally, MAX_LOAD_CONCURRENCY, MAX_LOAD_DURATION_SECS, MAX_LOAD_REQUESTS, format_latency_us,
    histogram,
};
use ferrumapi_domain::text::human_duration_ms;
use gpui::{AnyElement, Context, Window, div, prelude::*, px, relative};
use ui::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonStyle, Clickable as _, Label,
    LabelCommon as _, LabelSize, StyledTypography as _, TintColor, h_flex, v_flex,
};

const HISTOGRAM_BUCKETS: usize = 12;

fn stat(label: &'static str, value: String) -> impl IntoElement {
    v_flex()
        .min_w(px(84.))
        .child(
            Label::new(label)
                .size(LabelSize::XSmall)
                .color(ui::Color::Muted),
        )
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(value),
        )
}

fn summary(load: &LoadTestState, tally: &LoadTally) -> String {
    let total = load.config.map_or(0, |c| c.total);
    let done = tally.completed();
    let elapsed = human_duration_ms(tally.elapsed_ms);
    match load.end {
        None => format!("Running… {done} / {total} · {elapsed}"),
        Some(LoadEnd::Completed) => format!("Finished {done} requests in {elapsed}"),
        Some(LoadEnd::Cancelled) => format!("Cancelled after {done} / {total} · {elapsed}"),
        Some(LoadEnd::TimeLimit) => {
            format!("Duration limit reached: {done} / {total} in {elapsed}")
        }
    }
}

impl FerrumApi {
    fn render_load_results(
        &self,
        load: &LoadTestState,
        tally: &LoadTally,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let status = cx.theme().status().clone();
        let mut codes: Vec<AnyElement> = tally
            .by_status
            .iter()
            .map(|(code, n)| {
                div()
                    .px_2()
                    .rounded_sm()
                    .bg(colors.element_background)
                    .text_xs()
                    .text_color(status_color(*code, cx))
                    .child(format!("{code} × {n}"))
                    .into_any_element()
            })
            .collect();
        codes.extend(tally.by_error.iter().map(|(kind, n)| {
            div()
                .px_2()
                .rounded_sm()
                .bg(status.error_background)
                .text_xs()
                .text_color(status.error)
                .child(format!("{kind} × {n}"))
                .into_any_element()
        }));
        let bar = div()
            .h(px(6.))
            .w_full()
            .rounded_sm()
            .bg(colors.element_background)
            .child(
                div()
                    .h_full()
                    .rounded_sm()
                    .bg(status.info)
                    .w(relative(load.progress())),
            );

        let mut out = v_flex()
            .gap_3()
            .child(div().text_sm().child(summary(load, tally)))
            .child(bar)
            .child(
                h_flex()
                    .gap_4()
                    .child(stat(
                        "Requests/s",
                        format!("{:.1}", tally.requests_per_sec()),
                    ))
                    .child(stat("OK (< 400)", tally.ok.to_string()))
                    .child(stat("HTTP errors", tally.http_errors.to_string()))
                    .child(stat("Failed", tally.failures.to_string())),
            )
            .child(h_flex().gap_2().flex_wrap().children(codes));
        if let Some(l) = tally.latency() {
            out = out.child(
                h_flex()
                    .gap_4()
                    .child(stat("Min", format_latency_us(l.min)))
                    .child(stat("Avg", format_latency_us(l.avg)))
                    .child(stat("p50", format_latency_us(l.p50)))
                    .child(stat("p90", format_latency_us(l.p90)))
                    .child(stat("p99", format_latency_us(l.p99)))
                    .child(stat("Max", format_latency_us(l.max))),
            );
            let buckets = histogram(&tally.latencies_us, HISTOGRAM_BUCKETS);
            let peak = buckets.iter().map(|b| b.count).max().unwrap_or(1).max(1);
            let rows = buckets.into_iter().map(|b| {
                h_flex()
                    .gap_2()
                    .text_xs()
                    .font_buffer(cx)
                    .child(
                        div()
                            .w(px(180.))
                            .text_color(colors.text_muted)
                            .child(format!(
                                "{} – {}",
                                format_latency_us(b.from),
                                format_latency_us(b.to)
                            )),
                    )
                    .child(
                        div().flex_1().h(px(10.)).child(
                            div()
                                .h_full()
                                .rounded_sm()
                                .bg(status.info)
                                .w(relative(b.count as f32 / peak as f32)),
                        ),
                    )
                    .child(div().w(px(48.)).child(b.count.to_string()))
            });
            out = out
                .child(Label::new("Latency histogram").size(LabelSize::Small))
                .child(v_flex().gap_0p5().children(rows));
        }
        out.into_any_element()
    }

    pub(super) fn render_load_test_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(load) = self.state.load_test.clone() else {
            self.state.dialog = None;
            return div().into_any_element();
        };
        let status = cx.theme().status().clone();
        let text_field = |key: &'static str, value: String, placeholder: &'static str| {
            FieldSpec::new(key).value(value).placeholder(placeholder)
        };
        let total = self.field(
            text_field("load-total", load.total_text.clone(), "100"),
            on_change(|this, text, _| {
                if let Some(l) = this.state.load_test.as_mut() {
                    l.total_text = text;
                }
            }),
            window,
            cx,
        );
        let concurrency = self.field(
            text_field("load-concurrency", load.concurrency_text.clone(), "10"),
            on_change(|this, text, _| {
                if let Some(l) = this.state.load_test.as_mut() {
                    l.concurrency_text = text;
                }
            }),
            window,
            cx,
        );
        let duration = self.field(
            text_field("load-duration", load.duration_text.clone(), "none"),
            on_change(|this, text, _| {
                if let Some(l) = this.state.load_test.as_mut() {
                    l.duration_text = text;
                }
            }),
            window,
            cx,
        );
        let input = |label: &'static str, field| {
            h_flex()
                .gap_2()
                .child(Label::new(label).size(LabelSize::Small))
                .child(div().w(px(80.)).child(field))
        };
        let action = if load.running {
            Button::new("load-cancel", "Cancel")
                .style(ButtonStyle::Tinted(TintColor::Error))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.stop_load_test();
                    cx.notify();
                }))
        } else {
            let label = if load.warning.is_some() {
                "Start anyway"
            } else {
                "Start"
            };
            Button::new("load-start", label)
                .style(ButtonStyle::Filled)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.start_load_test(cx);
                    cx.notify();
                }))
        };
        let results = load
            .tally
            .as_ref()
            .map(|tally| self.render_load_results(&load, tally, cx));

        modal(format!("Load test — {}", load.title), px(760.), cx)
            .child(
                h_flex()
                    .gap_4()
                    .child(input("Requests", total))
                    .child(input("Concurrency", concurrency))
                    .child(input("Duration (s)", duration)),
            )
            .child(hint(format!(
                "Sends the request as it is now in the tab (auth, variables of the active environment), without cookies or history. Limits: {MAX_LOAD_REQUESTS} requests, {MAX_LOAD_CONCURRENCY} concurrent, {MAX_LOAD_DURATION_SECS} s. A duration stops new requests early; leave it empty for no limit."
            )))
            .when_some(load.warning.clone(), |this, warning| {
                this.child(
                    div()
                        .p_2()
                        .rounded_md()
                        .bg(status.warning_background)
                        .text_color(status.warning)
                        .text_sm()
                        .child(warning),
                )
            })
            .when_some(load.error.clone(), |this, e| {
                this.child(div().text_sm().text_color(status.error).child(e))
            })
            .children(results)
            .footer(
                Button::new("load-close", "Close")
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
