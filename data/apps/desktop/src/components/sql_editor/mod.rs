//! Interim SQL editor: tab bar + native textarea with a line-number gutter.
//! The IDE-grade editor is gated on the ADR-003 spike.

use crate::commands::{query, workspace};
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumdata_application::AppServices;

/// `tabs_only` renders just the tab bar (used above table-browser tabs).
#[component]
pub fn EditorPane(tabs_only: bool) -> Element {
    if tabs_only {
        return rsx! { TabBar {} };
    }
    rsx! {
        div { class: "flex h-full min-h-0 flex-col",
            TabBar {}
            Editor {}
        }
    }
}

#[component]
fn TabBar() -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let active = *state.active_tab.read();
    let tabs = state.tabs.read().clone();
    let svc_new = services.clone();

    rsx! {
        div { class: "flex h-9 shrink-0 items-end gap-0.5 overflow-x-auto border-b bg-sidenav px-2", role: "tablist",
            for tab in tabs {
                {
                    let id = tab.id;
                    let selected = active == Some(id);
                    let cls = if selected { "bg-background border-x border-t text-foreground" } else { "text-muted-foreground hover:text-foreground" };
                    let svc = services.clone();
                    rsx! {
                        div {
                            key: "{id}",
                            class: "group flex h-8 cursor-default items-center gap-2 rounded-t-md px-3 text-xs {cls}",
                            role: "tab",
                            "aria-selected": selected,
                            onclick: move |_| state.active_tab.set(Some(id)),
                            if tab.table.is_some() {
                                span { class: "text-[10px] text-muted-foreground", "▦" }
                            }
                            span { "{tab.title}" }
                            if tab.unsaved {
                                span { class: "size-1.5 rounded-full bg-muted-foreground", title: "Not yet saved locally" }
                            }
                            button {
                                class: "rounded px-1 opacity-50 hover:bg-accent hover:opacity-100",
                                "aria-label": "Close tab",
                                onclick: move |e| { e.stop_propagation(); workspace::close_tab(state, svc.clone(), id); },
                                "×"
                            }
                        }
                    }
                }
            }
            button {
                class: "mb-1 rounded px-2 text-sm text-muted-foreground hover:bg-accent",
                "aria-label": "New tab",
                title: "New tab",
                onclick: move |_| workspace::new_tab(state, svc_new.clone()),
                "+"
            }
        }
    }
}

#[component]
fn Editor() -> Element {
    let state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let mut gutter_offset = use_signal(|| 0.0f64);
    let Some(tab) = state.active_tab() else {
        return rsx! { div { class: "flex-1" } };
    };
    let id = tab.id;
    let lines = tab.sql.lines().count().max(1) + usize::from(tab.sql.ends_with('\n'));
    let numbers: String = (1..=lines).map(|n| format!("{n}\n")).collect();

    rsx! {
        div { class: "relative flex min-h-0 flex-1 overflow-hidden bg-background font-mono text-[13px] leading-5",
            pre {
                class: "pointer-events-none w-12 shrink-0 select-none overflow-hidden border-r bg-sidenav py-2 pr-2 text-right text-muted-foreground",
                "aria-hidden": "true",
                div { style: "transform: translateY(-{gutter_offset}px)", "{numbers}" }
            }
            // Uncontrolled textarea keyed per tab: avoids the controlled-input
            // race where a stale re-render overwrites fast typing.
            textarea {
                key: "{id}",
                id: query::EDITOR_ID,
                class: "min-h-0 flex-1 resize-none bg-transparent p-2 outline-none select-text whitespace-pre",
                spellcheck: "false",
                autofocus: true,
                "aria-label": "SQL editor",
                placeholder: "SELECT * FROM … ;  Run with ⌘/Ctrl+Enter",
                initial_value: "{tab.sql}",
                oninput: move |e| {
                    let sql = e.value();
                    state.update_tab(id, |t| { t.sql = sql; t.unsaved = true; });
                },
                onscroll: move |e| gutter_offset.set(e.data().scroll_top()),
                onkeydown: move |e: KeyboardEvent| {
                    let m = e.modifiers();
                    if e.key() == Key::Enter && (m.meta() || m.ctrl()) {
                        e.prevent_default();
                        spawn(query::run_from_editor(state, services.clone()));
                    }
                },
            }
        }
    }
}
