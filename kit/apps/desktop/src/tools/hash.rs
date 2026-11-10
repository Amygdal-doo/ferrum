//! MD5 / SHA-1 / SHA-256 / SHA-512 of text.

use super::layout;
use crate::widgets::*;
use ferrum_ui::prelude::*;
use ferrum_ui::ui::Checkbox;
use ferrumkit_core::hash::{HashAlgorithm, hash_all};

pub struct HashTool {
    input: Entity<TextInput>,
    digests: Vec<(HashAlgorithm, String)>,
    uppercase: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for HashTool {}

impl HashTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor("Text to hash (UTF-8)…", 10, window, cx);
        let subscriptions = vec![on_change(&input, cx, Self::run)];
        let mut this = Self {
            input,
            digests: Vec::new(),
            uppercase: false,
            _subscriptions: subscriptions,
        };
        this.run(cx);
        this
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        self.digests = hash_all(&text_of(&self.input, cx));
    }
}

impl Render for HashTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let upper = Checkbox::new(
            "hash-upper",
            if self.uppercase {
                ToggleState::Selected
            } else {
                ToggleState::Unselected
            },
        )
        .label("Uppercase hex")
        .on_click(cx.listener(|this, state: &ToggleState, _, cx| {
            this.uppercase = *state == ToggleState::Selected;
            cx.notify();
        }));
        let rows: Vec<_> = self
            .digests
            .clone()
            .into_iter()
            .map(|(algorithm, digest)| {
                let digest = if self.uppercase {
                    digest.to_uppercase()
                } else {
                    digest
                };
                value_row(algorithm.label(), algorithm.label(), digest, cx).into_any_element()
            })
            .collect();
        layout(
            controls().child(upper).child(hint(
                "Hashes the UTF-8 bytes of the text exactly as typed (no trailing newline added).",
            )),
            v_flex()
                .flex_1()
                .min_h_0()
                .gap_3()
                .child(v_flex().h(px(220.)).flex_none().child(panel(
                    "hash-in",
                    "Input",
                    None,
                    self.input.clone(),
                )))
                .child(v_flex().gap_1().children(rows)),
        )
    }
}
