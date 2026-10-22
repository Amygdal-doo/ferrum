use crate::state::{AppState, TabResponse};
use dioxus::prelude::*;
use ferrumapi_domain::text::{human_duration_ms, human_size};

#[component]
pub fn StatusBar() -> Element {
    let state = use_context::<AppState>();
    let env_label = state.active_environment().map_or_else(
        || "No environment".to_owned(),
        |e| format!("Env: {}", e.name),
    );
    let collections = state.collections.read().len();
    let tab = state.active_tab();
    let origin_label = tab
        .as_ref()
        .and_then(|t| t.origin)
        .and_then(|o| state.collection(o.collection))
        .map(|c| format!("In “{}”", c.name))
        .unwrap_or_else(|| "Unsaved request".to_owned());
    let response = tab
        .as_ref()
        .and_then(|t| state.responses.read().get(&t.id).cloned());
    let run_label = match response {
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

    rsx! {
        footer { class: "flex h-7 shrink-0 items-center gap-4 border-t bg-sidenav px-3 text-xs text-muted-foreground",
            span { "{env_label}" }
            span { "{origin_label}" }
            div { class: "flex-1" }
            if !run_label.is_empty() { span { "{run_label}" } }
            span { "{collections} collections" }
            span { title: "All data is stored locally", "Local" }
        }
    }
}
