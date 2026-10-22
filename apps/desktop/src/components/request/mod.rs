//! Request editor: tab bar, URL bar, sub-tabs.

mod body;
mod form_data;
mod sections;
mod settings;
mod tab_bar;
mod url_bar;

use crate::state::{AppState, TabResponse};
use dioxus::prelude::*;
use ferrumapi_application::http::unresolved_variables;
use sections::{SectionBody, SectionTabs};
use tab_bar::TabBar;
use url_bar::UrlBar;

#[component]
pub fn RequestPane() -> Element {
    let state = use_context::<AppState>();
    let Some(tab) = state.active_tab() else {
        return rsx! { TabBar {} };
    };
    let prepared = state.prepare_tab(&tab);
    let vars = prepared.variables.clone();
    let unresolved = unresolved_variables(&prepared.request, &vars).join(", ");
    let inherited = if tab.request.auth.is_inherit() && tab.origin.is_some() {
        Some(prepared.request.auth.kind_label().to_owned())
    } else {
        None
    };
    let secrets = state.secret_keys(&tab);
    let loading = state
        .responses
        .read()
        .get(&tab.id)
        .is_some_and(TabResponse::is_loading);

    rsx! {
        div { class: "flex h-full min-h-0 flex-col",
            TabBar {}
            UrlBar { tab: tab.clone(), vars: vars.clone(), secrets, loading }
            if !unresolved.is_empty() {
                div { class: "mx-3 mt-2 rounded-md bg-warning-light px-3 py-1.5 text-xs text-warning-dark", role: "status",
                    "Unresolved variables: "
                    span { class: "font-mono", "{unresolved}" }
                    " — define them in the active environment, the collection or globals."
                }
            }
            SectionTabs { tab: tab.clone() }
            SectionBody { tab, vars, inherited }
        }
    }
}
