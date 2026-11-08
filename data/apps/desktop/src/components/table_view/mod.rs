//! Table browser tab: breadcrumb, refresh, one-line SQL bar, full-height grid.

use crate::commands::{query, schema};
use crate::components::result_grid::ResultsPanel;
use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumdata_application::AppServices;

#[component]
pub fn TableView() -> Element {
    let state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let Some(tab) = state.active_tab() else {
        return rsx! {};
    };
    let Some(table) = tab.table.clone() else {
        return rsx! {};
    };
    let running = state
        .runs
        .read()
        .get(&tab.id)
        .is_some_and(|r| !r.state.is_terminal());
    let id = tab.id;
    let (svc_refresh, svc_run) = (services.clone(), services.clone());

    rsx! {
        div { class: "flex h-full min-h-0 flex-col",
            div { class: "flex shrink-0 items-center gap-3 px-4 pt-3",
                nav { class: "flex items-center gap-2 text-sm", "aria-label": "Breadcrumb",
                    span { class: "text-muted-foreground", "Tables" }
                    if let Some(s) = &table.schema {
                        span { class: "text-muted-foreground", "/" }
                        span { class: "text-muted-foreground", "{s}" }
                    }
                    span { class: "text-muted-foreground", "/" }
                    span { class: "font-medium", "{table.name}" }
                }
                div { class: "flex-1" }
                Button {
                    size: ButtonSize::Sm,
                    variant: ButtonVariant::Outline,
                    disabled: running,
                    title: "Refresh rows",
                    aria_label: "Refresh rows",
                    onclick: move |_| schema::refresh(state, svc_refresh.clone()),
                    "⟳ Refresh"
                }
            }
            div { class: "shrink-0 px-4 py-3",
                // Uncontrolled, keyed per tab (same reason as the SQL editor).
                input {
                    key: "{id}",
                    id: query::EDITOR_ID,
                    class: "h-10 w-full rounded-md border bg-background px-3 font-mono text-[13px] outline-none select-text focus-visible:ring-2 focus-visible:ring-ring/50",
                    spellcheck: "false",
                    "aria-label": "SQL for this table view",
                    initial_value: "{tab.sql}",
                    oninput: move |e| {
                        let sql = e.value();
                        state.update_tab(id, |t| { t.sql = sql; t.unsaved = true; });
                    },
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter {
                            e.prevent_default();
                            spawn(query::run_from_editor(state, svc_run.clone()));
                        }
                    },
                }
            }
            div { class: "min-h-0 flex-1 border-t", ResultsPanel {} }
        }
    }
}
