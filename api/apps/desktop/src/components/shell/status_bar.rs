use crate::app::FerrumApi;
use crate::state::{Dialog, TabResponse};
use ferrum_ui::components::{Tooltip, h_flex};
use ferrum_ui::status_bar;
use ferrumapi_domain::text::{human_duration_ms, human_size};
use gpui::{AnyElement, Context, div, prelude::*};

impl FerrumApi {
    pub fn render_status_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = &self.state;
        let env_label = state.active_environment().map_or_else(
            || "No environment".to_owned(),
            |e| format!("Env: {}", e.name),
        );
        let collections = state.collections.len();
        let tab = state.active_tab();
        let origin_label = tab
            .and_then(|t| t.origin)
            .and_then(|o| state.collection(o.collection))
            .map_or_else(
                || "Unsaved request".to_owned(),
                |c| format!("In “{}”", c.name),
            );
        let run_label = match tab.and_then(|t| state.responses.get(&t.id)) {
            None => String::new(),
            Some(TabResponse::Loading { .. }) => "Sending…".into(),
            Some(TabResponse::Done(r)) => format!(
                "{} · {} · {}",
                r.status,
                human_duration_ms(r.time_ms),
                human_size(r.size_bytes)
            ),
            Some(TabResponse::Failed(_)) => "Request failed".into(),
            Some(TabResponse::Cancelled) => "Cancelled".into(),
        };

        let runner_label = state.runner.as_ref().map(|r| {
            let counts = r.counts();
            if r.running {
                format!("Running “{}” {}/{}", r.title, counts.done, r.rows.len())
            } else {
                format!("Run: {} passed, {} failed", counts.passed, counts.failed)
            }
        });

        let mocks = self.running_mocks();
        let mock_label = match mocks.as_slice() {
            [] => None,
            [(id, url)] => Some((*id, format!("Mock {url}"))),
            [(id, _), ..] => Some((*id, format!("{} mock servers", mocks.len()))),
        };
        let webhook_label = state.webhooks.inspector.as_ref().map(|i| {
            format!(
                "Webhooks :{} · {}",
                i.addr().port(),
                state.webhooks.captures.len()
            )
        });
        let load_label = state.load_test.as_ref().filter(|l| l.running).map(|l| {
            let done = l.tally.as_ref().map_or(0, |t| t.completed());
            let total = l.config.map_or(0, |c| c.total);
            format!("Load test {done}/{total}")
        });

        status_bar(cx)
            .child(h_flex().gap_4().child(env_label).child(origin_label))
            .child(
                h_flex()
                    .gap_4()
                    .when_some(runner_label, |this, label| {
                        this.child(
                            div()
                                .id("runner-status")
                                .cursor_pointer()
                                .tooltip(Tooltip::text("Show the collection runner"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.state.dialog = Some(Dialog::Runner);
                                    cx.notify();
                                }))
                                .child(label),
                        )
                    })
                    .when_some(mock_label, |this, (id, label)| {
                        this.child(
                            div()
                                .id("mock-status")
                                .cursor_pointer()
                                .tooltip(Tooltip::text("Show the mock server"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open_mock_server(id, cx);
                                    cx.notify();
                                }))
                                .child(label),
                        )
                    })
                    .when_some(webhook_label, |this, label| {
                        this.child(
                            div()
                                .id("webhook-status")
                                .cursor_pointer()
                                .tooltip(Tooltip::text("Show the webhook inspector"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.state.dialog = Some(Dialog::Webhooks);
                                    cx.notify();
                                }))
                                .child(label),
                        )
                    })
                    .when_some(load_label, |this, label| {
                        this.child(
                            div()
                                .id("load-status")
                                .cursor_pointer()
                                .tooltip(Tooltip::text("Show the load test"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.state.dialog = Some(Dialog::LoadTest);
                                    cx.notify();
                                }))
                                .child(label),
                        )
                    })
                    .when(!run_label.is_empty(), |this| this.child(run_label))
                    .child(format!("{collections} collections"))
                    .child(
                        div()
                            .id("local-storage")
                            .tooltip(Tooltip::text("All data is stored locally"))
                            .child("Local"),
                    ),
            )
            .into_any_element()
    }
}
