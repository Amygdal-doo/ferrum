//! Commit list rows: month header and two-line commit rows (Tower style).

use super::lanes::{Lanes, PassThrough};
use super::{MONTH_H, ROW_H};
use crate::commands::repo;
use crate::components::avatar_class;
use crate::state::{AppState, Selection};
use dioxus::prelude::*;
use ferrumgit_application::{AppServices, GraphRow};
use ferrumgit_domain::text::initials;
use ferrumgit_domain::{Commit, GitRef, RefKind};

#[component]
pub fn MonthRow(label: String, top: f64, graph_px: f64, rails: Vec<(usize, usize)>) -> Element {
    rsx! {
        div { class: "absolute inset-x-0 flex items-end", style: "top: {top}px; height: {MONTH_H}px",
            div { class: "absolute inset-y-0 left-2", PassThrough { lanes: rails, width: graph_px, height: MONTH_H } }
            span { class: "pb-1.5 text-[11px] font-semibold tracking-wide text-muted-foreground", style: "padding-left: {graph_px + 18.0}px", "{label}" }
        }
    }
}

/// `5. 11. 2026.` (local time).
fn short_date(unix: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(unix, 0)
        .single()
        .map_or_else(String::new, |t| t.format("%-d. %-m. %Y.").to_string())
}

#[component]
pub fn CommitRow(
    commit: Commit,
    graph: Option<GraphRow>,
    refs: Vec<GitRef>,
    is_head: bool,
    selected: bool,
    graph_px: f64,
    lanes: usize,
    top: f64,
    on_menu: EventHandler<(f64, f64)>,
) -> Element {
    let state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let oid = commit.oid.clone();
    let date = short_date(commit.author_time);
    let full = ferrumgit_domain::text::full_time(commit.author_time);
    let short = commit.oid.short().to_owned();
    let avatar = initials(&commit.author_name);
    let avatar_bg = avatar_class(&commit.author_email);
    let card = if selected {
        "bg-primary-accent text-white shadow-sm"
    } else {
        "hover:bg-accent/60"
    };
    let muted = if selected {
        "text-white/75"
    } else {
        "text-muted-foreground"
    };
    let author = commit.author_name.clone();
    let title = format!("{} <{}>", commit.author_name, commit.author_email);
    rsx! {
        div { class: "absolute inset-x-0 flex items-stretch", style: "top: {top}px; height: {ROW_H}px",
            oncontextmenu: move |e: MouseEvent| { e.prevent_default(); let p = e.client_coordinates(); on_menu.call((p.x, p.y)); },
            div { class: "relative shrink-0 pl-2", style: "width: {graph_px + 8.0}px",
                if let Some(g) = graph { Lanes { graph: g, lanes, width: graph_px, height: ROW_H, is_head } }
            }
            div { class: "mr-2 flex min-w-0 flex-1 cursor-default items-center gap-2.5 rounded-lg px-2 {card}",
                onclick: move |_| if let Some(id) = *state.active.peek() { repo::navigate(state, services.clone(), id, Selection::Commit(oid.clone())) },
                span { class: "flex size-7 shrink-0 items-center justify-center rounded-full text-[11px] font-semibold {avatar_bg}", title: "{title}", "{avatar}" }
                div { class: "flex min-w-0 flex-1 flex-col gap-0.5 text-[12.5px] leading-tight",
                    div { class: "flex min-w-0 items-center gap-1.5",
                        span { class: "max-w-[45%] shrink-0 truncate font-semibold", "{author}" }
                        span { class: "flex min-w-0 flex-1 items-center gap-1 overflow-hidden",
                            for r in refs { RefBadge { key: "{r.full_name}", r: r.clone(), on_card: selected } }
                        }
                        span { class: "ml-auto shrink-0 pl-2 text-[11.5px] {muted}", title: "{full}", "{date}" }
                    }
                    div { class: "flex min-w-0 items-center gap-2",
                        span { class: "shrink-0 font-mono text-[11.5px] {muted}", "{short}" }
                        span { class: "truncate", title: "{commit.summary}", "{commit.summary}" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn RefBadge(r: GitRef, #[props(default)] on_card: bool) -> Element {
    let cls = if on_card {
        "bg-white/20 text-white"
    } else {
        match r.kind {
            Some(RefKind::LocalBranch) if r.is_head => "bg-primary-accent text-white",
            Some(RefKind::LocalBranch) => "bg-info-light text-info-dark",
            Some(RefKind::RemoteBranch) => "bg-muted-foreground/25 text-foreground",
            Some(RefKind::Tag) => "bg-warning-light text-warning-dark",
            None => "",
        }
    };
    let label = if r.is_head {
        format!("HEAD → {}", r.name)
    } else {
        r.name.clone()
    };
    rsx! {
        span { class: "inline-flex max-w-40 shrink-0 items-center truncate rounded px-1.5 py-px text-[10.5px] font-medium {cls}", title: "{r.full_name}", "{label}" }
    }
}
