//! gRPC: schema source (reflection / .proto), method picker, JSON message.

use super::{labelled, load_button, setter};
use crate::app::FerrumApi;
use crate::commands::files::file_name;
use crate::components::{hint, select};
use crate::state::protocols::Loadable;
use ferrumapi_domain::RequestTab;
use ferrumapi_domain::protocol::grpc::{GrpcSchemaSource, split_method};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*};
use ui::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _,
    LoadingLabel, Tooltip, h_flex, v_flex,
};

impl FerrumApi {
    pub(super) fn render_grpc(
        &mut self,
        tab: &RequestTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = tab.id;
        let spec = tab.request.specs.grpc.clone();
        let source = select(
            SharedString::from(format!("grpc-source-{id}")),
            spec.source.label(),
            GrpcSchemaSource::ALL
                .into_iter()
                .map(|s| (SharedString::from(s.label()), s))
                .collect(),
            move |this, s, _| this.state.edit_request(id, |r| r.specs.grpc.source = s),
            window,
            cx,
        );
        let state = self.state.protocols.grpc.get(&id).cloned();
        let loaded = matches!(state, Some(Loadable::Ready(_)));
        let methods = state
            .as_ref()
            .and_then(Loadable::ready)
            .cloned()
            .unwrap_or_default();
        let current = methods
            .iter()
            .find(|m| split_method(&m.full_name) == split_method(&spec.method));
        let shape =
            current.map(|m| format!("{} · {} → {}", m.shape(), m.input_type, m.output_type));
        let template = current.map(|m| m.input_template.clone());
        let method_select = (!methods.is_empty()).then(|| {
            select(
                SharedString::from(format!("grpc-method-{id}")),
                if spec.method.trim().is_empty() {
                    "Choose a method".to_owned()
                } else {
                    spec.method.clone()
                },
                methods
                    .iter()
                    .map(|m| {
                        (
                            SharedString::from(format!("{} ({})", m.full_name, m.shape())),
                            m.clone(),
                        )
                    })
                    .collect(),
                move |this, m, _| {
                    this.state.edit_request(id, |r| {
                        let blank = matches!(r.specs.grpc.message.trim(), "" | "{}");
                        r.specs.grpc.method = m.full_name;
                        if blank {
                            r.specs.grpc.message = m.input_template;
                        }
                    });
                },
                window,
                cx,
            )
        });
        let method = self.spec_field(
            id,
            "grpc-method",
            &spec.method,
            "package.Service/Method",
            None,
            setter(|r, t| r.specs.grpc.method = t),
            window,
            cx,
        );
        let message = self.json_editor(
            id,
            "grpc-message",
            &spec.message,
            "{ \"name\": \"{{user}}\" }",
            10,
            setter(|r, t| r.specs.grpc.message = t),
            window,
            cx,
        );
        let proto = (spec.source == GrpcSchemaSource::ProtoFile).then(|| {
            let imports = self.spec_field(
                id,
                "grpc-imports",
                &spec.import_paths,
                "Import folders, one per line (the file's folder is included)",
                Some(2),
                setter(|r, t| r.specs.grpc.import_paths = t),
                window,
                cx,
            );
            let chosen = if spec.proto_file.trim().is_empty() {
                "No file chosen".to_owned()
            } else {
                file_name(&spec.proto_file)
            };
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new(
                                SharedString::from(format!("grpc-pick-{id}")),
                                "Choose .proto…",
                            )
                            .style(ButtonStyle::Outlined)
                            .size(ButtonSize::Compact)
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.pick_then(
                                        cx,
                                        "Choose a .proto file",
                                        false,
                                        move |this, path, _| {
                                            let path = path.to_string_lossy().into_owned();
                                            this.state.edit_request(id, |r| {
                                                r.specs.grpc.proto_file = path
                                            });
                                        },
                                    );
                                },
                            )),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("grpc-proto-{id}")))
                                .text_xs()
                                .tooltip(Tooltip::text(spec.proto_file.clone()))
                                .child(chosen),
                        ),
                )
                .child(imports)
        });
        let status = match state {
            None => {
                hint("Load the methods to pick one and get a message template.").into_any_element()
            }
            Some(Loadable::Loading) => LoadingLabel::new("Loading methods").into_any_element(),
            Some(Loadable::Failed(e)) => div()
                .text_xs()
                .text_color(cx.theme().status().error)
                .child(e)
                .into_any_element(),
            Some(Loadable::Ready(m)) => hint(format!("{} methods", m.len())).into_any_element(),
        };
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_2()
                    .items_end()
                    .child(labelled("Schema", source))
                    .child(load_button(
                        format!("grpc-load-{id}"),
                        loaded,
                        "Fetch services via reflection or compile the .proto file",
                        cx,
                        move |this, cx| this.load_grpc_methods(id, loaded, cx),
                    ))
                    .child(status),
            )
            .children(proto)
            .child(
                h_flex()
                    .gap_2()
                    .items_end()
                    .child(div().flex_1().child(labelled("Method", method)))
                    .children(method_select),
            )
            .children(shape.map(hint))
            .child(labelled("Message (JSON)", message))
            .children(template.map(|t| {
                Button::new(
                    SharedString::from(format!("grpc-template-{id}")),
                    "Reset to template",
                )
                .style(ButtonStyle::Subtle)
                .size(ButtonSize::Compact)
                .tooltip(Tooltip::text(
                    "Replace the message with every field at its default",
                ))
                .on_click(cx.listener(move |this, _, _, cx| {
                    let t = t.clone();
                    this.state.edit_request(id, |r| r.specs.grpc.message = t);
                    cx.notify();
                }))
            }))
            .into_any_element()
    }
}
