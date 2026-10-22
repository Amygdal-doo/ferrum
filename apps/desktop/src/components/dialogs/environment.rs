//! Environment / globals variable editor.

use super::{Actions, Modal};
use crate::commands::environments;
use crate::components::widgets::{FIELD, VarsTable};
use crate::state::AppState;
use crate::state::kv::{self, RowEdit};
use dioxus::prelude::*;
use ferrumapi_application::AppServices;
use ferrumapi_domain::EnvironmentId;

#[component]
pub fn EnvironmentDialog(id: EnvironmentId) -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let mut draft = use_signal(|| environments::get(state, id));
    let Some(env) = draft.read().clone() else {
        return rsx! {};
    };
    let globals = id.is_globals();
    let active = *state.active_env.read() == Some(id);
    let title = if globals {
        "Globals".to_owned()
    } else {
        "Environment".to_owned()
    };

    rsx! {
        Modal { title, width: "max-w-3xl", on_close: move |_| state.dialog.set(None),
            div { class: "flex min-h-0 flex-col gap-3 text-sm",
                if globals {
                    p { class: "text-xs text-muted-foreground", "Globals are available to every request. Collection variables and the active environment override them." }
                } else {
                    div { class: "flex items-center gap-3",
                        span { class: "w-16 shrink-0 text-xs text-muted-foreground", "Name" }
                        input { class: FIELD, initial_value: "{env.name}",
                            oninput: move |e| if let Some(env) = draft.write().as_mut() { env.name = e.value(); } }
                        label { class: "flex shrink-0 items-center gap-1.5 text-xs",
                            input { r#type: "checkbox", checked: active,
                                onchange: move |e| environments::set_active(state, e.checked().then_some(id)) }
                            "Active"
                        }
                    }
                }
                div { class: "max-h-[60vh] min-h-48 overflow-y-auto rounded-md border",
                    VarsTable { rows: env.variables.clone(),
                        on_edit: move |(i, e): (usize, RowEdit)| if let Some(env) = draft.write().as_mut() { kv::apply(&mut env.variables, i, e); } }
                }
                p { class: "text-xs text-muted-foreground", "Use variables as {{{{name}}}} in the URL, params, headers, body and auth. Secret values are masked." }
            }
            Actions { confirm: "Save",
                on_confirm: move |_| {
                    if let Some(env) = draft.peek().clone() {
                        environments::save(state, services.clone(), env);
                    }
                    state.dialog.set(None);
                }
            }
        }
    }
}
