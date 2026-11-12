//! The GraphQL schema panel: load / reload, filter, types and fields.

use super::load_button;
use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change};
use crate::components::hint;
use crate::state::protocols::Loadable;
use ferrumapi_domain::TabId;
use ferrumapi_domain::protocol::graphql_schema::GqlType;
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use ui::{
    ActiveTheme as _, Color, Label, LabelCommon as _, LabelSize, LoadingLabel, h_flex, v_flex,
};

/// Rows of one type: its fields (or input fields / enum values).
fn type_detail(t: &GqlType) -> Vec<(String, String)> {
    if !t.fields.is_empty() {
        t.fields
            .iter()
            .map(|f| {
                let args = if f.args.is_empty() {
                    String::new()
                } else {
                    let list: Vec<String> = f
                        .args
                        .iter()
                        .map(|a| format!("{}: {}", a.name, a.type_ref))
                        .collect();
                    format!("({})", list.join(", "))
                };
                (format!("{}{args}", f.name), f.type_ref.clone())
            })
            .collect()
    } else if !t.input_fields.is_empty() {
        t.input_fields
            .iter()
            .map(|f| (f.name.clone(), f.type_ref.clone()))
            .collect()
    } else {
        t.enum_values
            .iter()
            .map(|v| (v.clone(), String::new()))
            .collect()
    }
}

impl FerrumApi {
    pub(super) fn render_schema_panel(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let state = self.state.protocols.graphql.get(&tab_id).cloned();
        let loaded = matches!(state, Some(Loadable::Ready(_)));
        let header = h_flex()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .child(Label::new("Schema").size(LabelSize::Small)),
            )
            .child(load_button(
                format!("gql-schema-load-{tab_id}"),
                loaded,
                "Introspect the endpoint (cached per URL)",
                cx,
                move |this, cx| this.load_graphql_schema(tab_id, loaded, cx),
            ));
        let body = match state {
            None => {
                hint("Load the schema to browse types and get field completion.").into_any_element()
            }
            Some(Loadable::Loading) => LoadingLabel::new("Introspecting").into_any_element(),
            Some(Loadable::Failed(e)) => div()
                .text_xs()
                .text_color(cx.theme().status().error)
                .child(e)
                .into_any_element(),
            Some(Loadable::Ready(schema)) => {
                let filter = self.field(
                    FieldSpec::new("gql-schema-filter")
                        .value(self.state.protocols.schema_filter.clone())
                        .placeholder("Filter types"),
                    on_change(|this, text, _| this.state.protocols.schema_filter = text),
                    window,
                    cx,
                );
                let needle = self.state.protocols.schema_filter.to_lowercase();
                let open = self.state.protocols.schema_open.clone();
                let mut rows: Vec<AnyElement> = Vec::new();
                for t in schema.user_types() {
                    if !needle.is_empty() && !t.name.to_lowercase().contains(&needle) {
                        continue;
                    }
                    let name = t.name.clone();
                    let expanded = open.as_deref() == Some(t.name.as_str());
                    rows.push(
                        h_flex()
                            .id(SharedString::from(format!("gql-type-{name}")))
                            .gap_2()
                            .px_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(colors.element_hover))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let open = &mut this.state.protocols.schema_open;
                                *open =
                                    (open.as_deref() != Some(name.as_str())).then(|| name.clone());
                                cx.notify();
                            }))
                            .child(div().flex_1().text_sm().child(t.name.clone()))
                            .child(
                                Label::new(t.kind.to_lowercase())
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            )
                            .into_any_element(),
                    );
                    if expanded {
                        for (field, ty) in type_detail(t) {
                            rows.push(
                                h_flex()
                                    .pl_3()
                                    .gap_2()
                                    .text_xs()
                                    .child(div().flex_1().min_w_0().truncate().child(field))
                                    .child(div().text_color(colors.text_muted).child(ty))
                                    .into_any_element(),
                            );
                        }
                    }
                }
                v_flex()
                    .gap_1()
                    .child(filter)
                    .child(
                        v_flex()
                            .id(SharedString::from(format!("gql-types-{tab_id}")))
                            .max_h(px(360.))
                            .overflow_y_scroll()
                            .children(rows),
                    )
                    .into_any_element()
            }
        };
        v_flex()
            .gap_2()
            .child(header)
            .child(body)
            .into_any_element()
    }
}
