//! Method select, URL input, variable strip, Send / Cancel, Save and Code.

use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_request_edit};
use crate::state::Dialog;
use crate::state::protocols::WsStatus;
use ferrum_ui::kbd;
use ferrumapi_application::interop::codegen::CodeTarget;
use ferrumapi_domain::urlparams::params_from_url;
use ferrumapi_domain::vars::Segment;
use ferrumapi_domain::{RequestKind, RequestTab, VariableSet};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use std::collections::HashSet;
use ui::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonStyle, Clickable as _,
    StyledTypography as _, Tooltip, h_flex, v_flex,
};

impl FerrumApi {
    pub(super) fn render_url_bar(
        &mut self,
        tab: &RequestTab,
        vars: &VariableSet,
        secrets: Vec<String>,
        loading: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let id = tab.id;
        let kind = tab.request.kind;
        let (method_select, method_colour) = self.render_method_picker(tab, window, cx);
        let url = self.field(
            FieldSpec::new(format!("url:{id}"))
                .value(tab.request.url.clone())
                .placeholder(kind.url_placeholder())
                .mono()
                .borderless()
                .autofocus(),
            on_request_edit(move |this, url, _| {
                this.state.edit_request(id, |r| {
                    r.params = params_from_url(&url, &r.params);
                    r.url = url;
                });
            }),
            window,
            cx,
        );
        let segments = vars.segments(&tab.request.url);
        let has_vars = segments.iter().any(|s| matches!(s, Segment::Var { .. }));

        let ws_status = self.state.protocols.ws_status(id);
        let send = if kind == RequestKind::WebSocket {
            let connected = matches!(ws_status, Some(WsStatus::Open | WsStatus::Connecting));
            h_flex()
                .gap_1()
                .child(
                    Button::new(
                        "ws-connect",
                        if connected { "Disconnect" } else { "Connect" },
                    )
                    .style(if connected {
                        ButtonStyle::Outlined
                    } else {
                        ButtonStyle::Filled
                    })
                    .tooltip(Tooltip::text("Connect / disconnect (⌘/Ctrl+Enter)"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.ws_toggle(id, cx);
                        cx.notify();
                    })),
                )
                .child(kbd("⌘↵", cx))
                .into_any_element()
        } else if loading {
            Button::new("cancel-request", "Cancel")
                .style(ButtonStyle::Outlined)
                .tooltip(Tooltip::text("Cancel the request (⌘/Ctrl+Enter)"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.cancel(id);
                    cx.notify();
                }))
                .into_any_element()
        } else {
            h_flex()
                .gap_1()
                .child(
                    Button::new(
                        "send-request",
                        if kind == RequestKind::Grpc {
                            "Invoke"
                        } else {
                            "Send"
                        },
                    )
                    .style(ButtonStyle::Filled)
                    .tooltip(Tooltip::text("Send (⌘/Ctrl+Enter)"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.send(id, cx);
                        cx.notify();
                    })),
                )
                .child(kbd("⌘↵", cx))
                .into_any_element()
        };

        v_flex()
            .gap_1()
            .px_3()
            .pt_3()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .h_9()
                            .rounded_md()
                            .border_1()
                            .border_color(colors.border)
                            .bg(colors.editor_background)
                            .child(
                                div()
                                    .flex_none()
                                    .w(px(112.))
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .px_1()
                                    .border_r_1()
                                    .border_color(colors.border)
                                    .text_color(method_colour)
                                    .child(method_select),
                            )
                            .child(div().flex_1().min_w_0().px_3().child(url)),
                    )
                    .child(send)
                    .child(
                        Button::new("save-request", "Save")
                            .style(ButtonStyle::Outlined)
                            .tooltip(Tooltip::text("Save (⌘/Ctrl+S)"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.save_tab(id, cx);
                                cx.notify();
                            })),
                    )
                    .when(kind.is_http(), |this| {
                        this.child(
                            Button::new("load-test", "Load")
                                .style(ButtonStyle::Subtle)
                                .tooltip(Tooltip::text("Load test this request"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open_load_test(id);
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        Button::new("generate-code", "</>")
                            .style(ButtonStyle::Subtle)
                            .tooltip(Tooltip::text("Generate code"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.state.dialog = Some(Dialog::Code {
                                    tab_id: id,
                                    target: CodeTarget::Curl,
                                });
                                cx.notify();
                            })),
                    ),
            )
            .when(has_vars, |this| {
                this.child(var_strip(segments, vars, secrets, cx))
            })
            .into_any_element()
    }
}

/// The URL split into text and `{{variables}}`: resolved in the info colour,
/// unresolved in the error colour. Hover shows the value (secrets masked).
fn var_strip(
    segments: Vec<Segment>,
    vars: &VariableSet,
    secrets: Vec<String>,
    cx: &mut Context<FerrumApi>,
) -> AnyElement {
    let colors = cx.theme().colors().clone();
    let status = cx.theme().status().clone();
    let secret: HashSet<String> = secrets.into_iter().collect();
    h_flex()
        .pl(px(112.))
        .overflow_hidden()
        .text_xs()
        .font_buffer(cx)
        .text_color(colors.text_muted)
        .children(segments.into_iter().enumerate().map(|(i, seg)| {
            match seg {
                Segment::Text(t) => div().child(t).into_any_element(),
                Segment::Var {
                    name,
                    resolved: true,
                } => {
                    let value = if secret.contains(&name) {
                        "••••••".to_owned()
                    } else {
                        vars.get(&name).unwrap_or_default().to_owned()
                    };
                    div()
                        .id(SharedString::from(format!("url-var-{i}")))
                        .px_0p5()
                        .rounded_sm()
                        .bg(status.info_background)
                        .text_color(status.info)
                        .tooltip(Tooltip::text(format!("{name} = {value}")))
                        .child(format!("{{{{{name}}}}}"))
                        .into_any_element()
                }
                Segment::Var {
                    name,
                    resolved: false,
                } => div()
                    .id(SharedString::from(format!("url-var-{i}")))
                    .px_0p5()
                    .rounded_sm()
                    .bg(status.error_background)
                    .text_color(status.error)
                    .tooltip(Tooltip::text(format!(
                        "{name} is not defined in the active environment, collection or globals"
                    )))
                    .child(format!("{{{{{name}}}}}"))
                    .into_any_element(),
            }
        }))
        .into_any_element()
}
