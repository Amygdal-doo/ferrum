//! A file row with change letter, name, directory and hover actions.

use crate::components::{change_class, split_path};
use dioxus::prelude::*;
use ferrumgit_domain::ChangeKind;

#[derive(Debug, Clone, PartialEq)]
pub struct RowAction {
    pub label: &'static str,
    pub title: &'static str,
    pub danger: bool,
}

#[component]
pub fn FileRow(
    path: String,
    kind: ChangeKind,
    #[props(default)] stats: Option<(u32, u32)>,
    #[props(default)] active: bool,
    #[props(default)] actions: Vec<RowAction>,
    on_open: EventHandler<()>,
    #[props(default)] on_action: Option<EventHandler<usize>>,
) -> Element {
    let (name, dir) = split_path(&path);
    let kind_class = change_class(kind);
    let tone = if active {
        "bg-selection text-selection-foreground"
    } else {
        "hover:bg-accent/60"
    };
    rsx! {
        div { class: "group flex h-7 cursor-pointer items-center gap-2 rounded px-2 text-xs {tone}", title: "{path}",
            onclick: move |_| on_open.call(()),
            span { class: "w-3 shrink-0 font-mono font-semibold {kind_class}", "{kind.letter()}" }
            span { class: "shrink-0 truncate", "{name}" }
            span { class: "min-w-0 flex-1 truncate text-muted-foreground", "{dir}" }
            if let Some((a, d)) = stats {
                span { class: "shrink-0 font-mono text-[10px] text-success", "+{a}" }
                span { class: "shrink-0 font-mono text-[10px] text-destructive", "−{d}" }
            }
            for (i, a) in actions.into_iter().enumerate() {
                button {
                    key: "{a.label}",
                    class: if a.danger { "hidden shrink-0 rounded border bg-background px-1.5 text-[10px] text-destructive hover:bg-accent group-hover:block" } else { "hidden shrink-0 rounded border bg-background px-1.5 text-[10px] hover:bg-accent group-hover:block" },
                    title: a.title,
                    onclick: move |e| { e.stop_propagation(); if let Some(h) = on_action { h.call(i) } },
                    "{a.label}"
                }
            }
        }
    }
}

/// Section header with count and trailing buttons.
#[component]
pub fn ListHeader(title: String, count: usize, children: Element) -> Element {
    rsx! {
        div { class: "flex h-8 shrink-0 items-center gap-1 border-b bg-sidenav px-3",
            span { class: "flex-1 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground", "{title} ({count})" }
            {children}
        }
    }
}
