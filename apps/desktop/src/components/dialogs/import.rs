//! Import a Postman v2.1 collection file or a pasted cURL command.

use super::Modal;
use crate::commands::{files, workspace};
use crate::components::rust_ui::button::{Button, ButtonVariant};
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumapi_application::AppServices;

#[component]
pub fn ImportDialog() -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let mut curl = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let svc_file = services.clone();

    let import_curl = move |_| {
        let text = curl.peek().trim().to_owned();
        if text.is_empty() {
            return;
        }
        match services.import_curl(&text) {
            Ok(req) => {
                state.dialog.set(None);
                workspace::open_request(state, String::new(), req, None);
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    rsx! {
        Modal { title: "Import", on_close: move |_| state.dialog.set(None),
            div { class: "flex flex-col gap-5 text-sm",
                section { class: "flex flex-col gap-2",
                    h3 { class: "text-xs font-semibold uppercase tracking-wide text-muted-foreground", "Postman collection" }
                    p { class: "text-xs text-muted-foreground", "Choose a Postman Collection v2.1 JSON file. It is added to your collections." }
                    Button { variant: ButtonVariant::Outline, class: "w-fit",
                        onclick: move |_| { state.dialog.set(None); files::import_collection(state, svc_file.clone()); },
                        "Choose file…"
                    }
                }
                div { class: "h-px bg-border" }
                section { class: "flex flex-col gap-2",
                    h3 { class: "text-xs font-semibold uppercase tracking-wide text-muted-foreground", "cURL command" }
                    textarea {
                        class: "h-32 resize-none rounded-md border bg-background p-2 font-mono text-xs outline-none focus:border-ring select-text",
                        placeholder: "curl -X POST https://api.example.com/items -H 'Content-Type: application/json' -d '{{\"name\":\"x\"}}'",
                        spellcheck: "false",
                        "aria-label": "cURL command",
                        oninput: move |e| { curl.set(e.value()); error.set(None); },
                    }
                    if let Some(e) = error.read().clone() {
                        p { class: "text-xs text-destructive select-text", "{e}" }
                    }
                    div { class: "flex justify-end gap-2",
                        Button { variant: ButtonVariant::Ghost, onclick: move |_| state.dialog.set(None), "Cancel" }
                        Button { disabled: curl.read().trim().is_empty(), onclick: import_curl, "Import cURL" }
                    }
                }
            }
        }
    }
}
