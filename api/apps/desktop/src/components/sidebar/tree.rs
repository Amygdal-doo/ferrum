//! Collection tree: collection, folder and request rows.

use super::menu::{ContextMenu, MenuAction, RenameInput};
use crate::commands::{collections as cmd, files, workspace};
use crate::components::method_class;
use crate::state::{AppState, Dialog};
use dioxus::prelude::*;
use ferrumapi_application::AppServices;
use ferrumapi_domain::{Collection, CollectionId, Item, RequestOrigin};
use uuid::Uuid;

const CONTAINER_ACTIONS: [MenuAction; 6] = [
    MenuAction::AddRequest,
    MenuAction::AddFolder,
    MenuAction::Rename,
    MenuAction::Duplicate,
    MenuAction::Export,
    MenuAction::Delete,
];

#[component]
pub fn CollectionNode(collection: Collection) -> Element {
    let state = use_context::<AppState>();
    let cid = collection.id;
    let expanded = state.expanded.read().contains(&cid.0);
    let mut actions = CONTAINER_ACTIONS.to_vec();
    actions.insert(2, MenuAction::Settings);
    rsx! {
        Row {
            uid: cid.0, cid, item: None, depth: 0, name: collection.name.clone(),
            folder: true, expanded, method: None, actions, active: false,
        }
        if expanded {
            for item in collection.items.clone() {
                ItemNode { key: "{item.id()}", cid, item, depth: 1 }
            }
            if collection.items.is_empty() {
                p { class: "py-1 pl-8 text-[11px] text-muted-foreground", "Empty collection" }
            }
        }
    }
}

#[component]
fn ItemNode(cid: CollectionId, item: Item, depth: usize) -> Element {
    let state = use_context::<AppState>();
    let id = item.id();
    match item {
        Item::Folder(f) => {
            let expanded = state.expanded.read().contains(&id.0);
            let mut actions = CONTAINER_ACTIONS.to_vec();
            actions.retain(|a| *a != MenuAction::Export);
            rsx! {
                Row { uid: id.0, cid, item: Some(id), depth, name: f.name.clone(), folder: true, expanded, method: None, actions, active: false }
                if expanded {
                    for child in f.items {
                        ItemNode { key: "{child.id()}", cid, item: child, depth: depth + 1 }
                    }
                }
            }
        }
        Item::Request(r) => {
            let origin = RequestOrigin {
                collection: cid,
                item: id,
            };
            let active = state.active_tab().and_then(|t| t.origin) == Some(origin);
            let actions = vec![
                MenuAction::Open,
                MenuAction::Rename,
                MenuAction::Duplicate,
                MenuAction::Delete,
            ];
            rsx! {
                Row { uid: id.0, cid, item: Some(id), depth, name: r.name.clone(), folder: false, expanded: false, method: Some(r.request.method), actions, active }
            }
        }
    }
}

#[component]
fn Row(
    uid: Uuid,
    cid: CollectionId,
    item: Option<ferrumapi_domain::ItemId>,
    depth: usize,
    name: String,
    folder: bool,
    expanded: bool,
    method: Option<ferrumapi_domain::HttpMethod>,
    actions: Vec<MenuAction>,
    active: bool,
) -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let mut menu = use_signal(|| None::<(f64, f64)>);
    let renaming = *state.renaming.read() == Some(uid);
    let pad = 8 + depth * 14;
    let bg = if active {
        "bg-accent"
    } else {
        "hover:bg-accent/60"
    };
    let weight = if depth == 0 { "font-medium" } else { "" };
    let chevron = if expanded { "▾" } else { "▸" };
    let svc = services.clone();
    let mut open = move || {
        if folder {
            let mut set = state.expanded.write();
            if !set.remove(&uid) {
                set.insert(uid);
            }
        } else if let Some(id) = item
            && let Some(saved) = state
                .collection(cid)
                .and_then(|c| c.find_request(id).cloned())
        {
            let origin = RequestOrigin {
                collection: cid,
                item: id,
            };
            workspace::open_request(state, saved.name, saved.request, Some(origin));
        }
    };
    let rename_name = name.clone();
    let pick = move |a: MenuAction| {
        let svc = svc.clone();
        match a {
            MenuAction::Open => open(),
            MenuAction::AddRequest => cmd::add_request(state, svc, cid, item),
            MenuAction::AddFolder => cmd::add_folder(state, svc, cid, item),
            MenuAction::Settings => state.dialog.set(Some(Dialog::CollectionSettings(cid))),
            MenuAction::Rename => state.renaming.set(Some(uid)),
            MenuAction::Duplicate => cmd::duplicate(state, svc, cid, item),
            MenuAction::Export => {
                if let Some(c) = state.collection(cid) {
                    files::export_collection(state, svc, c);
                }
            }
            MenuAction::Delete => state.dialog.set(Some(Dialog::DeleteItem {
                collection: cid,
                item,
                name: rename_name.clone(),
            })),
        }
    };

    rsx! {
        div {
            class: "group flex h-7 cursor-default items-center gap-1.5 rounded-md pr-1 text-sm {bg}",
            style: "padding-left: {pad}px",
            onclick: move |_| open(),
            oncontextmenu: move |e: MouseEvent| {
                e.prevent_default();
                let p = e.client_coordinates();
                menu.set(Some((p.x, p.y)));
            },
            if folder {
                span { class: "w-3 shrink-0 text-[10px] text-muted-foreground", "{chevron}" }
            } else if let Some(m) = method {
                span { class: "w-10 shrink-0 text-right text-[10px] font-semibold {method_class(m)}", "{m}" }
            }
            if renaming {
                RenameInput { id: uid, value: name.clone(), on_commit: move |n: String| cmd::rename(state, services.clone(), cid, item, n) }
            } else {
                span { class: "min-w-0 flex-1 truncate {weight}", title: "{name}", "{name}" }
                button {
                    class: "invisible shrink-0 rounded px-1 text-muted-foreground hover:bg-accent hover:text-foreground group-hover:visible",
                    "aria-label": "More actions",
                    title: "More actions",
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        let p = e.client_coordinates();
                        menu.set(Some((p.x, p.y)));
                    },
                    "⋯"
                }
            }
        }
        if let Some(pos) = *menu.read() {
            ContextMenu { pos, actions, on_pick: pick, on_close: move |_| menu.set(None) }
        }
    }
}
