use crate::app::FerrumApi;
use crate::state::TabResponse;
use ferrum_ui::status_bar;
use ferrumapi_domain::text::{human_duration_ms, human_size};
use gpui::{AnyElement, Context, div, prelude::*};
use ui::{Tooltip, h_flex};

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

        status_bar(cx)
            .child(h_flex().gap_4().child(env_label).child(origin_label))
            .child(
                h_flex()
                    .gap_4()
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
