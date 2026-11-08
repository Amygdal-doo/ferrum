//! Collection name, auth (inherited by its requests) and variables.

use super::{Actions, Modal};
use crate::commands::collections;
use crate::components::widgets::{AuthEditor, FIELD, VarsTable};
use crate::state::AppState;
use crate::state::kv::{self, RowEdit};
use dioxus::prelude::*;
use ferrumapi_application::AppServices;
use ferrumapi_domain::{Auth, CollectionId};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    Auth,
    Variables,
}

#[component]
pub fn CollectionSettingsDialog(id: CollectionId) -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let mut draft = use_signal(|| state.collection(id));
    let mut pane = use_signal(|| Pane::Auth);
    let Some(c) = draft.read().clone() else {
        return rsx! {};
    };
    // `Inherit` at the root means no auth; show it as such.
    let auth = if c.auth.is_inherit() {
        Auth::None
    } else {
        c.auth.clone()
    };
    let current = *pane.read();
    let tab_cls = |p: Pane| {
        if p == current {
            "border-foreground text-foreground"
        } else {
            "border-transparent text-muted-foreground hover:text-foreground"
        }
    };

    rsx! {
        Modal { title: "Collection settings", width: "max-w-3xl", on_close: move |_| state.dialog.set(None),
            div { class: "flex min-h-0 flex-col gap-3 text-sm",
                label { class: "flex items-center gap-3",
                    span { class: "w-24 shrink-0 text-xs text-muted-foreground", "Name" }
                    input { class: FIELD, initial_value: "{c.name}",
                        oninput: move |e| if let Some(c) = draft.write().as_mut() { c.name = e.value(); } }
                }
                div { class: "flex gap-4 border-b text-xs",
                    button { class: "border-b-2 pb-1.5 font-medium {tab_cls(Pane::Auth)}", onclick: move |_| pane.set(Pane::Auth), "Authorization" }
                    button { class: "border-b-2 pb-1.5 font-medium {tab_cls(Pane::Variables)}", onclick: move |_| pane.set(Pane::Variables), "Variables ({kv::active_count(&c.variables)})" }
                }
                div { class: "min-h-64 overflow-y-auto",
                    match current {
                        Pane::Auth => rsx! {
                            p { class: "mb-3 text-xs text-muted-foreground", "Requests and folders set to “Inherit from parent” use this auth." }
                            AuthEditor { auth, scope: "{id}", allow_inherit: false,
                                on_change: move |a: Auth| if let Some(c) = draft.write().as_mut() { c.auth = a; } }
                        },
                        Pane::Variables => rsx! {
                            p { class: "mb-3 text-xs text-muted-foreground", "Collection variables override globals; the active environment overrides both." }
                            VarsTable { rows: c.variables.clone(),
                                on_edit: move |(i, e): (usize, RowEdit)| if let Some(c) = draft.write().as_mut() { kv::apply(&mut c.variables, i, e); } }
                        },
                    }
                }
            }
            Actions { confirm: "Save",
                on_confirm: move |_| {
                    if let Some(c) = draft.peek().clone() {
                        collections::update_meta(state, services.clone(), c);
                    }
                    state.dialog.set(None);
                }
            }
        }
    }
}
