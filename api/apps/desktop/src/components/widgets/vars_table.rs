//! Variable table (environments, globals, collection variables): enabled,
//! key, value (masked when secret) and a secret toggle.

use super::CELL;
use crate::state::kv::RowEdit;
use dioxus::prelude::*;
use ferrumapi_domain::Variable;

#[component]
pub fn VarsTable(rows: Vec<Variable>, on_edit: EventHandler<(usize, RowEdit)>) -> Element {
    let mut local = use_signal(|| 0u64);
    let mut revealed = use_signal(|| None::<usize>);
    let n = rows.len();
    let mut display = rows;
    display.push(Variable::default());

    rsx! {
        table { class: "w-full table-fixed border-collapse text-xs",
            thead {
                tr { class: "text-left text-[11px] text-muted-foreground",
                    th { class: "w-8 border-b px-2 py-1 font-medium" }
                    th { class: "border-b px-2 py-1 font-medium", "Variable" }
                    th { class: "border-b px-2 py-1 font-medium", "Value" }
                    th { class: "w-16 border-b px-2 py-1 font-medium", "Secret" }
                    th { class: "w-8 border-b" }
                }
            }
            tbody {
                for (i, row) in display.into_iter().enumerate() {
                    {
                        let blank = i == n;
                        let masked = row.secret && *revealed.read() != Some(i);
                        let kind = if masked { "password" } else { "text" };
                        let dim = if row.enabled || blank { "" } else { "opacity-50" };
                        rsx! {
                            tr { key: "{local}-{i}", class: "group border-b {dim}",
                                td { class: "px-2 text-center",
                                    if !blank {
                                        input { r#type: "checkbox", "aria-label": "Enabled", checked: row.enabled,
                                            onchange: move |e| on_edit.call((i, RowEdit::Enabled(e.checked()))) }
                                    }
                                }
                                td { class: "p-0",
                                    input { class: CELL, placeholder: if blank { "Add variable" } else { "" },
                                        spellcheck: "false", initial_value: "{row.key}",
                                        oninput: move |e| on_edit.call((i, RowEdit::Key(e.value()))) }
                                }
                                td { class: "p-0",
                                    input { class: CELL, r#type: kind, autocomplete: "off",
                                        spellcheck: "false", initial_value: "{row.value}",
                                        onfocus: move |_| revealed.set(Some(i)),
                                        onblur: move |_| revealed.set(None),
                                        oninput: move |e| on_edit.call((i, RowEdit::Value(e.value()))) }
                                }
                                td { class: "px-2 text-center",
                                    if !blank {
                                        input { r#type: "checkbox", "aria-label": "Secret", checked: row.secret,
                                            title: "Mask this value",
                                            onchange: move |e| on_edit.call((i, RowEdit::Secret(e.checked()))) }
                                    }
                                }
                                td { class: "text-center",
                                    if !blank {
                                        button {
                                            class: "invisible rounded px-1 text-muted-foreground hover:bg-accent hover:text-destructive group-hover:visible",
                                            "aria-label": "Delete variable",
                                            onclick: move |_| { on_edit.call((i, RowEdit::Remove)); *local.write() += 1; },
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
