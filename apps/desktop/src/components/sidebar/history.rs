//! History grouped by day; click to reopen in a new tab.

use super::SectionHeader;
use crate::commands::{history as cmd, workspace};
use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::{method_class, status_class};
use crate::state::history::{group_by_local_day, time_of_day};
use crate::state::{AppState, Dialog, tabs};
use dioxus::prelude::*;
use ferrumapi_application::AppServices;
use ferrumapi_domain::HistoryEntry;
use ferrumapi_domain::text::human_duration_ms;

#[component]
pub fn HistoryView() -> Element {
    let mut state = use_context::<AppState>();
    let entries = state.history.read().clone();
    let empty = entries.is_empty();
    let groups = group_by_local_day(&entries);

    rsx! {
        SectionHeader { title: "History",
            if !empty {
                Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, class: "h-7 px-2 text-xs",
                    title: "Clear all history",
                    onclick: move |_| state.dialog.set(Some(Dialog::ClearHistory)),
                    "Clear"
                }
            }
        }
        div { class: "min-h-0 flex-1 overflow-y-auto px-2 pb-2",
            if empty {
                p { class: "px-2 py-6 text-center text-xs text-muted-foreground", "Requests you send appear here." }
            }
            for g in groups {
                div { key: "{g.label}", class: "mb-2",
                    div { class: "sticky top-0 bg-sidenav px-2 py-1 text-[11px] font-semibold text-muted-foreground", "{g.label}" }
                    for e in g.entries {
                        HistoryRow { key: "{e.id}", entry: e }
                    }
                }
            }
        }
    }
}

#[component]
fn HistoryRow(entry: HistoryEntry) -> Element {
    let state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let id = entry.id;
    let m = entry.request.method;
    let label = entry.request.label();
    let (status, status_cls) = match (entry.status, &entry.error) {
        (Some(s), _) => (s.to_string(), status_class(s)),
        (None, Some(_)) => ("ERR".to_owned(), "text-destructive"),
        (None, None) => (String::new(), ""),
    };
    let meta = format!(
        "{}{}",
        time_of_day(entry.sent_at),
        entry
            .time_ms
            .map(|t| format!(" · {}", human_duration_ms(t)))
            .unwrap_or_default()
    );
    let tooltip = entry.error.clone().unwrap_or_else(|| label.clone());
    let reopen = entry.clone();

    rsx! {
        div {
            class: "group flex cursor-default items-center gap-2 rounded-md px-2 py-1 hover:bg-accent/60",
            title: "{tooltip}",
            onclick: move |_| {
                // Always a new tab so the history snapshot never overwrites an open one.
                let mut tab = tabs::blank_tab();
                tab.request = reopen.request.clone();
                tab.origin = reopen.origin.filter(|o| tabs::find_by_origin(&state.tabs.peek(), *o).is_none());
                tab.dirty = tab.origin.is_some();
                let tid = tab.id;
                let mut list = state.tabs;
                list.write().push(tab);
                workspace::activate(state, tid);
            },
            span { class: "w-11 shrink-0 text-[10px] font-semibold {method_class(m)}", "{m}" }
            div { class: "flex min-w-0 flex-1 flex-col",
                span { class: "truncate font-mono text-xs", "{label}" }
                span { class: "text-[10px] text-muted-foreground", "{meta}" }
            }
            span { class: "shrink-0 text-[10px] font-semibold {status_cls}", "{status}" }
            button {
                class: "invisible shrink-0 rounded px-1 text-muted-foreground hover:bg-accent hover:text-destructive group-hover:visible",
                "aria-label": "Delete history entry",
                onclick: move |e: MouseEvent| { e.stop_propagation(); cmd::delete(state, services.clone(), id); },
                "×"
            }
        }
    }
}
