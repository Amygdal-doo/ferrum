//! SQL formatter.

use super::{layout, split};
use crate::widgets::*;
use ferrum_ui::prelude::*;
use ferrumkit_core::sql::{self, KeywordCase, SqlDialect, SqlOptions};

pub struct SqlTool {
    input: Entity<TextInput>,
    output: Entity<TextInput>,
    options: SqlOptions,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for SqlTool {}

impl SqlTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor("Paste SQL…", 20, window, cx);
        let output = output(20, window, cx);
        let subscriptions = vec![on_change(&input, cx, Self::run)];
        Self {
            input,
            output,
            options: SqlOptions::default(),
            _subscriptions: subscriptions,
        }
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let input = text_of(&self.input, cx);
        let formatted = if input.trim().is_empty() {
            String::new()
        } else {
            sql::format(&input, self.options)
        };
        set_text(&self.output, formatted, cx);
    }
}

impl Render for SqlTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let indent = choice(
            "sql-indent",
            &[(2, "2 spaces"), (4, "4 spaces"), (0, "Tab")],
            self.options.indent,
            |this: &mut Self, indent, _, cx| {
                this.options.indent = indent;
                this.run(cx);
            },
            cx,
        );
        let case = choice(
            "sql-case",
            &[
                (KeywordCase::Upper, "UPPER"),
                (KeywordCase::Lower, "lower"),
                (KeywordCase::Keep, "As written"),
            ],
            self.options.case,
            |this: &mut Self, case, _, cx| {
                this.options.case = case;
                this.run(cx);
            },
            cx,
        );
        let dialect = choice(
            "sql-dialect",
            &[
                (SqlDialect::Generic, "Generic"),
                (SqlDialect::Postgres, "PostgreSQL"),
                (SqlDialect::SqlServer, "SQL Server"),
            ],
            self.options.dialect,
            |this: &mut Self, dialect, _, cx| {
                this.options.dialect = dialect;
                this.run(cx);
            },
            cx,
        );
        let output_text = text_of(&self.output, cx);
        layout(
            v_flex()
                .gap_2()
                .child(
                    controls()
                        .child(h_flex().gap_2().child(caption("Indent")).child(indent))
                        .child(h_flex().gap_2().child(caption("Keywords")).child(case))
                        .child(h_flex().gap_2().child(caption("Dialect")).child(dialect)),
                )
                .child(hint(
                    "Reformats whitespace and keyword case only; the SQL is not checked.",
                )),
            split(
                panel("sql-in", "SQL", None, self.input.clone()),
                panel(
                    "sql-out",
                    "Formatted",
                    Some(copy_button("sql-copy", output_text, cx)),
                    self.output.clone(),
                ),
            ),
        )
    }
}

impl super::KitTool for SqlTool {
    fn inputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.input.clone()]
    }

    fn outputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.output.clone()]
    }

    fn rerun(&mut self, cx: &mut Context<Self>) {
        self.run(cx);
        cx.notify();
    }
}
