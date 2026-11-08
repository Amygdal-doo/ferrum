//! Key / value table (params, headers, urlencoded) with an enable checkbox,
//! delete button and an auto-appended blank row.

use super::{CELL, CELL_BAD, unresolved_title};
use crate::state::kv::RowEdit;
use dioxus::prelude::*;
use ferrumapi_domain::{KeyValue, VariableSet};

/// `rev` re-keys the rows when the owner changed them from elsewhere.
#[component]
pub fn KvTable(
    rows: Vec<KeyValue>,
    rev: u64,
    vars: VariableSet,
    on_edit: EventHandler<(usize, RowEdit)>,
    #[props(default = "Key".to_owned())] key_placeholder: String,
) -> Element {
    let mut local = use_signal(|| 0u64);
    let n = rows.len();
    let mut display = rows;
    display.push(KeyValue::default());

    rsx! {
        table { class: "w-full table-fixed border-collapse text-xs",
            thead {
                tr { class: "text-left text-[11px] text-muted-foreground",
                    th { class: "w-8 border-b px-2 py-1 font-medium" }
                    th { class: "border-b px-2 py-1 font-medium", "Key" }
                    th { class: "border-b px-2 py-1 font-medium", "Value" }
                    th { class: "w-8 border-b" }
                }
            }
            tbody {
                for (i, row) in display.into_iter().enumerate() {
                    {
                        let blank = i == n;
                        let key_bad = vars.unresolved(&row.key);
                        let val_bad = vars.unresolved(&row.value);
                        let key_cls = if key_bad.is_empty() { CELL } else { CELL_BAD };
                        let val_cls = if val_bad.is_empty() { CELL } else { CELL_BAD };
                        let dim = if row.enabled || blank { "" } else { "opacity-50" };
                        rsx! {
                            tr { key: "{rev}-{local}-{i}", class: "group border-b {dim}",
                                td { class: "px-2 text-center",
                                    if !blank {
                                        input {
                                            r#type: "checkbox",
                                            "aria-label": "Enabled",
                                            checked: row.enabled,
                                            onchange: move |e| on_edit.call((i, RowEdit::Enabled(e.checked()))),
                                        }
                                    }
                                }
                                td { class: "p-0",
                                    input {
                                        class: key_cls,
                                        placeholder: if blank { key_placeholder.clone() } else { String::new() },
                                        title: unresolved_title(&key_bad),
                                        spellcheck: "false",
                                        initial_value: "{row.key}",
                                        oninput: move |e| on_edit.call((i, RowEdit::Key(e.value()))),
                                    }
                                }
                                td { class: "p-0",
                                    input {
                                        class: val_cls,
                                        placeholder: if blank { "Value" } else { "" },
                                        title: unresolved_title(&val_bad),
                                        spellcheck: "false",
                                        initial_value: "{row.value}",
                                        oninput: move |e| on_edit.call((i, RowEdit::Value(e.value()))),
                                    }
                                }
                                td { class: "text-center",
                                    if !blank {
                                        button {
                                            class: "invisible rounded px-1 text-muted-foreground hover:bg-accent hover:text-destructive group-hover:visible",
                                            "aria-label": "Delete row",
                                            title: "Delete row",
                                            onclick: move |_| {
                                                on_edit.call((i, RowEdit::Remove));
                                                *local.write() += 1;
                                            },
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
