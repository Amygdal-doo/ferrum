//! Request tab strip: switch, close, new, dirty dot, double-click rename.

use crate::commands::workspace;
use crate::components::method_class;
use crate::state::{AppState, tabs::display_title};
use dioxus::prelude::*;
use ferrumapi_domain::TabId;

#[component]
pub fn TabBar() -> Element {
    let state = use_context::<AppState>();
    let active = *state.active_tab.read();
    let list = state.tabs.read().clone();
    let mut renaming = use_signal(|| None::<TabId>);

    rsx! {
        div { class: "flex h-9 shrink-0 items-end gap-0.5 overflow-x-auto border-b bg-sidenav px-2", role: "tablist",
            for tab in list {
                {
                    let id = tab.id;
                    let selected = active == Some(id);
                    let cls = if selected { "bg-background border-x border-t text-foreground" } else { "text-muted-foreground hover:text-foreground" };
                    let title = display_title(&tab);
                    let editing = *renaming.read() == Some(id);
                    let dirty_title = if tab.origin.is_some() { "Unsaved changes" } else { "Not saved to a collection" };
                    rsx! {
                        div {
                            key: "{id}",
                            class: "group flex h-8 max-w-56 shrink-0 cursor-default items-center gap-1.5 rounded-t-md px-3 text-xs {cls}",
                            role: "tab",
                            "aria-selected": selected,
                            title: "{title}",
                            onclick: move |_| workspace::activate(state, id),
                            ondoubleclick: move |_| renaming.set(Some(id)),
                            span { class: "text-[10px] font-semibold {method_class(tab.request.method)}", "{tab.request.method}" }
                            if editing {
                                input {
                                    class: "h-6 w-36 rounded border bg-background px-1 text-xs select-text",
                                    onmounted: |e: MountedEvent| async move { let _ = e.data().set_focus(true).await; },
                                    "aria-label": "Tab name",
                                    initial_value: "{title}",
                                    onclick: move |e| e.stop_propagation(),
                                    onkeydown: move |e: KeyboardEvent| {
                                        if e.key() == Key::Escape { renaming.set(None); }
                                    },
                                    onchange: move |e| {
                                        let name = e.value();
                                        if *renaming.peek() == Some(id) {
                                            state.update_tab(id, |t| t.title = name.trim().to_owned());
                                            renaming.set(None);
                                        }
                                    },
                                    onblur: move |_| renaming.set(None),
                                }
                            } else {
                                span { class: "truncate", "{title}" }
                            }
                            if tab.dirty {
                                span { class: "size-1.5 shrink-0 rounded-full bg-muted-foreground", title: dirty_title }
                            }
                            button {
                                class: "rounded px-1 opacity-50 hover:bg-accent hover:opacity-100",
                                "aria-label": "Close tab",
                                onclick: move |e| { e.stop_propagation(); workspace::close_tab(state, id); },
                                "×"
                            }
                        }
                    }
                }
            }
            button {
                class: "mb-1 shrink-0 rounded px-2 text-sm text-muted-foreground hover:bg-accent",
                "aria-label": "New tab",
                title: "New request tab",
                onclick: move |_| workspace::new_tab(state),
                "+"
            }
        }
    }
}
