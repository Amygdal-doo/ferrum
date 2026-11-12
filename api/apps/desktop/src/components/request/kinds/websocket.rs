//! WebSocket: subprotocols, message composer (text / JSON / base64
//! binary) and saved message templates. The live log is in the response
//! panel.

use super::{labelled, setter};
use crate::app::FerrumApi;
use crate::components::{hint, select};
use crate::state::protocols::WsStatus;
use ferrumapi_domain::RequestTab;
use ferrumapi_domain::protocol::{WsPayloadKind, WsTemplate};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use ui::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _,
    Disableable as _, IconButton, IconName, IconSize, Tooltip, h_flex, v_flex,
};

fn template_name(payload: &str) -> String {
    let one_line: String = payload.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut name: String = one_line.chars().take(32).collect();
    if one_line.chars().count() > 32 {
        name.push('…');
    }
    if name.is_empty() {
        "Empty message".into()
    } else {
        name
    }
}

impl FerrumApi {
    pub(super) fn render_ws_composer(
        &mut self,
        tab: &RequestTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = tab.id;
        let spec = tab.request.specs.websocket.clone();
        let open = self.state.protocols.ws_status(id) == Some(WsStatus::Open);
        let protocols = self.spec_field(
            id,
            "ws-protocols",
            &spec.subprotocols,
            "Subprotocols, comma-separated (optional)",
            None,
            setter(|r, t| r.specs.websocket.subprotocols = t),
            window,
            cx,
        );
        let kind = select(
            SharedString::from(format!("ws-kind-{id}")),
            spec.draft_kind.label(),
            WsPayloadKind::ALL
                .into_iter()
                .map(|k| (SharedString::from(k.label()), k))
                .collect(),
            move |this, k, _| {
                this.state
                    .edit_request(id, |r| r.specs.websocket.draft_kind = k)
            },
            window,
            cx,
        );
        let draft = self.spec_field(
            id,
            "ws-draft",
            &spec.draft,
            "Message to send ({{variables}} are substituted)",
            Some(6),
            setter(|r, t| r.specs.websocket.draft = t),
            window,
            cx,
        );
        let (draft_kind, draft_text) = (spec.draft_kind, spec.draft.clone());
        let send_draft = draft_text.clone();
        let colors = cx.theme().colors().clone();
        let templates: Vec<AnyElement> = spec
            .templates
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let (load, send) = (t.clone(), t.clone());
                h_flex()
                    .id(SharedString::from(format!("ws-template-{i}")))
                    .gap_2()
                    .px_1()
                    .rounded_sm()
                    .hover(|s| s.bg(colors.element_hover))
                    .child(
                        div()
                            .w(px(56.))
                            .text_xs()
                            .text_color(colors.text_muted)
                            .child(t.kind.label()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .child(t.name.clone()),
                    )
                    .child(
                        Button::new(SharedString::from(format!("ws-template-load-{i}")), "Edit")
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .tooltip(Tooltip::text("Put this message in the composer"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let t = load.clone();
                                this.state.edit_request(id, |r| {
                                    r.specs.websocket.draft = t.payload;
                                    r.specs.websocket.draft_kind = t.kind;
                                });
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("ws-template-send-{i}")), "Send")
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .disabled(!open)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ws_send(id, send.kind, &send.payload, cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        IconButton::new(
                            SharedString::from(format!("ws-template-del-{i}")),
                            IconName::Trash,
                        )
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Delete template"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.edit_request(id, |r| {
                                if i < r.specs.websocket.templates.len() {
                                    r.specs.websocket.templates.remove(i);
                                }
                            });
                            cx.notify();
                        })),
                    )
                    .into_any_element()
            })
            .collect();

        v_flex()
            .gap_3()
            .child(labelled("Subprotocols", protocols))
            .child(
                h_flex()
                    .gap_2()
                    .child(kind)
                    .child(div().flex_1())
                    .child(
                        Button::new(
                            SharedString::from(format!("ws-save-template-{id}")),
                            "Save as template",
                        )
                        .style(ButtonStyle::Subtle)
                        .size(ButtonSize::Compact)
                        .tooltip(Tooltip::text(
                            "Keep this message on the request (saved with it)",
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let template = WsTemplate {
                                name: template_name(&draft_text),
                                kind: draft_kind,
                                payload: draft_text.clone(),
                            };
                            this.state
                                .edit_request(id, |r| r.specs.websocket.templates.push(template));
                            cx.notify();
                        })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("ws-send-{id}")), "Send message")
                            .style(ButtonStyle::Filled)
                            .size(ButtonSize::Compact)
                            .disabled(!open)
                            .tooltip(Tooltip::text(if open {
                                "Send on the open connection"
                            } else {
                                "Connect first"
                            }))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ws_send(id, draft_kind, &send_draft, cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(div().min_h(px(120.)).child(draft))
            .child(labelled(
                "Templates",
                if templates.is_empty() {
                    hint("Saved messages appear here.").into_any_element()
                } else {
                    v_flex().gap_0p5().children(templates).into_any_element()
                },
            ))
            .into_any_element()
    }
}
