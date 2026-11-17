//! Unix timestamp ⇄ date.

use super::layout;
use crate::widgets::*;
use chrono::{Local, Utc};
use ferrum_ui::prelude::*;
use ferrumkit_core::timestamp::{self, TimestampInfo, Unit};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Zone {
    Local,
    Utc,
}

pub struct TimestampTool {
    input: Entity<TextInput>,
    unit: Unit,
    /// Zone used to read dates without an offset.
    zone: Zone,
    result: Option<Result<(TimestampInfo, Option<Unit>), String>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for TimestampTool {}

impl TimestampTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = line_input(
            "1700000000, 1700000000000 or 2024-05-01 12:30:00",
            true,
            window,
            cx,
        );
        let subscriptions = vec![on_change(&input, cx, Self::run)];
        let mut this = Self {
            input,
            unit: Unit::Auto,
            zone: Zone::Local,
            result: None,
            _subscriptions: subscriptions,
        };
        this.now(cx);
        this
    }

    fn now(&mut self, cx: &mut Context<Self>) {
        set_text(&self.input, Utc::now().timestamp().to_string(), cx);
        self.run(cx);
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let input = text_of(&self.input, cx);
        if input.trim().is_empty() {
            self.result = None;
            return;
        }
        let parsed = match self.zone {
            Zone::Local => timestamp::parse_any(&input, self.unit, &Local),
            Zone::Utc => timestamp::parse_any(&input, self.unit, &Utc),
        };
        self.result =
            Some(parsed.map(|(date, unit)| (timestamp::describe(date, &Local, Utc::now()), unit)));
    }
}

impl Render for TimestampTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let unit = choice(
            "ts-unit",
            &[
                (Unit::Auto, "Auto"),
                (Unit::Seconds, "Seconds"),
                (Unit::Milliseconds, "Milliseconds"),
            ],
            self.unit,
            |this: &mut Self, unit, _, cx| {
                this.unit = unit;
                this.run(cx);
            },
            cx,
        );
        let zone = choice(
            "ts-zone",
            &[(Zone::Local, "Local"), (Zone::Utc, "UTC")],
            self.zone,
            |this: &mut Self, zone, _, cx| {
                this.zone = zone;
                this.run(cx);
            },
            cx,
        );
        let input_row = h_flex()
            .gap_2()
            .child(div().flex_1().child(self.input.clone()))
            .child(
                Button::new("ts-now", "Now")
                    .start_icon(Icon::new(IconName::Clock))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.now(cx);
                        cx.notify();
                    })),
            );
        let body = match self.result.clone() {
            None => hint("Enter a Unix timestamp or a date.").into_any_element(),
            Some(Err(error)) => status_line(Err(error), cx).into_any_element(),
            Some(Ok((info, unit))) => {
                let read_as = match unit {
                    Some(Unit::Milliseconds) => "Read as milliseconds since 1970-01-01 UTC",
                    Some(_) => "Read as seconds since 1970-01-01 UTC",
                    None => "Read as a date",
                };
                v_flex()
                    .gap_1()
                    .child(hint(read_as))
                    .child(value_row("s", "Unix seconds", info.seconds.to_string(), cx))
                    .child(value_row(
                        "ms",
                        "Unix milliseconds",
                        info.millis.to_string(),
                        cx,
                    ))
                    .child(value_row("utc", "UTC", info.utc, cx))
                    .child(value_row("local", "Local", info.local, cx))
                    .child(value_row("iso", "ISO 8601", info.iso, cx))
                    .child(value_row("rel", "Relative", info.relative, cx))
                    .into_any_element()
            }
        };
        layout(
            v_flex().gap_2().child(input_row).child(
                controls()
                    .child(caption("Timestamp unit"))
                    .child(unit)
                    .child(caption("Dates without offset are"))
                    .child(zone),
            ),
            div()
                .id("ts-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(body),
        )
    }
}

impl super::KitTool for TimestampTool {
    fn inputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.input.clone()]
    }

    fn rerun(&mut self, cx: &mut Context<Self>) {
        self.run(cx);
        cx.notify();
    }
}
