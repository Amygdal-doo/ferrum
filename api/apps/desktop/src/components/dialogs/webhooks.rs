//! Webhook inspector: listener settings and reply, captured requests
//! (newest first) and the selected capture's details.

use super::modal;
use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change};
use crate::components::hint;
use crate::components::widgets::{form_row, header_cell, row_border};
use ferrumapi_domain::text::human_size;
use ferrumapi_domain::webhook::{CapturedRequest, DEFAULT_CAPTURE_LIMIT};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px, relative};
use ui::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonStyle, Clickable as _, Disableable as _,
    Label, LabelCommon as _, LabelSize, StyledTypography as _, TintColor, h_flex, v_flex,
};

fn time_label(c: &CapturedRequest) -> String {
    c.received_at
        .with_timezone(&chrono::Local)
        .format("%H:%M:%S")
        .to_string()
}

impl FerrumApi {
    fn render_capture_row(
        &self,
        c: &CapturedRequest,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let id = c.id;
        h_flex()
            .id(SharedString::from(format!("capture-{id}")))
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .text_xs()
            .cursor_pointer()
            .when(selected, |this| this.bg(colors.element_selected))
            .hover(|s| s.bg(colors.element_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.state.webhooks.selected = Some(id);
                cx.notify();
            }))
            .child(
                div()
                    .w(px(56.))
                    .text_color(colors.text_muted)
                    .child(time_label(c)),
            )
            .child(
                div()
                    .w(px(52.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(c.method.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_buffer(cx)
                    .child(c.target()),
            )
            .child(
                div()
                    .text_color(colors.text_muted)
                    .child(human_size(c.size as u64)),
            )
            .into_any_element()
    }

    fn pairs_table(&self, pairs: Vec<(String, String)>, cx: &mut Context<Self>) -> AnyElement {
        let border = row_border(cx);
        let rows = pairs.into_iter().map(|(k, v)| {
            h_flex()
                .border_b_1()
                .border_color(border)
                .text_xs()
                .font_buffer(cx)
                .child(
                    div()
                        .w(relative(0.35))
                        .min_w_0()
                        .px_2()
                        .py_0p5()
                        .truncate()
                        .child(k),
                )
                .child(div().flex_1().min_w_0().px_2().py_0p5().child(v))
        });
        v_flex()
            .child(
                h_flex()
                    .border_b_1()
                    .border_color(border)
                    .child(div().w(relative(0.35)).child(header_cell("Name")))
                    .child(div().flex_1().child(header_cell("Value"))),
            )
            .children(rows)
            .into_any_element()
    }

    fn render_capture_detail(
        &mut self,
        c: CapturedRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = c.id;
        let size = if c.truncated() {
            format!(
                "{} (first {} kept)",
                human_size(c.size as u64),
                human_size(c.body.len() as u64)
            )
        } else {
            human_size(c.size as u64)
        };
        let body_text = c.display_body();
        let body = self.field(
            FieldSpec::new(format!("capture-body:{id}"))
                .value(body_text.clone())
                .multiline(8)
                .mono()
                .read_only(),
            |_, _, _, _| {},
            window,
            cx,
        );
        let query = c.query_pairs();
        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Label::new(format!("{} {}", c.method, c.target())).size(LabelSize::Default),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("capture-open", "Open as request")
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_capture(id);
                                cx.notify();
                            })),
                    ),
            )
            .child(hint(format!(
                "Received {} · {size}",
                c.received_at
                    .with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M:%S")
            )))
            .when(!query.is_empty(), |this| {
                this.child(Label::new("Query").size(LabelSize::Small))
                    .child(self.pairs_table(query, cx))
            })
            .child(Label::new("Headers").size(LabelSize::Small))
            .child(self.pairs_table(c.headers.clone(), cx))
            .child(Label::new("Body").size(LabelSize::Small))
            .child(if body_text.is_empty() {
                hint("(empty)").into_any_element()
            } else {
                body.into_any_element()
            })
            .into_any_element()
    }

    pub(super) fn render_webhooks_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let status = cx.theme().status().clone();
        let w = &self.state.webhooks;
        let listening = w.inspector.as_ref().map(|i| i.base_url());
        let (starting, error) = (w.starting, w.error.clone());
        let captures = w.captures.clone();
        let current = w.current().cloned();
        let (port_text, status_text, ct, body) = (
            w.port_text.clone(),
            w.status_text.clone(),
            w.content_type.clone(),
            w.body.clone(),
        );

        let port = self.field(
            FieldSpec::new("webhook-port").value(port_text.clone()),
            on_change(|this, text, _| this.state.webhooks.port_text = text),
            window,
            cx,
        );
        let reply_status = self.field(
            FieldSpec::new("webhook-status").value(status_text),
            on_change(|this, text, _| this.edit_webhook_reply(|w| w.status_text = text)),
            window,
            cx,
        );
        let reply_ct = self.field(
            FieldSpec::new("webhook-ct")
                .value(ct)
                .placeholder("Content-Type"),
            on_change(|this, text, _| this.edit_webhook_reply(|w| w.content_type = text)),
            window,
            cx,
        );
        let reply_body = self.field(
            FieldSpec::new("webhook-body")
                .value(body)
                .placeholder("Response body")
                .mono(),
            on_change(|this, text, _| this.edit_webhook_reply(|w| w.body = text)),
            window,
            cx,
        );
        let toggle = if listening.is_some() || starting {
            Button::new("webhook-stop", "Stop")
                .style(ButtonStyle::Tinted(TintColor::Error))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.stop_webhooks();
                    cx.notify();
                }))
        } else {
            Button::new("webhook-start", "Start")
                .style(ButtonStyle::Filled)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.start_webhooks(cx);
                    cx.notify();
                }))
        };
        let (state_text, state_color) = match (&listening, starting, &error) {
            (_, _, Some(e)) => (e.clone(), status.error),
            (Some(url), _, _) => (format!("Listening at {url} (any path)"), status.success),
            (None, true, _) => ("Starting…".into(), colors.text_muted),
            (None, false, None) => ("Stopped".into(), colors.text_muted),
        };
        let port_hint = port_text.trim().to_owned();
        let selected = current.as_ref().map(|c| c.id);
        let list: Vec<AnyElement> = captures
            .iter()
            .map(|c| self.render_capture_row(c, Some(c.id) == selected, cx))
            .collect();
        let detail = match current {
            Some(c) => self.render_capture_detail(c, window, cx),
            None => {
                hint("Nothing captured yet. Send a request to the listener URL.").into_any_element()
            }
        };
        let empty = captures.is_empty();

        modal("Webhook inspector", px(1040.), cx)
            .child(
                h_flex()
                    .gap_3()
                    .child(Label::new("Port").size(LabelSize::Small))
                    .child(div().w(px(80.)).child(port))
                    .child(toggle)
                    .child(div().flex_1().text_sm().text_color(state_color).child(state_text)),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(div().w(px(220.)).child(form_row("Reply status", reply_status)))
                    .child(div().w(px(300.)).child(reply_ct))
                    .child(div().flex_1().child(reply_body)),
            )
            .child(hint(format!(
                "Listens on 127.0.0.1 only, so only this machine can reach it. To receive webhooks from the internet, run a tunnel to it — e.g. `cloudflared tunnel --url http://127.0.0.1:{port_hint}` or `ngrok http {port_hint}` — and give the provider the tunnel URL. The last {DEFAULT_CAPTURE_LIMIT} requests are kept in memory only."
            )))
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(
                        v_flex()
                            .id("webhook-captures")
                            .w(px(380.))
                            .flex_none()
                            .max_h(px(520.))
                            .overflow_y_scroll()
                            .rounded_md()
                            .border_1()
                            .border_color(colors.border_variant)
                            .p_1()
                            .when(empty, |this| this.child(hint("No requests yet.")))
                            .children(list),
                    )
                    .child(div().flex_1().min_w_0().child(detail)),
            )
            .footer(
                Button::new("webhook-clear", "Clear")
                    .style(ButtonStyle::Subtle)
                    .disabled(empty)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.clear_webhooks();
                        cx.notify();
                    })),
            )
            .footer(
                Button::new("webhook-close", "Close")
                    .style(ButtonStyle::Filled)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.dialog = None;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}
