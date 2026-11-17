//! JSON Schema: validate a document, or generate a schema from a sample.

use super::{layout, split};
use crate::widgets::*;
use ferrum_ui::prelude::*;
use ferrumkit_core::json_schema::{self, SchemaError};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Validate,
    Generate,
}

/// Failures listed at most.
const MAX_ERRORS: usize = 200;

pub struct JsonSchemaTool {
    schema: Entity<TextInput>,
    document: Entity<TextInput>,
    /// The generated schema.
    output: Entity<TextInput>,
    mode: Mode,
    result: Option<Result<Vec<SchemaError>, String>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for JsonSchemaTool {}

impl JsonSchemaTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let schema = editor("Paste a JSON Schema…", 16, window, cx);
        let document = editor("Paste the JSON document (or a sample)…", 16, window, cx);
        let output = output(16, window, cx);
        let subscriptions = vec![
            on_change(&schema, cx, Self::run),
            on_change(&document, cx, Self::run),
        ];
        Self {
            schema,
            document,
            output,
            mode: Mode::Validate,
            result: None,
            _subscriptions: subscriptions,
        }
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let document = text_of(&self.document, cx);
        match self.mode {
            Mode::Validate => {
                let schema = text_of(&self.schema, cx);
                self.result = (!schema.trim().is_empty() && !document.trim().is_empty())
                    .then(|| json_schema::validate(&schema, &document));
            }
            Mode::Generate => {
                let (text, result) = if document.trim().is_empty() {
                    (String::new(), None)
                } else {
                    match json_schema::generate(&document) {
                        Ok(schema) => (schema, Some(Ok(Vec::new()))),
                        Err(error) => (String::new(), Some(Err(error))),
                    }
                };
                set_text(&self.output, text, cx);
                self.result = result;
            }
        }
    }

    /// Generated schema into the schema field, then validate.
    fn use_as_schema(&mut self, cx: &mut Context<Self>) {
        let generated = text_of(&self.output, cx);
        if generated.is_empty() {
            return;
        }
        set_text(&self.schema, generated, cx);
        self.mode = Mode::Validate;
        self.run(cx);
    }

    fn render_result(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let result = self.result.clone()?;
        if self.mode == Mode::Generate {
            return result.err().map(|e| status_line(Err(e), cx));
        }
        Some(match result {
            Err(error) => status_line(Err(error), cx),
            Ok(errors) if errors.is_empty() => {
                status_line(Ok("Valid: the document matches the schema".into()), cx)
            }
            Ok(errors) => {
                let count = errors.len();
                let rows: Vec<AnyElement> = errors
                    .into_iter()
                    .take(MAX_ERRORS)
                    .enumerate()
                    .map(|(i, error)| {
                        let path = if error.path.is_empty() {
                            "(root)".to_string()
                        } else {
                            error.path
                        };
                        value_row(format!("schema-error-{i}"), path, error.message, cx)
                    })
                    .collect();
                v_flex()
                    .gap_1()
                    .child(status_line(
                        Err(format!(
                            "{count} problem{}",
                            if count == 1 { "" } else { "s" }
                        )),
                        cx,
                    ))
                    .child(
                        v_flex()
                            .id("schema-errors")
                            .max_h(px(180.))
                            .overflow_y_scroll()
                            .gap_1()
                            .children(rows),
                    )
                    .into_any_element()
            }
        })
    }
}

impl Render for JsonSchemaTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = choice(
            "schema-mode",
            &[
                (Mode::Validate, "Validate"),
                (Mode::Generate, "Generate from sample"),
            ],
            self.mode,
            |this: &mut Self, mode, _, cx| {
                this.mode = mode;
                this.run(cx);
            },
            cx,
        );
        let note = match self.mode {
            Mode::Validate => {
                "Draft 4 to 2020-12 (from $schema); formats are checked; remote $refs are not fetched."
            }
            Mode::Generate => {
                "Every key present is required; array items are merged across elements."
            }
        };
        let result = self.render_result(cx);
        let body = match self.mode {
            Mode::Validate => split(
                panel("schema-in", "Schema", None, self.schema.clone()),
                panel("schema-doc", "Document", None, self.document.clone()),
            )
            .into_any_element(),
            Mode::Generate => {
                let generated = text_of(&self.output, cx);
                let trailing = h_flex()
                    .gap_1()
                    .child(
                        Button::new("schema-use", "Validate with it")
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .label_size(LabelSize::Small)
                            .disabled(generated.is_empty())
                            .on_click(cx.listener(|this, _, _, cx| this.use_as_schema(cx))),
                    )
                    .child(copy_button("schema-copy", generated, cx))
                    .into_any_element();
                split(
                    panel("schema-sample", "Sample", None, self.document.clone()),
                    panel("schema-out", "Schema", Some(trailing), self.output.clone()),
                )
                .into_any_element()
            }
        };
        layout(
            v_flex()
                .gap_2()
                .child(controls().child(mode).child(hint(note)))
                .children(result),
            body,
        )
    }
}

impl super::KitTool for JsonSchemaTool {
    fn inputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.schema.clone(), self.document.clone()]
    }

    fn outputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.output.clone()]
    }

    fn rerun(&mut self, cx: &mut Context<Self>) {
        self.run(cx);
        cx.notify();
    }
}
