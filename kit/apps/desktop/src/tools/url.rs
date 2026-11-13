//! URL encode/decode and URL parser.

use super::{layout, split};
use crate::widgets::*;
use ferrum_ui::components::Checkbox;
use ferrum_ui::prelude::*;
use ferrumkit_core::url_tool::{self, UrlParts};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Encode,
    Decode,
    Parse,
}

pub struct UrlTool {
    input: Entity<TextInput>,
    output: Entity<TextInput>,
    mode: Mode,
    plus_as_space: bool,
    error: Option<String>,
    parsed: Option<UrlParts>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for UrlTool {}

impl UrlTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor("Text or URL…", 8, window, cx);
        let output = output(8, window, cx);
        let subscriptions = vec![on_change(&input, cx, Self::run)];
        Self {
            input,
            output,
            mode: Mode::Encode,
            plus_as_space: true,
            error: None,
            parsed: None,
            _subscriptions: subscriptions,
        }
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let input = text_of(&self.input, cx);
        self.error = None;
        self.parsed = None;
        let output = match self.mode {
            Mode::Encode => Ok(url_tool::encode_component(&input)),
            Mode::Decode => url_tool::decode_component(&input, self.plus_as_space),
            Mode::Parse if input.trim().is_empty() => Ok(String::new()),
            Mode::Parse => url_tool::parse_url(&input).map(|parts| {
                self.parsed = Some(parts);
                String::new()
            }),
        };
        match output {
            Ok(text) => set_text(&self.output, text, cx),
            Err(error) => {
                self.error = Some(error);
                set_text(&self.output, "", cx);
            }
        }
    }

    fn render_parts(&self, parts: &UrlParts, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<_> = parts
            .rows()
            .into_iter()
            .map(|(label, value)| {
                value_row(format!("part-{label}"), label, value, cx).into_any_element()
            })
            .collect();
        let params: Vec<_> = parts
            .params
            .iter()
            .enumerate()
            .map(|(i, (key, value))| {
                value_row(format!("param-{i}"), key.clone(), value.clone(), cx).into_any_element()
            })
            .collect();
        v_flex()
            .gap_1()
            .children(rows)
            .child(div().pt_2().child(caption(format!(
                "Query parameters ({}, decoded)",
                params.len()
            ))))
            .when(params.is_empty(), |this| {
                this.child(hint("No query parameters"))
            })
            .children(params)
    }
}

impl Render for UrlTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = choice(
            "url-mode",
            &[
                (Mode::Encode, "Encode"),
                (Mode::Decode, "Decode"),
                (Mode::Parse, "Parse URL"),
            ],
            self.mode,
            |this: &mut Self, mode, _, cx| {
                this.mode = mode;
                this.run(cx);
            },
            cx,
        );
        let plus = Checkbox::new(
            "url-plus",
            if self.plus_as_space {
                ToggleState::Selected
            } else {
                ToggleState::Unselected
            },
        )
        .label("Treat + as space")
        .on_click(cx.listener(|this, state: &ToggleState, _, cx| {
            this.plus_as_space = *state == ToggleState::Selected;
            this.run(cx);
            cx.notify();
        }));
        let explanation = match self.mode {
            Mode::Encode => "Percent-encodes like encodeURIComponent.",
            Mode::Decode => "Decodes %XX escapes.",
            Mode::Parse => "Splits an absolute URL into its parts.",
        };
        let controls = controls()
            .child(mode)
            .when(self.mode == Mode::Decode, |this| this.child(plus))
            .child(hint(explanation));
        let body = match (&self.mode, &self.parsed) {
            (Mode::Parse, parsed) => {
                let parts = parsed
                    .clone()
                    .map(|p| self.render_parts(&p, cx).into_any_element())
                    .unwrap_or_else(|| hint("Paste a URL on the left").into_any_element());
                split(
                    panel("url-in", "URL", None, self.input.clone()),
                    panel("url-parts", "Parts", None, parts),
                )
                .into_any_element()
            }
            _ => {
                let output_text = text_of(&self.output, cx);
                split(
                    panel("url-in", "Input", None, self.input.clone()),
                    panel(
                        "url-out",
                        "Result",
                        Some(copy_button("url-copy", output_text, cx).into_any_element()),
                        self.output.clone(),
                    ),
                )
                .into_any_element()
            }
        };
        layout(
            v_flex()
                .gap_2()
                .child(controls)
                .children(self.error.clone().map(|e| status_line(Err(e), cx))),
            body,
        )
    }
}
