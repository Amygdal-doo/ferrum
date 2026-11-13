//! GraphQL: query editor with field completion, operation name, GET
//! toggle, variables; the schema panel lives in `graphql_schema.rs`.

use super::{labelled, setter};
use crate::app::FerrumApi;
use crate::components::{checkbox, hint};
use crate::state::protocols::Loadable;
use ferrum_ui::components::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _,
    LabelSize, Tooltip, h_flex, v_flex,
};
use ferrumapi_domain::RequestTab;
use ferrumapi_domain::protocol::graphql_schema::{complete_fields, partial_word};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};

/// Completion chips shown at most.
const MAX_COMPLETIONS: usize = 12;

impl FerrumApi {
    pub(super) fn render_graphql_query(
        &mut self,
        tab: &RequestTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = tab.id;
        let spec = tab.request.specs.graphql.clone();
        let op = self.spec_field(
            id,
            "gql-op",
            &spec.operation_name,
            "Operation name (optional)",
            None,
            setter(|r, t| r.specs.graphql.operation_name = t),
            window,
            cx,
        );
        let query = self.spec_field(
            id,
            "gql-query",
            &spec.query,
            "query { … }",
            Some(14),
            setter(|r, t| r.specs.graphql.query = t),
            window,
            cx,
        );
        let use_get = checkbox(
            SharedString::from(format!("gql-get-{id}")),
            spec.use_get,
            move |this, on, _| {
                this.state
                    .edit_request(id, |r| r.specs.graphql.use_get = on)
            },
            cx,
        );

        // Field completion from the loaded schema at the caret.
        let schema = self
            .state
            .protocols
            .graphql
            .get(&id)
            .and_then(Loadable::ready)
            .cloned();
        let cursor = query.read(cx).cursor();
        let text = query.read(cx).text().to_owned();
        let chips: Vec<AnyElement> = schema
            .as_ref()
            .map(|s| complete_fields(s, &text, cursor, MAX_COMPLETIONS))
            .unwrap_or_default()
            .into_iter()
            .map(|f| {
                let name = f.name.clone();
                let partial_len = partial_word(&text, cursor).len();
                Button::new(
                    SharedString::from(format!("gql-complete-{name}")),
                    name.clone(),
                )
                .style(ButtonStyle::Subtle)
                .size(ButtonSize::Compact)
                .label_size(LabelSize::Small)
                .tooltip(Tooltip::text(format!("{}: {}", f.name, f.type_ref)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(input) = this.fields.entity(&format!("gql-query:{id}")) {
                        let name = name.clone();
                        input.update(cx, |input, cx| {
                            input.select_range(cursor - partial_len..cursor, cx);
                            input.insert(&name, window, cx);
                            input.focus(window, cx);
                        });
                    }
                    cx.notify();
                }))
                .into_any_element()
            })
            .collect();

        let colors = cx.theme().colors().clone();
        let editor = v_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(
                h_flex().gap_3().child(div().w(px(260.)).child(op)).child(
                    h_flex()
                        .id(SharedString::from(format!("gql-get-row-{id}")))
                        .gap_1()
                        .tooltip(Tooltip::text(
                            "Send the query as GET parameters instead of a POST body",
                        ))
                        .child(use_get)
                        .child(hint("Send as GET")),
                ),
            )
            .child(div().min_h(px(220.)).child(query))
            .when(!chips.is_empty(), |this| {
                this.child(
                    h_flex()
                        .flex_wrap()
                        .gap_1()
                        .child(hint("Fields:"))
                        .children(chips),
                )
            });
        let panel = self.render_schema_panel(id, window, cx);
        h_flex()
            .items_start()
            .gap_3()
            .child(editor)
            .child(
                div()
                    .w(px(280.))
                    .flex_none()
                    .border_l_1()
                    .border_color(colors.border)
                    .pl_3()
                    .child(panel),
            )
            .into_any_element()
    }

    pub(super) fn render_graphql_variables(
        &mut self,
        tab: &RequestTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let editor = self.json_editor(
            tab.id,
            "gql-vars",
            &tab.request.specs.graphql.variables,
            "{ \"id\": \"{{userId}}\" }",
            10,
            setter(|r, t| r.specs.graphql.variables = t),
            window,
            cx,
        );
        v_flex()
            .gap_2()
            .child(labelled("Variables (JSON object)", editor))
            .child(hint(
                "{{variables}} from the environment are substituted before sending.",
            ))
            .into_any_element()
    }
}
