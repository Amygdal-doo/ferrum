//! Per-request transport settings.

use crate::components::widgets::FIELD;
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumapi_domain::{RequestSettings, TabId};

#[component]
pub fn SettingsEditor(tab_id: TabId, settings: RequestSettings) -> Element {
    let state = use_context::<AppState>();
    rsx! {
        div { class: "flex max-w-xl flex-col gap-4 text-sm",
            label { class: "flex items-center gap-3",
                span { class: "w-40 shrink-0 text-xs text-muted-foreground", "Timeout (ms)" }
                input {
                    class: "{FIELD} w-40",
                    r#type: "number",
                    min: "0",
                    step: "1000",
                    initial_value: "{settings.timeout_ms}",
                    oninput: move |e| {
                        if let Ok(ms) = e.value().trim().parse::<u64>() {
                            state.edit_request(tab_id, |r| r.settings.timeout_ms = ms);
                        }
                    },
                }
                span { class: "text-xs text-muted-foreground", "0 = no timeout" }
            }
            Toggle {
                label: "Follow redirects",
                hint: "Automatically follow 3xx responses.",
                on: settings.follow_redirects,
                on_change: move |v| state.edit_request(tab_id, |r| r.settings.follow_redirects = v),
            }
            Toggle {
                label: "Verify TLS certificates",
                hint: "Turn off only for local servers with self-signed certificates.",
                on: settings.verify_tls,
                on_change: move |v| state.edit_request(tab_id, |r| r.settings.verify_tls = v),
            }
        }
    }
}

#[component]
fn Toggle(label: String, hint: String, on: bool, on_change: EventHandler<bool>) -> Element {
    rsx! {
        label { class: "flex cursor-pointer items-start gap-3",
            input { class: "mt-0.5", r#type: "checkbox", checked: on, onchange: move |e| on_change.call(e.checked()) }
            span { class: "flex flex-col gap-0.5",
                span { class: "text-sm", "{label}" }
                span { class: "text-xs text-muted-foreground", "{hint}" }
            }
        }
    }
}
