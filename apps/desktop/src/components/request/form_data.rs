//! Multipart form-data rows with a Text / File toggle.

use crate::commands::files::{file_name, pick_file};
use crate::components::widgets::{CELL, CELL_BAD, unresolved_title};
use crate::state::AppState;
use crate::state::kv::{self, RowEdit};
use dioxus::prelude::*;
use ferrumapi_domain::{FormField, FormFieldKind, RequestBody, TabId, VariableSet};

#[component]
pub fn FormDataTable(tab_id: TabId, fields: Vec<FormField>, vars: VariableSet) -> Element {
    let state = use_context::<AppState>();
    let mut local = use_signal(|| 0u64);
    let edit = move |i: usize, e: RowEdit| {
        state.edit_request(tab_id, |r| {
            if let RequestBody::FormData { fields } = &mut r.body {
                kv::apply(fields, i, e);
            }
        });
    };
    let n = fields.len();
    let mut display = fields;
    display.push(FormField::default());

    rsx! {
        table { class: "w-full table-fixed border-collapse text-xs",
            thead {
                tr { class: "text-left text-[11px] text-muted-foreground",
                    th { class: "w-8 border-b px-2 py-1" }
                    th { class: "border-b px-2 py-1 font-medium", "Key" }
                    th { class: "w-20 border-b px-2 py-1 font-medium", "Type" }
                    th { class: "border-b px-2 py-1 font-medium", "Value" }
                    th { class: "w-8 border-b" }
                }
            }
            tbody {
                for (i, row) in display.into_iter().enumerate() {
                    {
                        let blank = i == n;
                        let is_file = row.kind == FormFieldKind::File;
                        let bad = if is_file { Vec::new() } else { vars.unresolved(&row.value) };
                        let val_cls = if bad.is_empty() { CELL } else { CELL_BAD };
                        let dim = if row.enabled || blank { "" } else { "opacity-50" };
                        let shown = if row.value.is_empty() { "Select file…".to_owned() } else { file_name(&row.value) };
                        rsx! {
                            tr { key: "{tab_id}-{local}-{i}", class: "group border-b {dim}",
                                td { class: "px-2 text-center",
                                    if !blank {
                                        input { r#type: "checkbox", "aria-label": "Enabled", checked: row.enabled,
                                            onchange: move |e| edit(i, RowEdit::Enabled(e.checked())) }
                                    }
                                }
                                td { class: "p-0",
                                    input { class: CELL, placeholder: if blank { "Key" } else { "" }, spellcheck: "false",
                                        initial_value: "{row.key}", oninput: move |e| edit(i, RowEdit::Key(e.value())) }
                                }
                                td { class: "px-1",
                                    select { class: "h-6 w-full rounded border bg-background text-xs", "aria-label": "Field type",
                                        onchange: move |e| { edit(i, RowEdit::File(e.value() == "file")); *local.write() += 1; },
                                        option { value: "text", selected: !is_file, "Text" }
                                        option { value: "file", selected: is_file, "File" }
                                    }
                                }
                                td { class: "p-0",
                                    if is_file {
                                        button {
                                            class: "h-7 w-full truncate px-2 text-left font-mono text-xs hover:bg-accent/40",
                                            title: "{row.value}",
                                            onclick: move |_| {
                                                spawn(async move {
                                                    if let Some(p) = pick_file("Choose a file", false).await {
                                                        edit(i, RowEdit::Value(p.to_string_lossy().into_owned()));
                                                    }
                                                });
                                            },
                                            "{shown}"
                                        }
                                    } else {
                                        input { class: val_cls, placeholder: if blank { "Value" } else { "" }, spellcheck: "false",
                                            title: unresolved_title(&bad),
                                            initial_value: "{row.value}", oninput: move |e| edit(i, RowEdit::Value(e.value())) }
                                    }
                                }
                                td { class: "text-center",
                                    if !blank {
                                        button {
                                            class: "invisible rounded px-1 text-muted-foreground hover:bg-accent hover:text-destructive group-hover:visible",
                                            "aria-label": "Delete row",
                                            onclick: move |_| { edit(i, RowEdit::Remove); *local.write() += 1; },
                                            "×"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
