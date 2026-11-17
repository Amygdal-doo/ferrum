//! JSON format / minify / validate and JSON ⇄ YAML ⇄ TOML conversion.

use super::{layout, split};
use crate::widgets::*;
use ferrum_ui::prelude::*;
use ferrumkit_core::data_format::{self, DataFormat, Indent};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Format,
    Minify,
    Convert,
}

pub struct JsonTool {
    input: Entity<TextInput>,
    output: Entity<TextInput>,
    mode: Mode,
    indent: Indent,
    from: DataFormat,
    to: DataFormat,
    status: Option<Result<String, String>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for JsonTool {}

impl JsonTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor("Paste JSON, YAML or TOML…", 20, window, cx);
        let output = output(20, window, cx);
        let subscriptions = vec![on_change(&input, cx, Self::run)];
        Self {
            input,
            output,
            mode: Mode::Format,
            indent: Indent::Spaces(2),
            from: DataFormat::Json,
            to: DataFormat::Yaml,
            status: None,
            _subscriptions: subscriptions,
        }
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let input = text_of(&self.input, cx);
        if input.trim().is_empty() {
            self.status = None;
            set_text(&self.output, "", cx);
            return;
        }
        let result = match self.mode {
            Mode::Format => data_format::format_json(&input, self.indent),
            Mode::Minify => data_format::minify_json(&input),
            Mode::Convert => data_format::convert(&input, self.from, self.to),
        };
        match result {
            Ok(text) => {
                let source = if self.mode == Mode::Convert {
                    self.from
                } else {
                    DataFormat::Json
                };
                self.status = Some(Ok(format!("Valid {}", source.label())));
                set_text(&self.output, text, cx);
            }
            Err(error) => {
                self.status = Some(Err(error.to_string()));
                set_text(&self.output, "", cx);
            }
        }
    }

    fn use_output_as_input(&mut self, cx: &mut Context<Self>) {
        let output = text_of(&self.output, cx);
        if output.is_empty() {
            return;
        }
        set_text(&self.input, output, cx);
        if self.mode == Mode::Convert {
            std::mem::swap(&mut self.from, &mut self.to);
        }
        self.run(cx);
    }
}

impl Render for JsonTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let formats: Vec<(DataFormat, &'static str)> =
            DataFormat::ALL.iter().map(|f| (*f, f.label())).collect();
        let mode = choice(
            "json-mode",
            &[
                (Mode::Format, "Format"),
                (Mode::Minify, "Minify"),
                (Mode::Convert, "Convert"),
            ],
            self.mode,
            |this: &mut Self, mode, _, cx| {
                this.mode = mode;
                this.run(cx);
            },
            cx,
        );
        let options = match self.mode {
            Mode::Format => h_flex()
                .gap_2()
                .child(caption("Indent"))
                .child(choice(
                    "json-indent",
                    &[
                        (Indent::Spaces(2), "2 spaces"),
                        (Indent::Spaces(4), "4 spaces"),
                        (Indent::Tab, "Tab"),
                    ],
                    self.indent,
                    |this: &mut Self, indent, _, cx| {
                        this.indent = indent;
                        this.run(cx);
                    },
                    cx,
                ))
                .into_any_element(),
            Mode::Minify => div().into_any_element(),
            Mode::Convert => h_flex()
                .gap_2()
                .child(caption("From"))
                .child(choice(
                    "json-from",
                    &formats,
                    self.from,
                    |this: &mut Self, f, _, cx| {
                        this.from = f;
                        this.run(cx);
                    },
                    cx,
                ))
                .child(caption("To"))
                .child(choice(
                    "json-to",
                    &formats,
                    self.to,
                    |this: &mut Self, f, _, cx| {
                        this.to = f;
                        this.run(cx);
                    },
                    cx,
                ))
                .into_any_element(),
        };
        let output_text = text_of(&self.output, cx);
        let trailing = h_flex()
            .gap_1()
            .child(
                IconButton::new("json-use-output", IconName::ArrowLeft)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Use output as input"))
                    .on_click(cx.listener(|this, _, _, cx| this.use_output_as_input(cx))),
            )
            .child(copy_button("json-copy", output_text, cx))
            .into_any_element();
        let input_title = if self.mode == Mode::Convert {
            self.from.label()
        } else {
            "JSON"
        };
        let output_title = match self.mode {
            Mode::Convert => self.to.label(),
            _ => "Result",
        };
        layout(
            v_flex()
                .gap_2()
                .child(controls().child(mode).child(options))
                .children(self.status.clone().map(|status| status_line(status, cx))),
            split(
                panel("json-in", input_title, None, self.input.clone()),
                panel(
                    "json-out",
                    output_title,
                    Some(trailing),
                    self.output.clone(),
                ),
            ),
        )
    }
}

impl super::KitTool for JsonTool {
    fn inputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.input.clone()]
    }

    fn rerun(&mut self, cx: &mut Context<Self>) {
        self.run(cx);
        cx.notify();
    }
}
