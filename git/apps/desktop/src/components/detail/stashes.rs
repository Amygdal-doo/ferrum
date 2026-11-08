//! Stash list (Stashes page).

use crate::commands::repo;
use crate::components::actions::{self, Action};
use crate::components::icons::{Icon, Svg};
use crate::components::menu::ContextMenu;
use crate::components::now_secs;
use crate::state::{AppState, Selection};
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use ferrumgit_domain::RepoId;
use ferrumgit_domain::text::relative_time;

#[component]
pub fn StashList(id: RepoId) -> Element {
    let state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let mut menu = use_signal(|| None::<((f64, f64), Vec<Action>)>);
    let view = state.view(id);
    let stashes = view
        .data
        .as_ref()
        .map(|d| d.snapshot.stashes.clone())
        .unwrap_or_default();
    let now = now_secs();
    rsx! {
        div { class: "flex h-9 shrink-0 items-center border-b px-3 text-[12.5px] text-muted-foreground", "Stashes" }
        div { class: "min-h-0 flex-1 overflow-y-auto p-2",
            if stashes.is_empty() { p { class: "p-6 text-center text-[13px] text-muted-foreground", "No stashes. Use Save in the toolbar to stash your changes." } }
            for s in stashes {
                {
                    let i = s.index;
                    let selected = view.selection == Selection::Stash(i);
                    let svc = services.clone();
                    let card = if selected { "bg-primary-accent text-white" } else { "hover:bg-accent/60" };
                    let muted = if selected { "text-white/75" } else { "text-muted-foreground" };
                    let when = relative_time(s.time, now);
                    rsx! {
                        div { key: "{s.oid}", class: "flex cursor-default items-center gap-3 rounded-lg px-3 py-2 {card}",
                            onclick: move |_| repo::navigate(state, svc.clone(), id, Selection::Stash(i)),
                            oncontextmenu: move |e: MouseEvent| {
                                e.prevent_default();
                                let p = e.client_coordinates();
                                menu.set(Some(((p.x, p.y), vec![Action::StashApply(i), Action::StashPop(i), Action::StashDrop(i)])));
                            },
                            Svg { icon: Icon::Layers, class: "size-4" }
                            div { class: "min-w-0 flex-1 text-[12.5px]",
                                p { class: "truncate font-medium", "{s.message}" }
                                p { class: "text-[11.5px] {muted}", "{s.refname()} · {when}" }
                            }
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
