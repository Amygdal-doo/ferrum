//! "Save to collection": choose a collection or folder and a name.

use super::{Actions, Modal};
use crate::commands::collections;
use crate::components::widgets::FIELD;
use crate::state::{AppState, tabs::display_title};
use dioxus::prelude::*;
use ferrumapi_application::AppServices;
use ferrumapi_domain::{Collection, CollectionId, Item, ItemId, TabId};

/// One choosable destination: (collection, folder, indented label).
type Target = (CollectionId, Option<ItemId>, String);

fn targets(collections: &[Collection]) -> Vec<Target> {
    fn walk(c: CollectionId, items: &[Item], depth: usize, out: &mut Vec<Target>) {
        for it in items {
            if let Item::Folder(f) = it {
                out.push((c, Some(f.id), format!("{}{}", "  ".repeat(depth), f.name)));
                walk(c, &f.items, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    for c in collections {
        out.push((c.id, None, c.name.clone()));
        walk(c.id, &c.items, 1, &mut out);
    }
    out
}

fn target_key(t: &Target) -> String {
    match t.1 {
        Some(f) => format!("{}/{}", t.0, f),
        None => t.0.to_string(),
    }
}

#[component]
pub fn SaveRequestDialog(tab_id: TabId) -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let initial = state
        .tabs
        .peek()
        .iter()
        .find(|t| t.id == tab_id)
        .map(display_title)
        .unwrap_or_default();
    let mut name = use_signal(|| initial.clone());
    let mut new_collection = use_signal(|| "New Collection".to_owned());
    let all = targets(&state.collections.read());
    let first = all.first().map(target_key).unwrap_or_default();
    let mut chosen = use_signal(move || first);
    let none = all.is_empty();
    let lookup = all.clone();

    let save = move |_| {
        let n = name.peek().trim().to_owned();
        if n.is_empty() {
            return;
        }
        let key = chosen.peek().clone();
        let target = if none {
            Some((
                collections::create(state, services.clone(), new_collection.peek().trim()),
                None,
            ))
        } else {
            lookup
                .iter()
                .find(|t| target_key(t) == key)
                .map(|t| (t.0, t.1))
        };
        if let Some((c, parent)) = target {
            state.dialog.set(None);
            collections::save_tab_as(state, services.clone(), tab_id, c, parent, n);
        }
    };

    rsx! {
        Modal { title: "Save request", on_close: move |_| state.dialog.set(None),
            div { class: "flex flex-col gap-3 text-sm",
                label { class: "flex flex-col gap-1",
                    span { class: "text-xs text-muted-foreground", "Request name" }
                    input { class: FIELD, onmounted: |e: MountedEvent| async move { let _ = e.data().set_focus(true).await; }, initial_value: "{initial}", oninput: move |e| name.set(e.value()) }
                }
                if none {
                    label { class: "flex flex-col gap-1",
                        span { class: "text-xs text-muted-foreground", "No collections yet. Create one named:" }
                        input { class: FIELD, initial_value: "New Collection", oninput: move |e| new_collection.set(e.value()) }
                    }
                } else {
                    div { class: "flex flex-col gap-1",
                        span { class: "text-xs text-muted-foreground", "Save to" }
                        div { class: "max-h-64 overflow-y-auto rounded-md border p-1",
                            for t in all {
                                {
                                    let k = target_key(&t);
                                    let selected = *chosen.read() == k;
                                    let cls = if selected { "bg-accent font-medium" } else { "hover:bg-accent/60" };
                                    let icon = if t.1.is_some() { "▸ " } else { "" };
                                    rsx! {
                                        button { key: "{k}", class: "block w-full whitespace-pre rounded px-2 py-1 text-left text-sm {cls}",
                                            onclick: move |_| chosen.set(k.clone()), "{icon}{t.2}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Actions { confirm: "Save", disabled: name.read().trim().is_empty(), on_confirm: save }
        }
    }
}
