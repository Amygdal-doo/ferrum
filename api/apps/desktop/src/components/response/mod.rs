//! Response viewer: empty / loading / error states and the response itself.

mod body_view;
mod display;
mod search_bar;
mod tables;
mod tests_view;
mod websocket;

pub use display::BodyDisplay;

use crate::app::FerrumApi;
use crate::commands::files;
use crate::components::{status_color, tab_button};
use crate::state::{ResponseView, TabResponse};
use ferrum_ui::badge;
use ferrum_ui::components::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _, Color,
    Label, LabelCommon as _, LabelSize, LoadingLabel, Tooltip, h_flex, v_flex,
};
use ferrumapi_domain::text::{human_duration_ms, human_size};
use ferrumapi_domain::{HttpResponse, TabId};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*};

fn centered(children: Vec<AnyElement>) -> AnyElement {
    v_flex()
        .flex_1()
        .size_full()
        .items_center()
        .justify_center()
        .gap_2()
        .children(children)
        .into_any_element()
}

impl FerrumApi {
    pub fn render_response_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tab_id = self.state.active_tab;
        if let Some(tab) = self.state.active_tab()
            && tab.request.kind == ferrumapi_domain::RequestKind::WebSocket
        {
            let id = tab.id;
            return v_flex()
                .size_full()
                .child(self.render_ws_log(id, window, cx))
                .into_any_element();
        }
        let response = tab_id.and_then(|id| self.state.responses.get(&id).cloned());
        let content = match (tab_id, response) {
            (_, None) | (None, _) => centered(vec![
                Label::new("No response yet")
                    .color(Color::Muted)
                    .into_any_element(),
                Label::new("Enter a URL and press Send (⌘/Ctrl+Enter).")
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .into_any_element(),
            ]),
            (Some(id), Some(TabResponse::Loading { .. })) => centered(vec![
                LoadingLabel::new("Sending request").into_any_element(),
                Button::new("cancel-loading", "Cancel")
                    .style(ButtonStyle::Outlined)
                    .size(ButtonSize::Compact)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.cancel(id);
                        cx.notify();
                    }))
                    .into_any_element(),
            ]),
            (Some(id), Some(TabResponse::Failed(message))) => {
                let error = cx.theme().status().error;
                let detail = self.field(
                    crate::components::fields::FieldSpec::new(format!("response-error:{id}"))
                        .value(message)
                        .read_only()
                        .borderless(),
                    |_, _, _, _| {},
                    window,
                    cx,
                );
                centered(vec![
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(error)
                        .child("Could not get a response")
                        .into_any_element(),
                    div().max_w(gpui::px(576.)).child(detail).into_any_element(),
                ])
            }
            (_, Some(TabResponse::Cancelled)) => centered(vec![
                Label::new("Request cancelled.")
                    .color(Color::Muted)
                    .into_any_element(),
            ]),
            (Some(id), Some(TabResponse::Done(resp))) => {
                self.render_response_view(id, *resp, window, cx)
            }
        };
        v_flex().size_full().child(content).into_any_element()
    }

    fn render_response_view(
        &mut self,
        tab_id: TabId,
        response: HttpResponse,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let report = response.script.clone();
        let current = match self.state.response_view {
            ResponseView::Tests if report.is_none() => ResponseView::Body,
            v => v,
        };
        let mut views = vec![
            (ResponseView::Body, "Body".to_owned()),
            (
                ResponseView::Headers,
                if response.grpc.is_some() {
                    format!(
                        "Metadata ({})",
                        response.headers.len() + response.trailers.len()
                    )
                } else {
                    format!("Headers ({})", response.headers.len())
                },
            ),
            (
                ResponseView::Cookies,
                format!("Cookies ({})", response.cookies.len()),
            ),
        ];
        if let Some(r) = &report {
            let label = if r.tests.is_empty() && r.errors.is_empty() {
                "Tests".to_owned()
            } else {
                format!("Tests ({}/{})", r.passed_count(), r.tests.len())
            };
            views.push((ResponseView::Tests, label));
        }
        let copy_body = response.body_text();
        let save_bytes = response.body.clone();
        let save_name = files::response_file_name(&response);
        let (summary, status_colour) = match &response.grpc {
            Some(g) => {
                let s = cx.theme().status().clone();
                let colour = if g.code == 0 { s.success } else { s.error };
                (format!("gRPC {} {}", g.code, g.name), colour)
            }
            None => (
                format!("{} {}", response.status, response.status_text),
                status_color(response.status, cx),
            ),
        };
        let grpc_message = response
            .grpc
            .as_ref()
            .map(|g| g.message.clone())
            .filter(|m| !m.is_empty());
        let time = human_duration_ms(response.time_ms);
        let size = human_size(response.size_bytes);
        let content = match current {
            ResponseView::Body => self.render_body_view(tab_id, &response, window, cx),
            ResponseView::Headers => {
                let mut rows = response.headers;
                rows.extend(
                    response
                        .trailers
                        .into_iter()
                        .map(|(k, v)| (format!("{k} (trailer)"), v)),
                );
                tables::headers_table(rows, cx)
            }
            ResponseView::Cookies => tables::cookies_table(response.cookies, cx),
            ResponseView::Tests => tests_view::tests_view(&report.unwrap_or_default(), cx),
        };

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
                    .children(views.into_iter().map(|(view, label)| {
                        tab_button(
                            SharedString::from(label.clone()),
                            label,
                            view == current,
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.response_view = view;
                            cx.notify();
                        }))
                    }))
                    .child(div().flex_1())
                    .child(badge(summary, status_colour, cx))
                    .children(grpc_message.map(|m| {
                        div()
                            .id("grpc-message")
                            .max_w(gpui::px(240.))
                            .truncate()
                            .text_xs()
                            .text_color(colors.text_muted)
                            .tooltip(Tooltip::text(m.clone()))
                            .child(m)
                    }))
                    .child(
                        div()
                            .id("response-time")
                            .text_xs()
                            .text_color(colors.text_muted)
                            .tooltip(Tooltip::text("Time"))
                            .child(time),
                    )
                    .child(
                        div()
                            .id("response-size")
                            .text_xs()
                            .text_color(colors.text_muted)
                            .tooltip(Tooltip::text("Size"))
                            .child(size),
                    )
                    .child(
                        Button::new("copy-response", "Copy")
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .tooltip(Tooltip::text("Copy the response body"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.copy_text(copy_body.clone(), cx);
                            })),
                    )
                    .child(
                        Button::new("save-response", "Save…")
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .tooltip(Tooltip::text("Save the response body to a file"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.save_response(save_bytes.clone(), save_name, cx);
                            })),
                    ),
            )
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
            .into_any_element()
    }
}
