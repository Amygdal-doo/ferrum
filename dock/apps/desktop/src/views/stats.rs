//! Stats tab: CPU and memory sparklines (last ~2 minutes) plus network and
//! block I/O totals and rates.

use crate::app::FerrumContainers;
use crate::views::widgets::muted;
use ferrumdock_engine::stats::{IoRates, StatsHistory, sparkline};
use ferrumdock_engine::{Stats, format};
use gpui::{AnyElement, Context, Hsla, div, prelude::*, px, relative};
use theme::ActiveTheme;
use ui::{Label, LabelCommon, LabelSize, h_flex, v_flex};

const CHART_HEIGHT: f32 = 56.;

/// Bars scaled to the panel width, newest on the right.
fn chart(id: &'static str, heights: &[f32], color: Hsla, background: Hsla) -> AnyElement {
    h_flex()
        .id(id)
        .w_full()
        .h(px(CHART_HEIGHT))
        .items_end()
        .gap_px()
        .p_1()
        .rounded_md()
        .bg(background)
        .children(heights.iter().map(|h| {
            div()
                .flex_1()
                .min_w(px(1.))
                .h(relative(h.max(0.02)))
                .rounded_t_sm()
                .bg(color)
        }))
        .into_any_element()
}

fn rate(bytes_per_sec: f64) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = bytes_per_sec.max(0.0).round() as u64;
    format!("{}/s", format::bytes(whole))
}

fn metric(title: &str, value: String, detail: String) -> AnyElement {
    v_flex()
        .gap_0p5()
        .child(muted(title.to_owned()))
        .child(Label::new(value).size(LabelSize::Small))
        .child(muted(detail))
        .into_any_element()
}

fn io_row(latest: &Stats, rates: Option<IoRates>) -> AnyElement {
    let r = rates.unwrap_or_default();
    h_flex()
        .gap_6()
        .child(metric(
            "Network in",
            format::bytes(latest.net_rx),
            rate(r.net_rx),
        ))
        .child(metric(
            "Network out",
            format::bytes(latest.net_tx),
            rate(r.net_tx),
        ))
        .child(metric(
            "Disk read",
            format::bytes(latest.blk_read),
            rate(r.blk_read),
        ))
        .child(metric(
            "Disk write",
            format::bytes(latest.blk_write),
            rate(r.blk_write),
        ))
        .into_any_element()
}

pub fn render(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let Some(detail) = app.state.detail.as_ref() else {
        return div().into_any_element();
    };
    let history: Option<&StatsHistory> = app.state.history.get(&detail.id);
    let Some((history, latest)) = history.and_then(|h| h.latest().map(|l| (h, *l))) else {
        let running = app
            .state
            .container(&detail.id)
            .is_some_and(|c| c.state.is_up());
        let text = if running {
            "Collecting samples…"
        } else {
            "Stats are sampled while the container runs."
        };
        return ferrum_ui::empty_state(text, "").into_any_element();
    };
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let background = colors.element_background;
    #[allow(clippy::cast_precision_loss)]
    let mem_floor = if latest.mem_limit > 0 {
        latest.mem_limit as f64 / 4.0
    } else {
        1.0
    };
    let cpu = sparkline(&history.cpu(), 10.0);
    let mem = sparkline(&history.memory(), mem_floor);
    v_flex()
        .id("stats-tab")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .p_3()
        .gap_3()
        .child(
            h_flex()
                .gap_2()
                .child(Label::new("CPU").size(LabelSize::Small))
                .child(muted(format::cpu(latest.cpu_percent)))
                .child(div().flex_1())
                .child(muted(format!("{} samples", history.len()))),
        )
        .child(chart("cpu-chart", &cpu, colors.text_accent, background))
        .child(
            h_flex()
                .gap_2()
                .child(Label::new("Memory").size(LabelSize::Small))
                .child(muted(format::memory(latest.mem_usage, latest.mem_limit))),
        )
        .child(chart("mem-chart", &mem, theme.status().success, background))
        .child(io_row(&latest, history.rates()))
        .child(muted(
            "Sampled every 2 s while the Containers list is shown. Totals count since the container started.",
        ))
        .into_any_element()
}
