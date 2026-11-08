//! Quick Open (switch repository) and the branch picker for Merge / Rebase.

use super::Modal;
use crate::commands::{repo, workspace};
use crate::components::icons::{Icon, Svg};
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use ferrumgit_domain::{RefKind, RepoOp};

const LIST: &str = "max-h-80 overflow-y-auto rounded-md border p-1";
const SEARCH: &str = "h-9 w-full rounded-md border bg-background px-3 text-sm outline-none focus:border-ring select-text";

fn row_class(active: bool) -> &'static str {
    if active {
        "flex w-full items-center gap-2 rounded px-2.5 py-1.5 text-left text-[13px] bg-primary-accent text-white"
    } else {
        "flex w-full items-center gap-2 rounded px-2.5 py-1.5 text-left text-[13px] hover:bg-accent"
    }
}

#[component]
pub fn QuickOpen() -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let mut query = use_signal(String::new);
    let mut cursor = use_signal(|| 0usize);
    let q = query.read().to_lowercase();
    let repos: Vec<_> = state
        .repos
        .read()
        .iter()
        .filter(|r| {
            q.is_empty() || r.name.to_lowercase().contains(&q) || r.path.to_lowercase().contains(&q)
        })
        .cloned()
        .collect();
    let cur = (*cursor.read()).min(repos.len().saturating_sub(1));
    let ids: Vec<_> = repos.iter().map(|r| r.id).collect();
    let s_key = services.clone();
    rsx! {
        Modal { title: "Quick Open", on_close: move |_| state.dialog.set(None),
            input { class: SEARCH, placeholder: "Repository name or path",
                onmounted: |e: MountedEvent| async move { let _ = e.data().set_focus(true).await; },
                oninput: move |e| { query.set(e.value()); cursor.set(0); },
                onkeydown: move |e: KeyboardEvent| match e.key() {
                    Key::ArrowDown => cursor.set((cur + 1).min(ids.len().saturating_sub(1))),
                    Key::ArrowUp => cursor.set(cur.saturating_sub(1)),
                    Key::Enter => if let Some(id) = ids.get(cur).copied() {
                        state.dialog.set(None);
                        workspace::activate(state, s_key.clone(), Some(id));
                    },
                    _ => {}
                } }
            div { class: "mt-3 {LIST}",
                if repos.is_empty() { p { class: "p-3 text-center text-xs text-muted-foreground", "No repositories match." } }
                for (i, r) in repos.into_iter().enumerate() {
                    {
                        let svc = services.clone();
                        let id = r.id;
                        rsx! {
                            button { key: "{id}", class: row_class(i == cur),
                                onclick: move |_| { state.dialog.set(None); workspace::activate(state, svc.clone(), Some(id)); },
                                Svg { icon: Icon::Folder, class: "size-4" }
                                span { class: "font-medium", "{r.name}" }
                                span { class: "min-w-0 flex-1 truncate text-right text-[11px] opacity-70", "{r.path}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn PickBranch(rebase: bool) -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let mut query = use_signal(String::new);
    let Some(id) = *state.active.read() else {
        return rsx! {};
    };
    let view = state.view(id);
    let q = query.read().to_lowercase();
    let current = view
        .data
        .as_ref()
        .and_then(|d| d.snapshot.status.branch.head.clone())
        .unwrap_or_default();
    let branches: Vec<_> = view
        .data
        .as_ref()
        .map(|d| {
            d.snapshot
                .refs
                .iter()
                .filter(|r| {
                    (r.is(RefKind::LocalBranch) || r.is(RefKind::RemoteBranch)) && !r.is_head
                })
                .filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let title = if rebase {
        format!("Rebase “{current}” onto…")
    } else {
        format!("Merge into “{current}”…")
    };
    rsx! {
        Modal { title, on_close: move |_| state.dialog.set(None),
            input { class: SEARCH, placeholder: "Filter branches",
                onmounted: |e: MountedEvent| async move { let _ = e.data().set_focus(true).await; },
                oninput: move |e| query.set(e.value()) }
            div { class: "mt-3 {LIST}",
                if branches.is_empty() { p { class: "p-3 text-center text-xs text-muted-foreground", "No other branches." } }
                for b in branches {
                    {
                        let svc = services.clone();
                        let name = b.name.clone();
                        let icon = if b.is(RefKind::RemoteBranch) { Icon::Cloud } else { Icon::Branch };
                        rsx! {
                            button { key: "{b.full_name}", class: row_class(false),
                                onclick: move |_| {
                                    state.dialog.set(None);
                                    let op = if rebase { RepoOp::Rebase { onto: name.clone() } } else { RepoOp::Merge { branch: name.clone(), no_ff: false } };
                                    repo::op(state, svc.clone(), id, op);
                                },
                                Svg { icon, class: "size-4 text-muted-foreground" }
                                span { class: "flex-1 truncate", "{b.name}" }
                                if b.merged { span { class: "text-[11px] text-muted-foreground", "merged" } }
                            }
                        }
                    }
                }
            }
        }
    }
}
