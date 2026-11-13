//! One mock response card: enable, name, status, delay, response headers,
//! match conditions and body.

use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change};
use crate::components::widgets::RowEditor;
use crate::components::{checkbox, hint};
use crate::state::kv;
use ferrum_ui::components::{
    ActiveTheme as _, ButtonCommon as _, Clickable as _, IconButton, IconName, IconSize, Label,
    LabelCommon as _, LabelSize, Tooltip, h_flex, v_flex,
};
use ferrumapi_domain::mock::{MAX_MOCK_DELAY_MS, MockResponse};
use ferrumapi_domain::{CollectionId, KeyValue, MockId, VariableSet};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use std::rc::Rc;

/// Which key/value list of a mock a table edits.
#[derive(Clone, Copy)]
enum Rows {
    Headers,
    Query,
    MatchHeaders,
}

fn rows_mut(m: &mut MockResponse, which: Rows) -> &mut Vec<KeyValue> {
    match which {
        Rows::Headers => &mut m.headers,
        Rows::Query => &mut m.match_query,
        Rows::MatchHeaders => &mut m.match_headers,
    }
}

fn small(text: &'static str) -> Label {
    Label::new(text).size(LabelSize::Small)
}

impl FerrumApi {
    fn mock_rows(
        &mut self,
        (collection, id): (CollectionId, MockId),
        which: Rows,
        rows: Vec<KeyValue>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let on_edit: RowEditor = Rc::new(move |this, i, edit, _| {
            this.edit_mocks(collection, |s| {
                s.edit_mock(id, |m| kv::apply(rows_mut(m, which), i, edit));
            });
        });
        let (scope, placeholder) = match which {
            Rows::Headers => ("headers", "Header"),
            Rows::Query => ("query", "Query param"),
            Rows::MatchHeaders => ("match-headers", "Header"),
        };
        let border = cx.theme().colors().border_variant;
        div()
            .rounded_md()
            .border_1()
            .border_color(border)
            .child(self.render_kv_table(
                format!("mock-{scope}:{id}"),
                rows,
                &VariableSet::new(),
                placeholder,
                on_edit,
                window,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_mock_card(
        &mut self,
        collection: CollectionId,
        mock: MockResponse,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let id = mock.id;
        let edit = move |this: &mut FerrumApi, f: Box<dyn FnOnce(&mut MockResponse)>| {
            this.edit_mocks(collection, |s| s.edit_mock(id, f));
        };
        let name = self.field(
            FieldSpec::new(format!("mock-name:{id}"))
                .initial(mock.name.clone())
                .placeholder("Response name"),
            on_change(move |this, text, _| edit(this, Box::new(move |m| m.name = text))),
            window,
            cx,
        );
        let status = self.field(
            FieldSpec::new(format!("mock-status:{id}"))
                .initial(mock.status.to_string())
                .placeholder("200"),
            on_change(move |this, text, _| {
                if let Ok(status) = text.trim().parse::<u16>()
                    && (100..=599).contains(&status)
                {
                    edit(this, Box::new(move |m| m.status = status));
                }
            }),
            window,
            cx,
        );
        let delay = self.field(
            FieldSpec::new(format!("mock-delay:{id}"))
                .initial(mock.delay_ms.to_string())
                .placeholder("0"),
            on_change(move |this, text, _| {
                let ms = text
                    .trim()
                    .parse::<u64>()
                    .unwrap_or(0)
                    .min(MAX_MOCK_DELAY_MS);
                edit(this, Box::new(move |m| m.delay_ms = ms));
            }),
            window,
            cx,
        );
        let body = self.field(
            FieldSpec::new(format!("mock-body:{id}"))
                .initial(mock.body.clone())
                .placeholder("Response body")
                .multiline(4)
                .mono(),
            on_change(move |this, text, _| edit(this, Box::new(move |m| m.body = text))),
            window,
            cx,
        );
        let key = (collection, id);
        let headers = self.mock_rows(key, Rows::Headers, mock.headers, window, cx);
        let query = self.mock_rows(key, Rows::Query, mock.match_query, window, cx);
        let match_headers = self.mock_rows(key, Rows::MatchHeaders, mock.match_headers, window, cx);

        v_flex()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(colors.border)
            .when(!mock.enabled, |this| this.opacity(0.6))
            .child(
                h_flex()
                    .gap_2()
                    .child(checkbox(
                        SharedString::from(format!("mock-enabled-{id}")),
                        mock.enabled,
                        move |this, on, _| {
                            this.edit_mocks(collection, |s| s.edit_mock(id, |m| m.enabled = on));
                        },
                        cx,
                    ))
                    .child(div().flex_1().min_w_0().child(name))
                    .child(small("Status"))
                    .child(div().w(px(64.)).child(status))
                    .child(small("Delay ms"))
                    .child(div().w(px(80.)).child(delay))
                    .child(
                        IconButton::new(SharedString::from(format!("mock-delete-{id}")), IconName::Trash)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Delete this response"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit_mocks(collection, |s| s.remove_mock(id));
                                cx.notify();
                            })),
                    ),
            )
            .child(small("Response headers"))
            .child(headers)
            .child(small("Body"))
            .child(body)
            .child(hint(
                "Only answer when the request has these query params / headers (empty value: any value). The most specific match wins.",
            ))
            .child(
                h_flex()
                    .gap_2()
                    .items_start()
                    .child(v_flex().flex_1().gap_1().child(small("Match query")).child(query))
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .child(small("Match headers"))
                            .child(match_headers),
                    ),
            )
            .into_any_element()
    }
}
