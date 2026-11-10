//! Case converter: every case at once, line by line.

use super::layout;
use crate::widgets::*;
use ferrum_ui::prelude::*;
use ferrumkit_core::case::{self, Case};

pub struct CaseTool {
    input: Entity<TextInput>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for CaseTool {}

impl CaseTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor(
            "Text or identifiers, one per line (e.g. userAccountID)…",
            6,
            window,
            cx,
        );
        // Rows are computed in render; just re-render on edits.
        let subscriptions = vec![on_change(&input, cx, |_, _| {})];
        Self {
            input,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for CaseTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input = text_of(&self.input, cx);
        let rows: Vec<_> = Case::ALL
            .iter()
            .map(|case| {
                value_row(
                    format!("case-{case:?}"),
                    case.label(),
                    case::convert(&input, *case),
                    cx,
                )
                .into_any_element()
            })
            .collect();
        layout(
            v_flex().h(px(160.)).flex_none().child(panel(
                "case-in",
                "Input",
                None,
                self.input.clone(),
            )),
            div()
                .id("case-rows")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(
                    v_flex()
                        .gap_1()
                        .child(hint(
                            "Words split at spaces, punctuation, camelCase humps and acronyms.",
                        ))
                        .children(rows),
                ),
        )
    }
}
