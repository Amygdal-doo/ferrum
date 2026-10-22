//! Code snippet for the request (variables substituted, auth resolved).

use super::Modal;
use crate::commands::files;
use crate::components::rust_ui::button::{Button, ButtonVariant};
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumapi_application::code_snippet;
use ferrumapi_application::interop::codegen::CodeTarget;
use ferrumapi_domain::TabId;

#[component]
pub fn CodeDialog(tab_id: TabId) -> Element {
    let mut state = use_context::<AppState>();
    let mut target = use_signal(|| CodeTarget::Curl);
    let Some(tab) = state.tabs.read().iter().find(|t| t.id == tab_id).cloned() else {
        return rsx! {};
    };
    let current = *target.read();
    let snippet = code_snippet(current, &state.prepare_tab(&tab));
    let shown = if snippet.is_empty() {
        "// No snippet could be generated for this request.".to_owned()
    } else {
        snippet.clone()
    };

    rsx! {
        Modal { title: "Code snippet", width: "max-w-3xl", on_close: move |_| state.dialog.set(None),
            div { class: "flex min-h-0 flex-col gap-3",
                div { class: "flex items-center gap-2",
                    select {
                        class: "h-8 rounded-md border bg-background px-2 text-sm",
                        "aria-label": "Language",
                        onchange: move |e| {
                            if let Some(t) = CodeTarget::ALL.into_iter().find(|t| t.label() == e.value()) {
                                target.set(t);
                            }
                        },
                        for t in CodeTarget::ALL {
                            option { key: "{t.label()}", value: t.label(), selected: t == current, "{t.label()}" }
                        }
                    }
                    div { class: "flex-1" }
                    Button { variant: ButtonVariant::Outline, disabled: snippet.is_empty(),
                        onclick: move |_| files::copy_text(state, snippet.clone()), "Copy" }
                }
                p { class: "text-xs text-warning-dark", "Variables are substituted, so the snippet may contain secrets." }
                pre { class: "max-h-[60vh] min-h-48 overflow-auto rounded-md border bg-muted p-3 font-mono text-xs leading-5 select-text whitespace-pre", "{shown}" }
            }
            div { class: "mt-4 flex justify-end",
                Button { variant: ButtonVariant::Ghost, onclick: move |_| state.dialog.set(None), "Close" }
            }
        }
    }
}
