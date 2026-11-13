//! The WebSocket session log: status, counts, frames in / out with time
//! and size, Clear.

use crate::app::FerrumApi;
use crate::components::fields::FieldSpec;
use crate::components::hint;
use crate::state::protocols::WsStatus;
use ferrum_ui::components::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _,
    StyledTypography as _, h_flex, v_flex,
};
use ferrumapi_domain::TabId;
use ferrumapi_domain::protocol::websocket::{WsDirection, WsFrame};
use ferrumapi_domain::text::human_size;
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};

/// Rows rendered at most (newest); the session keeps more.
const MAX_ROWS: usize = 500;

impl FerrumApi {
    pub(super) fn render_ws_log(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let status = cx.theme().status().clone();
        let Some(session) = self.state.protocols.ws.get(&tab_id) else {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_2()
                .child(hint("Not connected."))
                .child(hint(
                    "Enter a ws:// or wss:// URL and press Connect (⌘/Ctrl+Enter).",
                ))
                .into_any_element();
        };
        let (state_label, state_colour) = match session.status {
            WsStatus::Connecting => ("Connecting…", status.warning),
            WsStatus::Open => ("Connected", status.success),
            WsStatus::Closed => ("Disconnected", colors.text_muted),
        };
        let counts = format!(
            "↑ {} sent · ↓ {} received",
            session.log.sent, session.log.received
        );
        let protocol = session.protocol.clone();
        let entries: Vec<_> = session
            .log
            .entries
            .iter()
            .rev()
            .take(MAX_ROWS)
            .cloned()
            .collect();
        let selected = self.state.protocols.ws_selected.get(&tab_id).copied();
        let detail = selected.and_then(|i| {
            entries
                .iter()
                .find(|e| e.at.timestamp_nanos_opt() == Some(i))
                .cloned()
        });

        let rows =
            entries
                .into_iter()
                .map(|e| {
                    let key = e.at.timestamp_nanos_opt().unwrap_or_default();
                    let (arrow, colour) = match (e.direction, e.frame) {
                        (WsDirection::Sent, _) => ("↑", status.info),
                        (WsDirection::Received, _) => ("↓", status.success),
                        (WsDirection::Event, WsFrame::Error) => ("!", status.error),
                        (WsDirection::Event, _) => ("•", colors.text_muted),
                    };
                    let frame = match e.frame {
                        WsFrame::Text => "text",
                        WsFrame::Binary => "binary",
                        WsFrame::Ping => "ping",
                        WsFrame::Pong => "pong",
                        WsFrame::Close => "close",
                        WsFrame::Info => "info",
                        WsFrame::Error => "error",
                    };
                    let preview: String = e
                        .text
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .chars()
                        .take(200)
                        .collect();
                    h_flex()
                        .id(SharedString::from(format!("ws-row-{key}")))
                        .gap_2()
                        .px_3()
                        .py_0p5()
                        .text_xs()
                        .cursor_pointer()
                        .when(selected == Some(key), |this| {
                            this.bg(colors.element_selected)
                        })
                        .hover(|s| s.bg(colors.element_hover))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.protocols.ws_selected.insert(tab_id, key);
                            cx.notify();
                        }))
                        .child(div().w(px(12.)).text_color(colour).child(arrow))
                        .child(
                            div()
                                .w(px(84.))
                                .flex_none()
                                .text_color(colors.text_muted)
                                .child(
                                    e.at.with_timezone(&chrono::Local)
                                        .format("%H:%M:%S%.3f")
                                        .to_string(),
                                ),
                        )
                        .child(
                            div()
                                .w(px(48.))
                                .flex_none()
                                .text_color(colors.text_muted)
                                .child(frame),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_buffer(cx)
                                .child(preview),
                        )
                        .child(div().flex_none().text_color(colors.text_muted).child(
                            if e.size > 0 {
                                human_size(e.size as u64)
                            } else {
                                String::new()
                            },
                        ))
                })
                .collect::<Vec<_>>();
        let detail_view = detail.map(|e| {
            let pretty = ferrumapi_domain::text::pretty_json(&e.text).unwrap_or(e.text);
            let field = self.field(
                FieldSpec::new(format!("ws-detail:{tab_id}"))
                    .value(pretty)
                    .multiline(4)
                    .mono()
                    .read_only(),
                |_, _, _, _| {},
                window,
                cx,
            );
            div()
                .flex_none()
                .max_h(px(200.))
                .border_t_1()
                .border_color(colors.border)
                .p_2()
                .child(field)
        });

        v_flex()
            .size_full()
            .child(
                h_flex()
                    .flex_none()
                    .h_9()
                    .gap_3()
                    .px_3()
                    .border_b_1()
                    .border_color(colors.border)
                    .child(div().text_xs().text_color(state_colour).child(state_label))
                    .children(protocol.map(|p| hint(format!("subprotocol {p}"))))
                    .child(div().flex_1())
                    .child(hint(counts))
                    .child(
                        Button::new("ws-clear-log", "Clear")
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(s) = this.state.protocols.ws.get_mut(&tab_id) {
                                    s.log = Default::default();
                                }
                                cx.notify();
                            })),
                    ),
            )
            .child(
                v_flex()
                    .id("ws-log")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .py_1()
                    .children(rows),
            )
            .children(detail_view)
            .into_any_element()
    }
}
