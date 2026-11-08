use crate::commands::background;
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use std::time::Duration;

/// Transient notice in the bottom-right corner; errors stay longer.
#[component]
pub fn NoticeHost() -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let notice = state.notice.read().clone();

    use_effect(move || {
        if let Some(n) = state.notice.read().clone() {
            let services = services.clone();
            spawn(async move {
                let secs = if n.error { 10 } else { 4 };
                let _ = background(&services, tokio::time::sleep(Duration::from_secs(secs))).await;
                if state.notice.peek().as_ref() == Some(&n) {
                    state.notice.set(None);
                }
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
            class: "fixed bottom-10 right-4 z-50 max-w-lg whitespace-pre-wrap rounded-md border px-4 py-3 text-sm shadow-lg select-text {tone}",
            role: "status",
            title: "Click to dismiss",
            onclick: move |_| state.notice.set(None),
            "{n.message}"
        }
    }
}
