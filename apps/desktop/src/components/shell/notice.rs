use crate::commands::background;
use crate::state::AppState;
use dioxus::prelude::*;
use ferrum_application::AppServices;
use std::time::Duration;

/// Transient notice in the bottom-right corner; auto-dismisses.
#[component]
pub fn NoticeHost() -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let notice = state.notice.read().clone();

    use_effect(move || {
        if state.notice.read().is_some() {
            let services = services.clone();
            spawn(async move {
                let _ = background(&services, tokio::time::sleep(Duration::from_secs(5))).await;
                state.notice.set(None);
            });
        }
    });

    let Some(n) = notice else { return rsx! {} };
    let tone = if n.error {
        "border-destructive/40 bg-destructive text-destructive-foreground"
    } else {
        "border-border bg-popover text-popover-foreground"
    };
    rsx! {
        div {
            class: "fixed bottom-10 right-4 z-50 max-w-md rounded-md border px-4 py-3 text-sm shadow-lg {tone}",
            role: "status",
            onclick: move |_| state.notice.set(None),
            "{n.message}"
        }
    }
}
