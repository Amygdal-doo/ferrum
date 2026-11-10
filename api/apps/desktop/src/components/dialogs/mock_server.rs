//! Mock server of a collection: port + Start/Stop, the collection's
//! requests with their mock responses, and the live request log.

use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change};
use crate::components::{count_pill, hint, method_label, status_color};
use ferrum_ui::Modal;
use ferrumapi_application::variables_for;
use ferrumapi_domain::mock::{MockLogEntry, route_path};
use ferrumapi_domain::{CollectionId, HttpMethod, ItemId, RequestOrigin};
use gpui::{AnyElement, ClickEvent, Context, SharedString, Window, div, prelude::*, px};
use ui::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonStyle, Clickable as _, Disableable as _,
    Label, LabelCommon as _, LabelSize, StyledTypography as _, TintColor, h_flex, v_flex,
};

struct RouteRow {
    item: ItemId,
    name: String,
    method: HttpMethod,
    path: String,
    mocks: usize,
}

impl FerrumApi {
    fn mock_route_rows(&self, collection: CollectionId) -> Vec<RouteRow> {
        let state = &self.state;
        let (Some(c), Some(m)) = (state.collection(collection), state.mocks.get(&collection))
        else {
            return Vec::new();
        };
        c.requests()
            .into_iter()
            .map(|r| {
                let origin = RequestOrigin {
                    collection,
                    item: r.id,
                };
                let vars = variables_for(
                    Some(origin),
                    &state.collections,
                    &state.globals,
                    state.active_environment(),
                );
                RouteRow {
                    item: r.id,
                    name: r.name.clone(),
                    method: r.request.method,
                    path: route_path(&r.request.url, &vars),
                    mocks: m.draft.for_item(r.id).filter(|x| x.enabled).count(),
                }
            })
            .collect()
    }

    fn render_route_row(
        &self,
        collection: CollectionId,
        row: RouteRow,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let item = row.item;
        v_flex()
            .id(SharedString::from(format!("mock-route-{item}")))
            .px_2()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .when(selected, |this| this.bg(colors.element_selected))
            .hover(|s| s.bg(colors.element_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.edit_mocks(collection, |s| s.selected = Some(item));
                cx.notify();
            }))
            .child(
                h_flex()
                    .gap_2()
                    .child(method_label(row.method, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .child(row.name),
                    )
                    .when(row.mocks > 0, |this| this.child(count_pill(row.mocks, cx))),
            )
            .child(
                div()
                    .text_xs()
                    .font_buffer(cx)
                    .text_color(colors.text_muted)
                    .truncate()
                    .child(row.path),
            )
            .into_any_element()
    }

    fn render_mock_log(&self, log: Vec<MockLogEntry>, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let rows = log.into_iter().map(|e| {
            let matched = e
                .matched
                .clone()
                .unwrap_or_else(|| "no match — 404 with route list".into());
            h_flex()
                .gap_3()
                .px_2()
                .text_xs()
                .font_buffer(cx)
                .child(
                    div().w(px(64.)).text_color(colors.text_muted).child(
                        e.at.with_timezone(&chrono::Local)
                            .format("%H:%M:%S")
                            .to_string(),
                    ),
                )
                .child(div().w(px(56.)).child(e.method))
                .child(div().flex_1().min_w_0().truncate().child(e.target))
                .child(
                    div()
                        .w(px(36.))
                        .text_color(status_color(e.status, cx))
                        .child(e.status.to_string()),
                )
                .child(
                    div()
                        .w(px(220.))
                        .truncate()
                        .text_color(colors.text_muted)
                        .child(matched),
                )
        });
        v_flex()
            .id("mock-log")
            .max_h(px(160.))
            .overflow_y_scroll()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(colors.border_variant)
            .children(rows)
            .into_any_element()
    }

    pub(super) fn render_mock_dialog(
        &mut self,
        collection: CollectionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(c) = self.state.collection(collection) else {
            self.state.dialog = None;
            return div().into_any_element();
        };
        let title = format!("Mock server — {}", c.name);
        let rows = self.mock_route_rows(collection);
        let Some(ms) = self.state.mocks.get(&collection) else {
            self.state.dialog = None;
            return div().into_any_element();
        };
        let colors = cx.theme().colors().clone();
        let status_colors = cx.theme().status().clone();
        let (running_url, starting, error) = (ms.running_url(), ms.starting, ms.error.clone());
        let selected = ms.selected;
        let mocks: Vec<_> = selected
            .map(|item| ms.draft.for_item(item).cloned().collect())
            .unwrap_or_default();
        let log: Vec<MockLogEntry> = ms.log.iter().cloned().collect();
        let (port_text, routes_total) = (ms.port_text.clone(), ms.draft.responses.len());

        let port = self.field(
            FieldSpec::new(format!("mock-port:{collection}")).value(port_text),
            on_change(move |this, text, _| this.edit_mocks(collection, |s| s.set_port_text(text))),
            window,
            cx,
        );
        let toggle = if running_url.is_some() || starting {
            Button::new("mock-stop", "Stop")
                .style(ButtonStyle::Tinted(TintColor::Error))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.stop_mock_server(collection);
                    cx.notify();
                }))
        } else {
            Button::new("mock-start", "Start")
                .style(ButtonStyle::Filled)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.start_mock_server(collection, cx);
                    cx.notify();
                }))
        };
        let (status, status_color) = match (&running_url, starting, &error) {
            (_, _, Some(e)) => (e.clone(), status_colors.error),
            (Some(url), _, _) => (format!("Running at {url}"), status_colors.success),
            (None, true, _) => ("Starting…".into(), colors.text_muted),
            (None, false, None) => ("Stopped".into(), colors.text_muted),
        };

        let list = rows
            .into_iter()
            .map(|row| {
                let is_selected = selected == Some(row.item);
                self.render_route_row(collection, row, is_selected, cx)
            })
            .collect::<Vec<_>>();
        let cards: Vec<AnyElement> = mocks
            .into_iter()
            .map(|m| self.render_mock_card(collection, m, window, cx))
            .collect();
        let editor = match selected {
            None => v_flex().child(hint("Save requests into this collection to mock them.")),
            Some(item) => v_flex()
                .gap_2()
                .when(cards.is_empty(), |this| {
                    this.child(hint(
                        "No mock responses yet: requests to this route get the 404 route list.",
                    ))
                })
                .children(cards)
                .child(
                    Button::new("mock-add", "Add response")
                        .style(ButtonStyle::Outlined)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_mocks(collection, |s| {
                                s.add_mock(item);
                            });
                            cx.notify();
                        })),
                ),
        };

        Modal::new(title)
            .width(px(1040.))
            .on_dismiss(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.close_mock_dialog(collection, cx);
                cx.notify();
            }))
            .child(
                h_flex()
                    .gap_3()
                    .child(Label::new("Port").size(LabelSize::Small))
                    .child(div().w(px(80.)).child(port))
                    .child(toggle)
                    .child(div().flex_1().text_sm().text_color(status_color).child(status)),
            )
            .child(hint(format!(
                "Listens on 127.0.0.1 only. Routes come from each saved request's method and URL path (a leading {{{{baseUrl}}}} or scheme://host is dropped; :id, {{id}} and {{{{id}}}} segments match anything). Point your app at the server's URL. {routes_total} responses defined; edits apply immediately."
            )))
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(
                        v_flex()
                            .id("mock-routes")
                            .w(px(280.))
                            .flex_none()
                            .max_h(px(520.))
                            .overflow_y_scroll()
                            .children(list),
                    )
                    .child(div().flex_1().min_w_0().child(editor)),
            )
            .child(Label::new("Log").size(LabelSize::Small))
            .child(if log.is_empty() {
                hint("Requests to the mock server appear here.").into_any_element()
            } else {
                self.render_mock_log(log, cx)
            })
            .footer(
                Button::new("mock-save", "Save")
                    .style(ButtonStyle::Subtle)
                    .disabled(!self.state.mocks.get(&collection).is_some_and(|s| s.dirty))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.save_mocks(collection, cx);
                        cx.notify();
                    })),
            )
            .footer(
                Button::new("mock-close", "Close")
                    .style(ButtonStyle::Filled)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.close_mock_dialog(collection, cx);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}
