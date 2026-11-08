//! Left sidebar: Collections | Environments | History.

mod environments;
mod history;
mod menu;
mod tree;

use crate::commands::{collections, files};
use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::state::{AppState, SidebarView};
use dioxus::prelude::*;
use ferrumapi_application::AppServices;

#[component]
pub fn Sidebar() -> Element {
    let mut state = use_context::<AppState>();
    let current = *state.sidebar_view.read();
    let views = [
        (SidebarView::Collections, "Collections"),
        (SidebarView::Environments, "Environments"),
        (SidebarView::History, "History"),
    ];
    rsx! {
        aside { class: "flex h-full flex-col bg-sidenav",
            div { class: "flex h-10 shrink-0 items-center gap-1 border-b px-2", role: "tablist",
                for (v, label) in views {
                    {
                        let cls = if v == current { "bg-background text-foreground shadow-xs" } else { "text-muted-foreground hover:text-foreground" };
                        rsx! {
                            button { key: "{label}", class: "rounded-md px-2 py-1 text-xs font-medium {cls}", role: "tab",
                                "aria-selected": v == current, onclick: move |_| state.sidebar_view.set(v), "{label}" }
                        }
                    }
                }
            }
            div { class: "flex min-h-0 flex-1 flex-col",
                match current {
                    SidebarView::Collections => rsx! { CollectionsView {} },
                    SidebarView::Environments => rsx! { environments::EnvironmentsView {} },
                    SidebarView::History => rsx! { history::HistoryView {} },
                }
            }
        }
    }
}

/// Section header with a title and trailing actions.
#[component]
fn SectionHeader(title: String, children: Element) -> Element {
    rsx! {
        div { class: "flex h-9 shrink-0 items-center gap-1 px-3",
            span { class: "flex-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground", "{title}" }
            {children}
        }
    }
}

#[component]
fn CollectionsView() -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let mut filter = use_signal(String::new);
    let needle = filter.read().to_lowercase();
    let all = state.collections.read().clone();
    let empty = all.is_empty();
    let list: Vec<_> = all
        .into_iter()
        .filter(|c| {
            needle.is_empty()
                || c.name.to_lowercase().contains(&needle)
                || c.requests()
                    .iter()
                    .any(|r| r.name.to_lowercase().contains(&needle))
        })
        .collect();
    let (svc_new, svc_import, svc_empty) = (services.clone(), services.clone(), services.clone());

    rsx! {
        SectionHeader { title: "Collections",
            Button { size: ButtonSize::IconSm, variant: ButtonVariant::Ghost, class: "size-7", aria_label: "Import collection", title: "Import Postman collection",
                onclick: move |_| files::import_collection(state, svc_import.clone()), "⇣" }
            Button { size: ButtonSize::IconSm, variant: ButtonVariant::Ghost, class: "size-7", aria_label: "New collection", title: "New collection",
                onclick: move |_| {
                    let id = collections::create(state, svc_new.clone(), "New Collection");
                    state.renaming.set(Some(id.0));
                },
                "+"
            }
        }
        if !empty {
            div { class: "px-3 pb-2",
                input {
                    class: "h-7 w-full rounded-md border bg-background px-2 text-xs select-text",
                    placeholder: "Filter collections and requests",
                    "aria-label": "Filter collections",
                    value: "{filter}",
                    oninput: move |e| filter.set(e.value()),
                }
            }
        }
        div { class: "min-h-0 flex-1 overflow-y-auto px-2 pb-2",
            if empty {
                div { class: "px-2 py-6 text-center text-xs text-muted-foreground",
                    p { "No collections yet." }
                    div { class: "mt-3 flex justify-center gap-2",
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline,
                            onclick: move |_| { let id = collections::create(state, svc_empty.clone(), "New Collection"); state.renaming.set(Some(id.0)); },
                            "New collection" }
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost,
                            onclick: move |_| files::import_collection(state, services.clone()), "Import…" }
                    }
                }
            }
            for c in list {
                tree::CollectionNode { key: "{c.id}", collection: c }
            }
        }
    }
}
