//! History: commits grouped by month, with graph rails, virtualized.

mod lanes;
mod row;

pub use row::RefBadge;

use crate::commands::{repo, workspace};
use crate::components::actions::{self, Action};
use crate::components::icons::{Icon, Svg};
use crate::components::menu::ContextMenu;
use crate::state::{AppState, Selection};
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use ferrumgit_domain::{Commit, GitRef};
use std::collections::HashMap;

pub const ROW_H: f64 = 50.0;
pub const MONTH_H: f64 = 34.0;
pub const LANE_W: f64 = 11.0;
const MAX_LANES: usize = 16;
const OVERSCAN_PX: f64 = 600.0;

enum Item {
    Month(String),
    Commit(usize),
}

fn month(unix: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(unix, 0)
        .single()
        .map_or_else(String::new, |t| {
            t.format("%B %Y").to_string().to_uppercase()
        })
}

fn matches(c: &Commit, needle: &str) -> bool {
    c.summary.to_lowercase().contains(needle)
        || c.author_name.to_lowercase().contains(needle)
        || c.oid.as_str().starts_with(needle)
}

#[component]
pub fn CommitList() -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let mut scroll_top = use_signal(|| 0.0f64);
    let mut viewport = use_signal(|| 900.0f64);
    let mut menu = use_signal(|| None::<((f64, f64), Vec<Action>)>);
    let Some(id) = *state.active.read() else {
        return rsx! {};
    };
    let view = state.view(id);
    let Some(data) = view.data.clone() else {
        let svc = services.clone();
        return rsx! {
            div { class: "flex flex-1 flex-col items-center justify-center gap-3 p-8 text-sm text-muted-foreground",
                if let Some(e) = view.error.clone() {
                    p { class: "max-w-lg whitespace-pre-wrap text-center text-destructive select-text", "{e}" }
                    button { class: "rounded-md border px-3 py-1 hover:bg-accent", onclick: move |_| repo::load(state, svc.clone(), id), "Retry" }
                } else { "Loading repository…" }
            }
        };
    };
    if let Some(row) = view.scroll_to {
        state.update_view(id, |v| v.scroll_to = None);
        // Approximate: rows plus a header per month boundary before it.
        let top = (row as f64 * ROW_H - 200.0).max(0.0);
        document::eval(&format!(
            "const g = document.getElementById('ferrumgit-history'); if (g) g.scrollTop = {top};"
        ));
    }
    let snap = &data.snapshot;
    let needle = view.search.trim().to_lowercase();
    let searching = !needle.is_empty();
    let mut by_oid: HashMap<&str, Vec<GitRef>> = HashMap::new();
    for r in &snap.refs {
        by_oid.entry(r.target.as_str()).or_default().push(r.clone());
    }
    // Build items (month headers + commits) and their offsets.
    let mut items = Vec::new();
    let mut last_month = String::new();
    for (i, c) in snap.commits.iter().enumerate() {
        if searching && !matches(c, &needle) {
            continue;
        }
        let m = month(c.author_time);
        if m != last_month {
            items.push(Item::Month(m.clone()));
            last_month = m;
        }
        items.push(Item::Commit(i));
    }
    let mut offsets = Vec::with_capacity(items.len() + 1);
    let mut y = 6.0;
    for it in &items {
        offsets.push(y);
        y += match it {
            Item::Month(_) => MONTH_H,
            Item::Commit(_) => ROW_H,
        };
    }
    let total_h = y + if snap.has_more { 56.0 } else { 12.0 };
    let (top, bottom) = (
        *scroll_top.read() - OVERSCAN_PX,
        *scroll_top.read() + *viewport.read() + OVERSCAN_PX,
    );
    let first = offsets.partition_point(|o| *o < top).saturating_sub(1);
    let last = offsets.partition_point(|o| *o < bottom).min(items.len());
    let lanes = if searching {
        0
    } else {
        data.graph
            .iter()
            .map(|r| r.width)
            .max()
            .unwrap_or(1)
            .min(MAX_LANES)
    };
    let graph_px = if searching {
        0.0
    } else {
        lanes as f64 * LANE_W + 12.0
    };
    let head = snap.status.branch.oid.clone();
    let count = if searching {
        format!(
            "{} of {}",
            items
                .iter()
                .filter(|i| matches!(i, Item::Commit(_)))
                .count(),
            snap.commits.len()
        )
    } else {
        snap.commits.len().to_string()
    };
    let svc_more = services.clone();

    rsx! {
        div { class: "flex h-9 shrink-0 items-center gap-2 border-b px-3 text-[12.5px] text-muted-foreground",
            span { class: "flex-1 truncate", if searching { "Search results" } else { "All Branches, Remotes, Tags" } }
            span { class: "text-[11px]", "{count}" }
            button { class: "rounded p-1 hover:bg-accent hover:text-foreground", title: if state.settings.read().hide_invisibles { "Show whitespace in diffs" } else { "Hide whitespace in diffs" },
                onclick: move |_| { let v = state.settings.peek().hide_invisibles; state.settings.write().hide_invisibles = !v; },
                Svg { icon: Icon::Sliders, class: "size-3.5" } }
        }
        div {
            id: "ferrumgit-history",
            class: "relative min-h-0 flex-1 overflow-y-auto",
            onscroll: move |e| scroll_top.set(e.data().scroll_top()),
            onmounted: move |e: MountedEvent| async move {
                if let Ok(r) = e.data().get_client_rect().await { viewport.set(r.height()); }
            },
            div { class: "relative", style: "height: {total_h}px",
                for i in first..last {
                    match &items[i] {
                        Item::Month(label) => {
                            // Rails of lanes continuing through the header into the next commit.
                            let rails: Vec<(usize, usize)> = match items.get(i + 1) {
                                Some(Item::Commit(n)) if !searching => data.graph[*n].up.iter().filter(|e| e.from < lanes).map(|e| (e.from, e.color)).collect(),
                                _ => Vec::new(),
                            };
                            rsx! { row::MonthRow { key: "m{label}", label: label.clone(), top: offsets[i], graph_px, rails } }
                        }
                        Item::Commit(n) => {
                            let c = snap.commits[*n].clone();
                            let refs = by_oid.get(c.oid.as_str()).cloned().unwrap_or_default();
                            let selected = view.selection == Selection::Commit(c.oid.clone());
                            let oid = c.oid.clone();
                            let graph = (!searching).then(|| data.graph[*n].clone());
                            rsx! {
                                row::CommitRow { key: "{c.oid}", is_head: head.as_ref() == Some(&c.oid), commit: c, graph, refs, selected, graph_px, lanes, top: offsets[i],
                                    on_menu: move |pos: (f64, f64)| menu.set(Some((pos, actions::for_commit(&oid)))) }
                            }
                        }
                    }
                }
                if snap.has_more && !searching {
                    div { class: "absolute inset-x-0 flex justify-center py-3", style: "top: {y}px",
                        button { class: "rounded-md border px-3 py-1 text-xs hover:bg-accent",
                            onclick: move |_| {
                                let limit = state.log_limit() + 2000;
                                state.settings.write().log_limit = u32::try_from(limit).unwrap_or(u32::MAX);
                                workspace::save_settings(state, svc_more.clone());
                                repo::load(state, svc_more.clone(), id);
                            },
                            "Load 2000 more commits"
                        }
                    }
                }
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
