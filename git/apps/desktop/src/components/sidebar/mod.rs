//! Left sidebar (Tower layout): Workspace pages, branches grouped into
//! folders by `/`, tags, remotes; filter and "new branch" at the bottom.

mod tree;

use crate::commands::repo;
use crate::components::actions::{self, Action};
use crate::components::icons::{Icon, Svg};
use crate::components::menu::ContextMenu;
use crate::state::{AppState, Dialog, Page, PromptKind};
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use ferrumgit_domain::{GitRef, RefKind};
use std::collections::HashSet;
use tree::{RefTree, build};

/// Opens a context menu at a position with these actions.
pub type MenuOpener = EventHandler<((f64, f64), Vec<Action>)>;

#[component]
pub fn Sidebar() -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let mut menu = use_signal(|| None::<((f64, f64), Vec<Action>)>);
    let mut filter = use_signal(String::new);
    let collapsed = use_signal(|| HashSet::from(["section:Tags".to_owned()]));
    let Some(id) = *state.active.read() else {
        return rsx! {};
    };
    let view = state.view(id);
    let page = view.page;
    let snap = view
        .data
        .as_ref()
        .map(|d| d.snapshot.clone())
        .unwrap_or_default();
    let needle = filter.read().to_lowercase();
    let keep = |r: &&GitRef| needle.is_empty() || r.name.to_lowercase().contains(&needle);
    let local: Vec<(String, GitRef)> = snap
        .refs
        .iter()
        .filter(|r| r.is(RefKind::LocalBranch))
        .filter(keep)
        .map(|r| (r.name.clone(), r.clone()))
        .collect();
    let tags: Vec<(String, GitRef)> = snap
        .refs
        .iter()
        .filter(|r| r.is(RefKind::Tag))
        .filter(keep)
        .map(|r| (r.name.clone(), r.clone()))
        .collect();
    let remotes: Vec<(String, Vec<(String, GitRef)>)> = snap
        .remotes
        .iter()
        .map(|rm| {
            let prefix = format!("{}/", rm.name);
            let branches = snap
                .refs
                .iter()
                .filter(|r| r.is(RefKind::RemoteBranch))
                .filter(keep)
                .filter_map(|r| {
                    r.name
                        .strip_prefix(&prefix)
                        .map(|n| (n.to_owned(), r.clone()))
                })
                .collect();
            (rm.name.clone(), branches)
        })
        .collect();
    let all_refs = snap.refs.clone();
    let changes = snap.status.files.len();
    let stashes = snap.stashes.len();
    let open_menu: MenuOpener =
        EventHandler::new(move |m: ((f64, f64), Vec<Action>)| menu.set(Some(m)));
    let svc = services.clone();
    let go = move |p: Page| repo::set_page(state, svc.clone(), id, p);
    let (g1, g2, g3) = (go.clone(), go.clone(), go);

    rsx! {
        aside { class: "flex h-full flex-col bg-sidenav",
            div { class: "min-h-0 flex-1 overflow-y-auto px-2 pb-3 text-[13px]",
                SectionTitle { title: "Workspace", key_name: "Workspace", collapsed }
                if !collapsed.read().contains("section:Workspace") {
                    NavItem { icon: Icon::FolderOpen, label: "Working Copy", active: page == Page::WorkingCopy, badge: (changes > 0).then(|| changes.to_string()), onclick: move |_| g1(Page::WorkingCopy) }
                    NavItem { icon: Icon::History, label: "History", active: page == Page::History, badge: None, onclick: move |_| g2(Page::History) }
                    NavItem { icon: Icon::Layers, label: "Stashes", active: page == Page::Stashes, badge: (stashes > 0).then(|| stashes.to_string()), onclick: move |_| g3(Page::Stashes) }
                    NavItem { icon: Icon::Sliders, label: "Settings", active: false, badge: None, onclick: move |_| state.dialog.set(Some(Dialog::RepoSettings(id))) }
                }
                SectionTitle { title: "Branches", key_name: "Branches", collapsed }
                if !collapsed.read().contains("section:Branches") {
                    RefTree { nodes: build(local), scope: "local", depth: 0, collapsed, all_refs: all_refs.clone(), on_menu: open_menu }
                }
                SectionTitle { title: "Tags", key_name: "Tags", collapsed, count: Some(tags.len()) }
                if !collapsed.read().contains("section:Tags") {
                    RefTree { nodes: build(tags), scope: "tags", depth: 0, collapsed, all_refs: all_refs.clone(), on_menu: open_menu }
                }
                SectionTitle { title: "Remotes", key_name: "Remotes", collapsed,
                    action: Some(rsx! { button { class: "rounded px-1 opacity-0 hover:bg-accent group-hover:opacity-100", title: "Add remote",
                        onclick: move |e| { e.stop_propagation(); state.dialog.set(Some(Dialog::AddRemote)); }, Svg { icon: Icon::Plus, class: "size-3.5" } } }) }
                if !collapsed.read().contains("section:Remotes") {
                    for (name, branches) in remotes {
                        RemoteNode { key: "{name}", name: name.clone(), branches, collapsed, all_refs: all_refs.clone(), on_menu: open_menu }
                    }
                }
            }
            div { class: "flex h-10 shrink-0 items-center gap-1 border-t px-2",
                Svg { icon: Icon::Search, class: "ml-1 size-3.5 text-muted-foreground" }
                input { class: "h-7 min-w-0 flex-1 bg-transparent px-1 text-[12.5px] outline-none select-text", placeholder: "Filter branches & tags",
                    "aria-label": "Filter refs", value: "{filter}", oninput: move |e| filter.set(e.value()) }
                button { class: "rounded p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground", title: "New branch at HEAD",
                    onclick: move |_| state.dialog.set(Some(Dialog::Prompt { kind: PromptKind::CreateBranch { start: None }, initial: String::new() })),
                    Svg { icon: Icon::Plus, class: "size-4" } }
            }
        }
        if let Some((pos, items)) = menu.read().clone() {
            {
                let svc = services.clone();
                let picked = items.clone();
                rsx! {
                    ContextMenu { pos, items: items.iter().map(Action::item).collect(),
                        on_pick: move |i: usize| if let Some(a) = picked.get(i).cloned() { actions::perform(state, svc.clone(), a) },
                        on_close: move |_| menu.set(None) }
                }
            }
        }
    }
}

#[component]
fn SectionTitle(
    title: String,
    key_name: String,
    collapsed: Signal<HashSet<String>>,
    #[props(default)] count: Option<usize>,
    #[props(default)] action: Option<Element>,
) -> Element {
    let mut collapsed = collapsed;
    let k = format!("section:{key_name}");
    rsx! {
        div { class: "group mt-3 flex h-6 items-center px-2 text-[11.5px] font-semibold text-muted-foreground",
            button { class: "flex-1 text-left hover:text-foreground",
                onclick: move |_| { let mut c = collapsed.write(); if !c.remove(&k) { c.insert(k.clone()); } },
                "{title}"
                if let Some(n) = count.filter(|n| *n > 0) { span { class: "ml-1.5 font-normal", "{n}" } }
            }
            if let Some(a) = action { {a} }
        }
    }
}

#[component]
fn NavItem(
    icon: Icon,
    label: String,
    active: bool,
    badge: Option<String>,
    onclick: EventHandler<()>,
) -> Element {
    let cls = if active {
        "bg-accent text-primary-accent font-medium"
    } else {
        "hover:bg-accent/60"
    };
    rsx! {
        button { class: "flex h-7 w-full items-center gap-2.5 rounded-md px-2.5 text-left {cls}", onclick: move |_| onclick.call(()),
            Svg { icon, class: "size-4" }
            span { class: "flex-1 truncate", "{label}" }
            if let Some(b) = badge { span { class: "rounded-full bg-muted-foreground/20 px-1.5 text-[10.5px] text-foreground", "{b}" } }
        }
    }
}

#[component]
fn RemoteNode(
    name: String,
    branches: Vec<(String, GitRef)>,
    collapsed: Signal<HashSet<String>>,
    all_refs: Vec<GitRef>,
    on_menu: MenuOpener,
) -> Element {
    let mut collapsed = collapsed;
    let k = format!("remote:{name}");
    let open = !collapsed.read().contains(&k);
    let r1 = name.clone();
    rsx! {
        div { class: "flex h-7 cursor-default items-center gap-1.5 rounded-md px-1 hover:bg-accent/60",
            onclick: move |_| { let mut c = collapsed.write(); if !c.remove(&k) { c.insert(k.clone()); } },
            oncontextmenu: move |e: MouseEvent| {
                e.prevent_default();
                let p = e.client_coordinates();
                on_menu.call(((p.x, p.y), vec![Action::FetchRemote(r1.clone()), Action::RemoveRemote(r1.clone())]));
            },
            span { class: "w-3.5 text-muted-foreground", if open { Svg { icon: Icon::ChevronDown, class: "size-3" } } else { Svg { icon: Icon::ChevronRight, class: "size-3" } } }
            Svg { icon: Icon::Cloud, class: "size-4 text-muted-foreground" }
            span { class: "truncate", "{name}" }
        }
        if open {
            RefTree { nodes: build(branches), scope: format!("remote:{name}"), depth: 1, collapsed, all_refs, on_menu }
        }
    }
}
