use crate::state::{AppState, ConnStatus};
use dioxus::prelude::*;
use ferrum_application::AppServices;

#[component]
pub fn StatusBar() -> Element {
    let state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let profile = state.active_profile();
    let status = profile
        .as_ref()
        .and_then(|p| state.conn_status.read().get(&p.id).cloned());
    let run_label = state
        .active_tab()
        .and_then(|t| state.runs.read().get(&t.id).map(|r| r.status_label()))
        .unwrap_or_default();

    let conn_label = match (&profile, &status) {
        (None, _) => "No connection selected".to_owned(),
        (Some(p), None) => format!("{} · not connected", p.display_name),
        (Some(p), Some(ConnStatus::Connecting)) => format!("{} · connecting…", p.display_name),
        (Some(p), Some(ConnStatus::Ready(v))) => format!("{} · {v}", p.display_name),
        (Some(p), Some(ConnStatus::Failed(_))) => format!("{} · connection failed", p.display_name),
    };
    let flags = profile
        .as_ref()
        .map(|p| {
            let mut f = vec![p.env.label().to_owned(), p.tls.label().to_owned()];
            if p.read_only {
                f.push("Read only".into());
            }
            f.join(" · ")
        })
        .unwrap_or_default();

    rsx! {
        footer { class: "flex h-7 shrink-0 items-center gap-4 border-t bg-sidenav px-3 text-xs text-muted-foreground",
            span { "{conn_label}" }
            if !flags.is_empty() { span { "{flags}" } }
            div { class: "flex-1" }
            if !run_label.is_empty() { span { "{run_label}" } }
            span { title: "Where saved passwords are stored", "Secrets: {services.connections.vault_name()}" }
        }
    }
}
