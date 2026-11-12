//! "New request": pick a kind (HTTP, GraphQL, SOAP, OData, tRPC, WebSocket,
//! gRPC) or open the webhook inspector. A popover on the tab strip opens a
//! tab; the dialog (from the tree) adds the request to a collection.

use super::modal;
use crate::app::FerrumApi;
use crate::components::{MenuHandler, action_menu, hint, kind_color, menu_item};
use crate::state::Dialog;
use ferrumapi_domain::{CollectionId, ItemId, RequestKind};
use gpui::{AnyElement, Context, ElementId, SharedString, prelude::*, px};
use ui::{
    ActiveTheme as _, ButtonCommon as _, IconButton, IconName, IconSize, PopoverMenu, Tooltip,
    h_flex, v_flex,
};

/// One-line description of each kind.
#[must_use]
pub const fn kind_hint(kind: RequestKind) -> &'static str {
    match kind {
        RequestKind::Rest => "Any HTTP method, params, headers and body",
        RequestKind::Graphql => "Query + variables, schema introspection, field completion",
        RequestKind::Soap => "SOAP 1.1 / 1.2 envelope, SOAPAction, WSDL operations",
        RequestKind::Odata => "Query builder ($filter, $select, …) and $metadata entity sets",
        RequestKind::Trpc => "Procedure query / mutation with JSON input, batching",
        RequestKind::WebSocket => "Connect, send text / JSON / binary, live message log",
        RequestKind::Grpc => "Unary and server-streaming calls via reflection or .proto",
    }
}

fn entries() -> Vec<(SharedString, MenuHandler)> {
    let mut out: Vec<(SharedString, MenuHandler)> = RequestKind::ALL
        .into_iter()
        .map(|kind| {
            menu_item(
                format!("New {} request", kind.label()),
                move |this, _, _| {
                    this.new_tab_of_kind(kind);
                },
            )
        })
        .collect();
    out.push(menu_item("Webhook inspector…", |this, _, _| {
        this.state.dialog = Some(Dialog::Webhooks);
    }));
    out
}

/// Chevron next to "+" listing every kind.
pub fn new_request_menu(id: impl Into<ElementId>, cx: &mut Context<FerrumApi>) -> impl IntoElement {
    let weak = cx.weak_entity();
    PopoverMenu::new(id)
        .trigger(
            IconButton::new("new-request-kind", IconName::ChevronDown)
                .icon_size(IconSize::XSmall)
                .tooltip(Tooltip::text("New request of another kind")),
        )
        .menu(move |window, cx| Some(action_menu(window, cx, weak.clone(), entries())))
}

impl FerrumApi {
    pub(super) fn render_new_request_dialog(
        &mut self,
        target: (CollectionId, Option<ItemId>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let (collection, parent) = target;
        let rows: Vec<_> = RequestKind::ALL
            .into_iter()
            .map(|kind| {
                h_flex()
                    .id(SharedString::from(format!("new-kind-{}", kind.badge())))
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(colors.element_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.dialog = None;
                        this.add_request(collection, parent, kind, cx);
                        cx.notify();
                    }))
                    .child(
                        gpui::div()
                            .w(px(52.))
                            .flex_none()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(kind_color(kind, cx))
                            .child(kind.badge()),
                    )
                    .child(
                        v_flex()
                            .child(gpui::div().text_sm().child(kind.label()))
                            .child(hint(kind_hint(kind))),
                    )
            })
            .collect();
        modal("New request", px(520.), cx)
            .child(v_flex().gap_0p5().children(rows))
            .child(
                h_flex()
                    .id("new-kind-webhooks")
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(colors.element_hover))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.dialog = Some(Dialog::Webhooks);
                        cx.notify();
                    }))
                    .child(
                        gpui::div()
                            .w(px(52.))
                            .flex_none()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(colors.text_muted)
                            .child("HOOK"),
                    )
                    .child(
                        v_flex()
                            .child(gpui::div().text_sm().child("Webhook inspector"))
                            .child(hint(
                                "Capture incoming webhooks; save one here as a REST request",
                            )),
                    ),
            )
            .into_any_element()
    }
}
