//! Base64 encode/decode of text.

use super::{layout, split};
use crate::widgets::*;
use ferrum_ui::prelude::*;
use ferrumkit_core::base64::{self, Alphabet};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Encode,
    Decode,
}

pub struct Base64Tool {
    input: Entity<TextInput>,
    output: Entity<TextInput>,
    direction: Direction,
    alphabet: Alphabet,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for Base64Tool {}

impl Base64Tool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor("Text to encode…", 16, window, cx);
        let output = output(16, window, cx);
        let subscriptions = vec![on_change(&input, cx, Self::run)];
        Self {
            input,
            output,
            direction: Direction::Encode,
            alphabet: Alphabet::Standard,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let input = text_of(&self.input, cx);
        let result = match self.direction {
            Direction::Encode => Ok(base64::encode(&input, self.alphabet)),
            Direction::Decode if input.trim().is_empty() => Ok(String::new()),
            Direction::Decode => base64::decode(&input, self.alphabet),
        };
        match result {
            Ok(text) => {
                self.error = None;
                set_text(&self.output, text, cx);
            }
            Err(error) => {
                self.error = Some(error);
                set_text(&self.output, "", cx);
            }
        }
    }

    fn set_direction(&mut self, direction: Direction, cx: &mut Context<Self>) {
        if direction == self.direction {
            return;
        }
        // Swap so the previous result becomes the new input.
        let output = text_of(&self.output, cx);
        self.direction = direction;
        let placeholder = match direction {
            Direction::Encode => "Text to encode…",
            Direction::Decode => "Base64 to decode…",
        };
        self.input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, cx);
            if !output.is_empty() {
                input.set_text(output, cx);
            }
        });
        self.run(cx);
    }
}

impl Render for Base64Tool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let output_text = text_of(&self.output, cx);
        let direction = choice(
            "b64-direction",
            &[(Direction::Encode, "Encode"), (Direction::Decode, "Decode")],
            self.direction,
            |this: &mut Self, d, _, cx| this.set_direction(d, cx),
            cx,
        );
        let alphabet = choice(
            "b64-alphabet",
            &[
                (Alphabet::Standard, "Standard"),
                (Alphabet::UrlSafe, "URL-safe"),
            ],
            self.alphabet,
            |this: &mut Self, a, _, cx| {
                this.alphabet = a;
                this.run(cx);
            },
            cx,
        );
        let (in_title, out_title) = match self.direction {
            Direction::Encode => ("Text", "Base64"),
            Direction::Decode => ("Base64", "Text"),
        };
        layout(
            v_flex()
                .gap_2()
                .child(controls().child(direction).child(alphabet).child(hint(
                    "URL-safe output has no padding; decoding accepts either.",
                )))
                .children(self.error.clone().map(|e| status_line(Err(e), cx))),
            split(
                panel("b64-in", in_title, None, self.input.clone()),
                panel(
                    "b64-out",
                    out_title,
                    Some(copy_button("b64-copy", output_text, cx).into_any_element()),
                    self.output.clone(),
                ),
            ),
        )
    }
}

impl super::KitTool for Base64Tool {
    fn inputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.input.clone()]
    }

    fn rerun(&mut self, cx: &mut Context<Self>) {
        self.run(cx);
        cx.notify();
    }
}
