//! Method select, URL input with variable highlighting, Send / Cancel,
//! Save and Code.

use crate::commands::{collections, send};
use crate::components::method_class;
use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::rust_ui::kbd::Kbd;
use crate::state::{AppState, Dialog};
use dioxus::prelude::*;
use ferrumapi_application::AppServices;
use ferrumapi_domain::urlparams::params_from_url;
use ferrumapi_domain::vars::Segment;
use ferrumapi_domain::{HttpMethod, RequestTab, VariableSet};
use std::collections::HashSet;

#[component]
pub fn UrlBar(tab: RequestTab, vars: VariableSet, secrets: Vec<String>, loading: bool) -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let id = tab.id;
    let method = tab.request.method;
    let url_rev = state.revs.read().url;
    let svc_send = services.clone();
    let svc_save = services.clone();
    let segments = vars.segments(&tab.request.url);
    let has_vars = segments.iter().any(|s| matches!(s, Segment::Var { .. }));

    rsx! {
        div { class: "flex flex-col gap-1 px-3 pt-3",
            div { class: "flex items-center gap-2",
                div { class: "flex h-9 min-w-0 flex-1 items-center rounded-md border bg-background focus-within:border-ring",
                    select {
                        class: "h-full w-28 shrink-0 rounded-l-md border-r bg-transparent px-2 text-sm font-semibold {method_class(method)}",
                        "aria-label": "HTTP method",
                        onchange: move |e| {
                            if let Some(m) = HttpMethod::parse(&e.value()) {
                                state.edit_request(id, |r| r.method = m);
                            }
                        },
                        for m in HttpMethod::ALL {
                            option { key: "{m}", value: "{m}", selected: m == method, class: method_class(m), "{m}" }
                        }
                    }
                    for k in [format!("{id}-{url_rev}")] {
                        input {
                            key: "{k}",
                            class: "h-full min-w-0 flex-1 bg-transparent px-3 font-mono text-sm outline-none select-text",
                            placeholder: "Enter URL, e.g. https://api.example.com/users",
                            "aria-label": "Request URL",
                            spellcheck: "false",
                            autofocus: true,
                            initial_value: "{tab.request.url}",
                            oninput: move |e| {
                                let url = e.value();
                                state.edit_request(id, |r| {
                                    r.params = params_from_url(&url, &r.params);
                                    r.url = url;
                                });
                                state.revs.write().params += 1;
                            },
                        }
                    }
                }
                if loading {
                    Button { size: ButtonSize::Default, variant: ButtonVariant::Outline, class: "w-24",
                        title: "Cancel the request (⌘/Ctrl+Enter)",
                        onclick: move |_| send::cancel(state, id),
                        "Cancel"
                    }
                } else {
                    Button { size: ButtonSize::Default, class: "w-24",
                        title: "Send (⌘/Ctrl+Enter)",
                        onclick: move |_| send::send(state, svc_send.clone(), id),
                        "Send"
                        Kbd { class: "bg-primary-foreground/20 text-primary-foreground", "⌘↵" }
                    }
                }
                Button { variant: ButtonVariant::Outline, title: "Save (⌘/Ctrl+S)",
                    onclick: move |_| collections::save_tab(state, svc_save.clone(), id),
                    "Save"
                }
                Button { variant: ButtonVariant::Ghost, title: "Generate code",
                    onclick: move |_| state.dialog.set(Some(Dialog::Code(id))),
                    "</>"
                }
            }
            if has_vars {
                VarStrip { segments, vars, secrets }
            }
        }
    }
}

/// The URL split into text and `{{variables}}`: resolved in the info colour,
/// unresolved in the destructive colour. Hover shows the value (secrets masked).
#[component]
fn VarStrip(segments: Vec<Segment>, vars: VariableSet, secrets: Vec<String>) -> Element {
    let secret: HashSet<String> = secrets.into_iter().collect();
    rsx! {
        div { class: "truncate pl-28 font-mono text-[11px] text-muted-foreground", "aria-label": "URL variables",
            for (i, seg) in segments.into_iter().enumerate() {
                match seg {
                    Segment::Text(t) => rsx! { span { key: "{i}", "{t}" } },
                    Segment::Var { name, resolved: true } => {
                        let value = if secret.contains(&name) { "••••••".to_owned() } else { vars.get(&name).unwrap_or_default().to_owned() };
                        rsx! { span { key: "{i}", class: "rounded bg-info-light px-0.5 text-info-dark", title: "{name} = {value}", "{{{{{name}}}}}" } }
                    }
                    Segment::Var { name, resolved: false } => rsx! {
                        span { key: "{i}", class: "rounded bg-destructive/15 px-0.5 text-destructive", title: "{name} is not defined in the active environment, collection or globals", "{{{{{name}}}}}" }
                    },
                }
            }
        }
    }
}
