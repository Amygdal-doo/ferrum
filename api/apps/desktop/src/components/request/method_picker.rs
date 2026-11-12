//! The box left of the URL: method, GraphQL POST / GET, tRPC query /
//! mutation, or the kind.

use crate::app::FerrumApi;
use crate::components::{kind_color, method_color, select};
use ferrumapi_domain::protocol::TrpcType;
use ferrumapi_domain::{HttpMethod, RequestKind, RequestTab};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*};

impl FerrumApi {
    /// The box left of the URL: the method for REST / OData, POST / GET for
    /// GraphQL, Query / Mutation for tRPC, the kind otherwise.
    pub(super) fn render_method_picker(
        &mut self,
        tab: &RequestTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (AnyElement, gpui::Hsla) {
        let id = tab.id;
        let r = &tab.request;
        let kind_colour = kind_color(r.kind, cx);
        match r.kind {
            RequestKind::Rest | RequestKind::Odata => (
                select(
                    SharedString::from(format!("method-{id}")),
                    r.method.to_string(),
                    HttpMethod::ALL
                        .into_iter()
                        .map(|m| (SharedString::from(m.to_string()), m))
                        .collect(),
                    move |this, m, _| this.state.edit_request(id, |r| r.method = m),
                    window,
                    cx,
                )
                .into_any_element(),
                method_color(r.method, cx),
            ),
            RequestKind::Graphql => (
                select(
                    SharedString::from(format!("gql-method-{id}")),
                    if r.specs.graphql.use_get {
                        "GQL · GET"
                    } else {
                        "GQL · POST"
                    },
                    vec![("POST".into(), false), ("GET".into(), true)],
                    move |this, get, _| {
                        this.state
                            .edit_request(id, |r| r.specs.graphql.use_get = get);
                    },
                    window,
                    cx,
                )
                .into_any_element(),
                kind_colour,
            ),
            RequestKind::Trpc => (
                select(
                    SharedString::from(format!("trpc-method-{id}")),
                    r.specs.trpc.procedure_type.label(),
                    TrpcType::ALL
                        .into_iter()
                        .map(|t| (SharedString::from(t.label()), t))
                        .collect(),
                    move |this, t, _| {
                        this.state
                            .edit_request(id, |r| r.specs.trpc.procedure_type = t);
                    },
                    window,
                    cx,
                )
                .into_any_element(),
                kind_colour,
            ),
            kind => (
                div()
                    .px_2()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(kind.label())
                    .into_any_element(),
                kind_colour,
            ),
        }
    }
}
