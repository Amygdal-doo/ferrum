//! The shared Ferrum project bar above the header.
//!
//! FerrumAPI keeps collections in its own SQLite store, not in folders, so
//! there is no project folder to show or switch to: the bar names the
//! active tab's collection (or the workspace) and the active environment,
//! and its switcher is off.

use crate::app::FerrumApi;
use crate::state::AppState;
use ferrum_ui::ProjectBar;
use gpui::{AppContext as _, Context, Entity, SharedString, Window};

/// The executable name, for the shared bar.
const APP_BINARY: &str = "ferrumapi";

pub fn new_bar(window: &mut Window, cx: &mut Context<FerrumApi>) -> Entity<ProjectBar> {
    cx.new(|cx| ProjectBar::new(APP_BINARY, window, cx).switchable(false))
}

/// `(name, detail)`: the active tab's collection, else the workspace, and
/// what it holds under the active environment.
#[must_use]
pub fn bar_labels(state: &AppState) -> (SharedString, SharedString) {
    let env = state.active_environment().map_or_else(
        || "No environment".to_owned(),
        |e| format!("env: {}", e.name),
    );
    let collection = state
        .active_tab()
        .and_then(|tab| tab.origin)
        .and_then(|origin| state.collection(origin.collection));
    match collection {
        Some(c) => {
            let requests = c.requests().len();
            let noun = if requests == 1 { "request" } else { "requests" };
            (
                c.name.clone().into(),
                format!("Collection · {requests} {noun} · {env}").into(),
            )
        }
        None => {
            let count = state.collections.len();
            let noun = if count == 1 {
                "collection"
            } else {
                "collections"
            };
            ("Workspace".into(), format!("{count} {noun} · {env}").into())
        }
    }
}

impl FerrumApi {
    /// Mirrors the active collection and environment into the bar (called on
    /// every render; the bar ignores unchanged text).
    pub(crate) fn sync_project_bar(&mut self, cx: &mut Context<Self>) {
        if !self.state.loaded {
            return;
        }
        let (title, detail) = bar_labels(&self.state);
        let bar = self.project_bar.read(cx);
        if bar.title() != Some(&title) || bar.detail() != Some(&detail) {
            self.project_bar.update(cx, |bar, cx| {
                bar.set_title(Some(title), cx);
                bar.set_detail(Some(detail), cx);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tabs::blank_tab;
    use ferrumapi_domain::{
        Collection, Environment, HttpRequest, Item, RequestOrigin, SavedRequest,
    };

    #[test]
    fn the_bar_names_the_active_collection_and_environment() {
        let mut state = AppState::new();
        state.collections = vec![Collection::new("Billing"), Collection::new("Users")];
        let (name, detail) = bar_labels(&state);
        assert_eq!(name.as_ref(), "Workspace");
        assert_eq!(detail.as_ref(), "2 collections · No environment");

        let mut users = Collection::new("Users");
        let saved = SavedRequest::new("list", HttpRequest::default());
        let origin = RequestOrigin {
            collection: users.id,
            item: saved.id,
        };
        users.insert(None, Item::Request(saved));
        state.collections.push(users);
        let staging = Environment::new("Staging");
        state.active_env = Some(staging.id);
        state.environments = vec![staging];
        let mut tab = blank_tab();
        tab.origin = Some(origin);
        state.active_tab = Some(tab.id);
        state.tabs = vec![tab];
        let (name, detail) = bar_labels(&state);
        assert_eq!(name.as_ref(), "Users");
        assert_eq!(detail.as_ref(), "Collection · 1 request · env: Staging");
    }
}
