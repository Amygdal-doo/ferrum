//! Cron expression explainer with the next run times.

use super::layout;
use crate::widgets::*;
use chrono::{Local, TimeZone, Utc};
use ferrum_ui::prelude::*;
use ferrumkit_core::cron;
use ferrumkit_core::timestamp::relative;

/// Run times listed.
const RUNS: usize = 10;

const EXAMPLES: [(&str, &str); 5] = [
    ("*/5 * * * *", "Every 5 min"),
    ("0 * * * *", "Hourly"),
    ("30 9 * * 1-5", "Weekdays 9:30"),
    ("0 0 * * 0", "Sundays"),
    ("0 3 1 * *", "Monthly"),
];

pub struct CronTool {
    input: Entity<TextInput>,
    result: Option<Result<(String, Vec<chrono::NaiveDateTime>), String>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for CronTool {}

impl CronTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .monospace()
                .placeholder("minute hour day-of-month month day-of-week, e.g. 30 9 * * 1-5")
                .with_text("30 9 * * 1-5")
        });
        let subscriptions = vec![on_change(&input, cx, Self::run)];
        let mut this = Self {
            input,
            result: None,
            _subscriptions: subscriptions,
        };
        this.run(cx);
        this
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let input = text_of(&self.input, cx);
        if input.trim().is_empty() {
            self.result = None;
            return;
        }
        let now = Local::now().naive_local();
        self.result = Some(
            cron::parse(&input)
                .map(|parsed| (cron::explain(&parsed), cron::next_runs(&parsed, now, RUNS))),
        );
    }

    fn render_runs(
        &self,
        runs: &[chrono::NaiveDateTime],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let now = Utc::now();
        runs.iter()
            .enumerate()
            .filter_map(|(i, run)| {
                // A time skipped by a daylight-saving change has no instant.
                let local = Local.from_local_datetime(run).earliest()?;
                let utc = local.with_timezone(&Utc);
                let display = format!(
                    "{}  ·  {}  ({})",
                    local.format("%a %Y-%m-%d %H:%M %:z"),
                    utc.format("%H:%M UTC"),
                    relative(utc, now)
                );
                let label = if i == 0 {
                    "Next".to_string()
                } else {
                    format!("#{}", i + 1)
                };
                Some(value_row_with(
                    format!("cron-run-{i}"),
                    label,
                    display,
                    local.to_rfc3339(),
                    cx,
                ))
            })
            .collect()
    }
}

impl Render for CronTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let examples =
            h_flex()
                .gap_1()
                .child(caption("Examples"))
                .children(EXAMPLES.iter().enumerate().map(|(i, (expression, label))| {
                    let expression = *expression;
                    Button::new(SharedString::from(format!("cron-example-{i}")), *label)
                        .style(ButtonStyle::Subtle)
                        .size(ButtonSize::Compact)
                        .label_size(LabelSize::Small)
                        .tooltip(Tooltip::text(expression))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            set_text(&this.input, expression, cx);
                            this.run(cx);
                            cx.notify();
                        }))
                }));
        let body = match self.result.clone() {
            None => vec![hint(
                "Five fields: minute (0–59), hour (0–23), day of month (1–31), month (1–12 or JAN–DEC), day of week (0–7 or SUN–SAT). Also @hourly, @daily, @weekly, @monthly, @yearly.",
            )],
            Some(Err(error)) => vec![status_line(Err(error), cx)],
            Some(Ok((explanation, runs))) => {
                let mut rows = vec![
                    div().text_lg().child(explanation).into_any_element(),
                    caption(format!(
                        "Next {} runs (local time, {})",
                        runs.len(),
                        Local::now().format("%Z")
                    )),
                ];
                if runs.is_empty() {
                    rows.push(hint("No run in the next eight years."));
                }
                rows.extend(self.render_runs(&runs, cx));
                rows
            }
        };
        layout(
            v_flex()
                .gap_2()
                .child(self.input.clone())
                .child(controls().child(examples)),
            v_flex()
                .id("cron-body")
                .flex_1()
                .min_h_0()
                .gap_1()
                .overflow_y_scroll()
                .children(body),
        )
    }
}

impl super::KitTool for CronTool {
    fn inputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.input.clone()]
    }

    fn rerun(&mut self, cx: &mut Context<Self>) {
        self.run(cx);
        cx.notify();
    }
}
