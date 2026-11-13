//! SOAP: version, SOAPAction, envelope editor, WSDL operations.

use super::{labelled, load_button, setter};
use crate::app::FerrumApi;
use crate::components::{hint, select};
use crate::state::protocols::Loadable;
use ferrum_ui::components::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _,
    LoadingLabel, Tooltip, h_flex, v_flex,
};
use ferrumapi_domain::RequestTab;
use ferrumapi_domain::protocol::SoapVersion;
use ferrumapi_domain::protocol::soap::{envelope, soap_headers};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};

impl FerrumApi {
    pub(super) fn render_soap(
        &mut self,
        tab: &RequestTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = tab.id;
        let spec = tab.request.specs.soap.clone();
        let version = select(
            SharedString::from(format!("soap-version-{id}")),
            spec.version.label(),
            SoapVersion::ALL
                .into_iter()
                .map(|v| (SharedString::from(v.label()), v))
                .collect(),
            move |this, v, _| {
                this.state.edit_request(id, |r| {
                    let soap = &mut r.specs.soap;
                    // Keep the envelope namespace in step with the version.
                    soap.envelope = soap
                        .envelope
                        .replace(soap.version.namespace(), v.namespace());
                    soap.version = v;
                });
            },
            window,
            cx,
        );
        let action = self.spec_field(
            id,
            "soap-action",
            &spec.action,
            "SOAPAction, e.g. http://tempuri.org/Add",
            None,
            setter(|r, t| r.specs.soap.action = t),
            window,
            cx,
        );
        let envelope_field = self.spec_field(
            id,
            "soap-envelope",
            &spec.envelope,
            "<soap:Envelope …>",
            Some(14),
            setter(|r, t| r.specs.soap.envelope = t),
            window,
            cx,
        );
        let wsdl_url = self.spec_field(
            id,
            "soap-wsdl",
            &spec.wsdl_url,
            "WSDL URL, e.g. https://example.com/service.asmx?wsdl",
            None,
            setter(|r, t| r.specs.soap.wsdl_url = t),
            window,
            cx,
        );
        let headers: Vec<String> = soap_headers(&spec)
            .into_iter()
            .map(|h| format!("{}: {}", h.key, h.value))
            .collect();
        let ops = self.render_wsdl_operations(tab, cx);
        let loaded = matches!(self.state.protocols.wsdl.get(&id), Some(Loadable::Ready(_)));
        let wsdl_version = spec.version;
        let editor = v_flex()
            .flex_1()
            .min_w_0()
            .gap_3()
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(labelled("Version", version))
                    .child(div().flex_1().child(labelled("Action", action))),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(hint(format!("Sends {}", headers.join(" · "))))
                    .child(div().flex_1())
                    .child(
                        Button::new(
                            SharedString::from(format!("soap-blank-{id}")),
                            "Blank envelope",
                        )
                        .style(ButtonStyle::Subtle)
                        .size(ButtonSize::Compact)
                        .tooltip(Tooltip::text("Replace the envelope with an empty one"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.edit_request(id, |r| {
                                r.specs.soap.envelope = envelope(wsdl_version, "");
                            });
                            cx.notify();
                        })),
                    ),
            )
            .child(div().min_h(px(240.)).child(envelope_field));
        let colors = cx.theme().colors().clone();
        h_flex()
            .items_start()
            .gap_3()
            .child(editor)
            .child(
                v_flex()
                    .w(px(280.))
                    .flex_none()
                    .gap_2()
                    .border_l_1()
                    .border_color(colors.border)
                    .pl_3()
                    .child(labelled("WSDL", wsdl_url))
                    .child(load_button(
                        format!("soap-wsdl-load-{id}"),
                        loaded,
                        "Fetch the WSDL and list its operations (cached per URL)",
                        cx,
                        move |this, cx| this.load_wsdl(id, loaded, cx),
                    ))
                    .child(ops),
            )
            .into_any_element()
    }

    fn render_wsdl_operations(&mut self, tab: &RequestTab, cx: &mut Context<Self>) -> AnyElement {
        let id = tab.id;
        let current = tab.request.specs.soap.operation.clone();
        let colors = cx.theme().colors().clone();
        match self.state.protocols.wsdl.get(&id).cloned() {
            None => hint("Load a WSDL to pick an operation and get a sample envelope.")
                .into_any_element(),
            Some(Loadable::Loading) => LoadingLabel::new("Loading WSDL").into_any_element(),
            Some(Loadable::Failed(e)) => div()
                .text_xs()
                .text_color(cx.theme().status().error)
                .child(e)
                .into_any_element(),
            Some(Loadable::Ready(ops)) => v_flex()
                .id(SharedString::from(format!("wsdl-ops-{id}")))
                .max_h(px(360.))
                .overflow_y_scroll()
                .children(ops.into_iter().enumerate().map(|(i, op)| {
                    let selected = op.name == current;
                    let label = format!("{} · {}", op.name, op.version.label());
                    h_flex()
                        .id(SharedString::from(format!("wsdl-op-{i}")))
                        .px_1()
                        .py_0p5()
                        .rounded_sm()
                        .text_sm()
                        .cursor_pointer()
                        .when(selected, |this| this.bg(colors.element_selected))
                        .hover(|s| s.bg(colors.element_hover))
                        .tooltip(Tooltip::text(format!("SOAPAction: {}", op.action)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.apply_wsdl_operation(id, &op);
                            cx.notify();
                        }))
                        .child(label)
                }))
                .into_any_element(),
        }
    }
}
