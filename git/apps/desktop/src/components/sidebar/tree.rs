//! Refs grouped into folders by `/` (`feature/a`, `feature/b` → feature ▸ a, b).

use super::MenuOpener;
use crate::components::actions::{self, Action};
use crate::components::icons::{Icon, Svg};
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use ferrumgit_domain::{GitRef, RefKind};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Folder {
        name: String,
        path: String,
        children: Vec<Node>,
    },
    Leaf {
        label: String,
        r: GitRef,
    },
}

/// Builds a folder tree from `(display path, ref)` pairs; folders first, then leaves, by name.
pub fn build(items: Vec<(String, GitRef)>) -> Vec<Node> {
    fn insert(
        dirs: &mut BTreeMap<String, Vec<(String, GitRef)>>,
        leaves: &mut Vec<(String, GitRef)>,
        item: (String, GitRef),
    ) {
        match item.0.split_once('/') {
            Some((dir, rest)) => dirs
                .entry(dir.to_owned())
                .or_default()
                .push((rest.to_owned(), item.1)),
            None => leaves.push(item),
        }
    }
    fn go(items: Vec<(String, GitRef)>, prefix: &str) -> Vec<Node> {
        let mut dirs = BTreeMap::new();
        let mut leaves = Vec::new();
        for it in items {
            insert(&mut dirs, &mut leaves, it);
        }
        let mut out: Vec<Node> = dirs
            .into_iter()
            .map(|(name, children)| {
                let path = format!("{prefix}{name}/");
                Node::Folder {
                    children: go(children, &path),
                    name,
                    path,
                }
            })
            .collect();
        leaves.sort_by_key(|(l, _)| l.to_lowercase());
        out.extend(leaves.into_iter().map(|(label, r)| Node::Leaf { label, r }));
        out
    }
    go(items, "")
}

#[component]
pub fn RefTree(
    nodes: Vec<Node>,
    scope: String,
    depth: usize,
    collapsed: Signal<HashSet<String>>,
    all_refs: Vec<GitRef>,
    on_menu: MenuOpener,
) -> Element {
    let mut collapsed = collapsed;
    rsx! {
        for n in nodes {
            match n {
                Node::Folder { name, path, children } => {
                    let k = format!("{scope}:{path}");
                    let open = !collapsed.read().contains(&k);
                    let pad = 4.0 + depth as f64 * 14.0;
                    rsx! {
                        div { key: "{k}",
                            div { class: "flex h-7 cursor-default items-center gap-1.5 rounded-md hover:bg-accent/60", style: "padding-left: {pad}px",
                                onclick: move |_| { let mut c = collapsed.write(); if !c.remove(&k) { c.insert(k.clone()); } },
                                span { class: "w-3.5 text-muted-foreground", if open { Svg { icon: Icon::ChevronDown, class: "size-3" } } else { Svg { icon: Icon::ChevronRight, class: "size-3" } } }
                                Svg { icon: Icon::Folder, class: "size-4 text-muted-foreground" }
                                span { class: "truncate", "{name}" }
                            }
                            if open {
                                RefTree { nodes: children, scope: scope.clone(), depth: depth + 1, collapsed, all_refs: all_refs.clone(), on_menu }
                            }
                        }
                    }
                }
                Node::Leaf { label, r } => rsx! { RefRow { key: "{r.full_name}", label, r: r.clone(), depth, menu_items: actions::for_ref(&r, &all_refs), on_menu } },
            }
        }
    }
}

#[component]
fn RefRow(
    label: String,
    r: GitRef,
    depth: usize,
    menu_items: Vec<Action>,
    on_menu: MenuOpener,
) -> Element {
    let state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let target = r.target.clone();
    let dbl = match r.kind {
        Some(RefKind::LocalBranch) if !r.is_head => Some(Action::Checkout(r.name.clone())),
        Some(RefKind::RemoteBranch) => Some(Action::CheckoutRemote(r.name.clone())),
        _ => None,
    };
    let icon = if r.is(RefKind::Tag) {
        Icon::Tag
    } else {
        Icon::Branch
    };
    // Leaves line up with folder names (chevron width + gap).
    let pad = 4.0 + depth as f64 * 14.0 + 20.0;
    let label_cls = if r.merged {
        "truncate text-muted-foreground"
    } else if r.is_head {
        "truncate font-semibold"
    } else {
        "truncate"
    };
    let title = match (&r.upstream, r.upstream_gone) {
        (Some(u), true) => format!("{} — upstream {u} is gone", r.name),
        (Some(u), false) => format!("{} → {u}", r.name),
        _ => r.name.clone(),
    };
    let (s1, s2) = (services.clone(), services);
    let track = match (r.ahead, r.behind) {
        (0, 0) => None,
        (a, 0) => Some(format!("↑{a}")),
        (0, b) => Some(format!("↓{b}")),
        (a, b) => Some(format!("↑{a} | ↓{b}")),
    };
    rsx! {
        div { class: "group flex h-7 cursor-default items-center gap-1.5 rounded-md pr-1.5 hover:bg-accent/60", style: "padding-left: {pad}px", title: "{title}",
            onclick: move |_| actions::reveal(state, s1.clone(), &target),
            ondoubleclick: move |_| if let Some(a) = dbl.clone() { actions::perform(state, s2.clone(), a) },
            oncontextmenu: move |e: MouseEvent| {
                e.prevent_default();
                let p = e.client_coordinates();
                on_menu.call(((p.x, p.y), menu_items.clone()));
            },
            Svg { icon, class: if r.merged { "size-4 text-muted-foreground/60" } else { "size-4 text-muted-foreground" } }
            span { class: "min-w-0 flex-1 {label_cls}", "{label}" }
            if r.upstream_gone { span { class: "rounded border px-1 text-[10px] text-warning", "gone" } }
            if r.merged { span { class: "flex items-center gap-0.5 rounded-full border px-1.5 text-[10px] text-muted-foreground", Svg { icon: Icon::Check, class: "size-2.5" } "Merged" } }
            if r.is_head { span { class: "rounded border px-1 text-[10px] font-semibold", "HEAD" } }
            if let Some(t) = track { span { class: "rounded border px-1 text-[10px] text-muted-foreground", "{t}" } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(name: &str) -> (String, GitRef) {
        (
            name.to_owned(),
            GitRef {
                name: name.to_owned(),
                full_name: format!("refs/heads/{name}"),
                ..GitRef::default()
            },
        )
    }

    #[test]
    fn groups_by_slash_folders_first() {
        let nodes = build(vec![
            r("main"),
            r("feature/b"),
            r("feature/a"),
            r("fix/deep/x"),
            r("Staging"),
        ]);
        let names: Vec<String> = nodes
            .iter()
            .map(|n| match n {
                Node::Folder { name, .. } => format!("{name}/"),
                Node::Leaf { label, .. } => label.clone(),
            })
            .collect();
        assert_eq!(names, ["feature/", "fix/", "main", "Staging"]);
        let Node::Folder { children, path, .. } = &nodes[1] else {
            panic!()
        };
        assert_eq!(path, "fix/");
        assert!(matches!(&children[0], Node::Folder { path, .. } if path == "fix/deep/"));
        let Node::Folder { children, .. } = &nodes[0] else {
            panic!()
        };
        assert!(matches!(&children[0], Node::Leaf { label, .. } if label == "a"));
    }
}
