//! Page columns: the middle list (commits / working-copy files / stashes)
//! and the right panel (details / diff) for the active repository.

mod commit;
mod files;
mod stashes;
mod wip;

use crate::components::diff::DiffPane;
use crate::components::graph::CommitList;
use crate::state::{AppState, Page};
use dioxus::prelude::*;

fn empty(text: &str) -> Element {
    rsx! { div { class: "flex flex-1 items-center justify-center p-8 text-[13px] text-muted-foreground", "{text}" } }
}

#[component]
pub fn MiddleColumn() -> Element {
    let state = use_context::<AppState>();
    let Some(id) = *state.active.read() else {
        return rsx! {};
    };
    let view = state.view(id);
    let body = match view.page {
        Page::History => rsx! { CommitList {} },
        Page::WorkingCopy if view.data.is_some() => rsx! { wip::WipPanel { id } },
        Page::Stashes if view.data.is_some() => rsx! { stashes::StashList { id } },
        _ => empty("Loading…"),
    };
    rsx! { section { class: "flex h-full min-w-0 flex-col bg-background", {body} } }
}

#[component]
pub fn RightColumn() -> Element {
    let state = use_context::<AppState>();
    let Some(id) = *state.active.read() else {
        return rsx! {};
    };
    let view = state.view(id);
    let body = match view.page {
        Page::WorkingCopy => match view.diff {
            Some(_) => rsx! { DiffPane {} },
            None => empty("Select a file to see its changes"),
        },
        Page::History | Page::Stashes => match view.details.clone() {
            Some(d) => rsx! { commit::CommitPanel { id, details: d } },
            None if view.page == Page::Stashes
                && view
                    .data
                    .as_ref()
                    .is_some_and(|d| d.snapshot.stashes.is_empty()) =>
            {
                empty("No stashes")
            }
            None if matches!(view.selection, crate::state::Selection::Wip) => {
                empty("Select a commit")
            }
            None => empty("Loading…"),
        },
    };
    rsx! { section { class: "flex h-full min-w-0 flex-1 flex-col bg-background", {body} } }
}
