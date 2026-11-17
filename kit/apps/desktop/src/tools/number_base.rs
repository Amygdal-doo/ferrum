//! Number base converter.

use super::layout;
use crate::widgets::*;
use ferrum_ui::prelude::*;
use ferrumkit_core::number_base::{self, Base};

pub struct NumberBaseTool {
    input: Entity<TextInput>,
    from: Base,
    result: Option<Result<Vec<(Base, String)>, String>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for NumberBaseTool {}

impl NumberBaseTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = line_input("e.g. 255, 0xff, 0b1111_1111", true, window, cx);
        let subscriptions = vec![on_change(&input, cx, Self::run)];
        Self {
            input,
            from: Base::Decimal,
            result: None,
            _subscriptions: subscriptions,
        }
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let input = text_of(&self.input, cx);
        self.result =
            (!input.trim().is_empty()).then(|| number_base::convert_all(&input, self.from));
    }
}

impl Render for NumberBaseTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bases: Vec<(Base, &'static str)> = Base::ALL.iter().map(|b| (*b, b.label())).collect();
        let from = choice(
            "nb-from",
            &bases,
            self.from,
            |this: &mut Self, base, _, cx| {
                this.from = base;
                this.run(cx);
            },
            cx,
        );
        let body = match self.result.clone() {
            None => hint("Enter an integer (up to 128 bits, may be negative).").into_any_element(),
            Some(Err(error)) => status_line(Err(error), cx).into_any_element(),
            Some(Ok(values)) => v_flex()
                .gap_1()
                .children(values.into_iter().map(|(base, value)| {
                    let label = format!("{} (base {})", base.label(), base.radix());
                    // Binary is grouped in nibbles for reading but copied without spaces.
                    let copy = value.replace(' ', "");
                    value_row_with(base.label(), label, value, copy, cx)
                }))
                .into_any_element(),
        };
        layout(
            v_flex()
                .gap_2()
                .child(controls().child(caption("Input base")).child(from))
                .child(self.input.clone()),
            div()
                .id("nb-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(body),
        )
    }
}

impl super::KitTool for NumberBaseTool {
    fn inputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.input.clone()]
    }

    fn rerun(&mut self, cx: &mut Context<Self>) {
        self.run(cx);
        cx.notify();
    }
}
