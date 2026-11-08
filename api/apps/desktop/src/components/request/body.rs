//! Body mode radio and per-mode editors.

use super::form_data::FormDataTable;
use crate::commands::files::{file_name, pick_file};
use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::widgets::KvTable;
use crate::state::AppState;
use crate::state::kv::{self, RowEdit};
use dioxus::prelude::*;
use ferrumapi_domain::text::pretty_json;
use ferrumapi_domain::{RawLanguage, RequestBody, TabId, VariableSet};

const MODES: [&str; 5] = [
    "none",
    "form-data",
    "x-www-form-urlencoded",
    "raw",
    "binary",
];

fn empty_body(mode: &str) -> RequestBody {
    match mode {
        "form-data" => RequestBody::FormData { fields: Vec::new() },
        "x-www-form-urlencoded" => RequestBody::UrlEncoded { fields: Vec::new() },
        "raw" => RequestBody::Raw {
            language: RawLanguage::Json,
            text: String::new(),
        },
        "binary" => RequestBody::Binary { path: None },
        _ => RequestBody::None,
    }
}

#[component]
pub fn BodyEditor(tab_id: TabId, body: RequestBody, vars: VariableSet, rev: u64) -> Element {
    let state = use_context::<AppState>();
    let mode = body.mode_label();
    let set_body = move |b: RequestBody| state.edit_request(tab_id, |r| r.body = b);

    rsx! {
        div { class: "flex h-full min-h-0 flex-col gap-3",
            div { class: "flex flex-wrap items-center gap-4 text-xs", role: "radiogroup", "aria-label": "Body type",
                for m in MODES {
                    label { key: "{m}", class: "flex cursor-pointer items-center gap-1.5",
                        input {
                            r#type: "radio",
                            name: "body-mode-{tab_id}",
                            checked: m == mode,
                            onchange: move |_| if m != mode { set_body(empty_body(m)) },
                        }
                        "{m}"
                    }
                }
            }
            match body {
                RequestBody::None => rsx! {
                    p { class: "text-xs text-muted-foreground", "This request does not have a body." }
                },
                RequestBody::Raw { language, text } => rsx! { RawEditor { tab_id, language, text, rev } },
                RequestBody::UrlEncoded { fields } => rsx! {
                    KvTable {
                        rows: fields, rev, vars,
                        on_edit: move |(i, edit): (usize, RowEdit)| state.edit_request(tab_id, |r| {
                            if let RequestBody::UrlEncoded { fields } = &mut r.body { kv::apply(fields, i, edit); }
                        }),
                    }
                },
                RequestBody::FormData { fields } => rsx! { FormDataTable { tab_id, fields, vars } },
                RequestBody::Binary { path } => rsx! {
                    div { class: "flex items-center gap-3 text-xs",
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline,
                            onclick: move |_| {
                                spawn(async move {
                                    if let Some(p) = pick_file("Choose a file to send", false).await {
                                        set_body(RequestBody::Binary { path: Some(p.to_string_lossy().into_owned()) });
                                    }
                                });
                            },
                            "Select file…"
                        }
                        match path {
                            Some(p) => rsx! { span { class: "font-mono", title: "{p}", "{file_name(&p)}" } },
                            None => rsx! { span { class: "text-muted-foreground", "No file selected" } },
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn RawEditor(tab_id: TabId, language: RawLanguage, text: String, rev: u64) -> Element {
    let mut state = use_context::<AppState>();
    let t2 = text.clone();
    rsx! {
        div { class: "flex items-center gap-2",
            select {
                class: "h-7 rounded-md border bg-background px-2 text-xs",
                "aria-label": "Raw body language",
                onchange: move |e| {
                    let lang = RawLanguage::ALL.into_iter().find(|l| l.label() == e.value()).unwrap_or_default();
                    state.edit_request(tab_id, |r| {
                        if let RequestBody::Raw { language, .. } = &mut r.body { *language = lang; }
                    });
                },
                for l in RawLanguage::ALL {
                    option { key: "{l.label()}", value: l.label(), selected: l == language, "{l.label()}" }
                }
            }
            if language == RawLanguage::Json {
                Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, class: "h-7 px-2 text-xs",
                    title: "Pretty-print the JSON body",
                    onclick: move |_| match pretty_json(&t2) {
                        Some(pretty) => {
                            state.edit_request(tab_id, |r| {
                                if let RequestBody::Raw { text, .. } = &mut r.body { *text = pretty; }
                            });
                            state.revs.write().body += 1;
                        }
                        None => state.notify("The body is not valid JSON.", true),
                    },
                    "Beautify"
                }
            }
        }
        // Single-item loop so the key re-mounts the uncontrolled textarea.
        for k in [format!("{tab_id}-{rev}")] {
            textarea {
                key: "{k}",
                class: "min-h-40 flex-1 resize-none rounded-md border bg-background p-2 font-mono text-xs leading-5 outline-none focus:border-ring select-text whitespace-pre",
                spellcheck: "false",
                "aria-label": "Raw body",
                initial_value: "{text}",
                oninput: move |e| {
                    let v = e.value();
                    state.edit_request(tab_id, |r| {
                        if let RequestBody::Raw { text, .. } = &mut r.body { *text = v; }
                    });
                },
            }
        }
    }
}
