//! The shared Ferrum project bar, inline under the sidebar's title row.
//!
//! A FerrumAPI project is a folder (usually a git repository), shared with
//! the other Ferrum apps through the bar's recent-projects switcher.
//! Collections belong to a project or are shared; the sidebar shows the
//! current project's collections plus the shared ones. With no project the
//! bar names the active tab's collection (or the workspace) instead.

use crate::app::FerrumApi;
use crate::state::AppState;
use ferrum_ui::project::project_root;
use ferrum_ui::{ProjectBar, ProjectBarEvent};
use ferrumapi_domain::Collection;
use gpui::{AppContext as _, Context, Entity, SharedString, Subscription, Window};
use std::path::{Path, PathBuf};

/// The executable name, for the shared bar.
const APP_BINARY: &str = "ferrumapi";

pub fn new_bar(
    window: &mut Window,
    cx: &mut Context<FerrumApi>,
) -> (Entity<ProjectBar>, Subscription) {
    let bar = cx.new(|cx| ProjectBar::new(APP_BINARY, window, cx).inline(true));
    let subscription = cx.subscribe(&bar, |app: &mut FerrumApi, _, event, cx| match event {
        ProjectBarEvent::Open(path) => app.open_project(Some(path.clone()), cx),
    });
    (bar, subscription)
}

/// Display name of a project path (its folder name).
#[must_use]
pub fn project_name(path: &str) -> String {
    ferrum_ui::project::project_name(Path::new(path))
}

/// Whether `c` is listed for `project`: its own collections and shared ones.
#[must_use]
pub fn visible_in(c: &Collection, project: Option<&str>) -> bool {
    c.project.is_none() || c.project.as_deref() == project
}

/// `(name, detail)` shown when there is no project folder: the active tab's
/// collection, else the workspace, and what it holds under the active
/// environment.
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
            let count = state
                .collections
                .iter()
                .filter(|c| visible_in(c, state.project.as_deref()))
                .count();
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
    /// Mirrors the project (or the active collection) into the bar; called
    /// on every render, the bar ignores unchanged values.
    pub(crate) fn sync_project_bar(&mut self, cx: &mut Context<Self>) {
        if !self.state.loaded {
            return;
        }
        let project = self.state.project.as_ref().map(PathBuf::from);
        let (title, detail) = bar_labels(&self.state);
        let detail = project.is_none().then_some(detail);
        let bar = self.project_bar.read(cx);
        if bar.project() != project.as_deref()
            || bar.title() != Some(&title)
            || bar.detail() != detail.as_ref()
        {
            self.project_bar.update(cx, |bar, cx| {
                bar.set_project(project, cx);
                bar.set_title(Some(title), cx);
                bar.set_detail(detail, cx);
            });
        }
    }

    /// Switches to a project folder (its git root), or to none.
    pub fn open_project(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        let project = path.map(|p| project_root(&p).to_string_lossy().into_owned());
        if self.state.project == project {
            return;
        }
        self.state.project = project;
        self.state.collection_filter.clear();
        self.state.touch();
        cx.notify();
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

    #[test]
    fn projects_see_their_own_and_shared_collections() {
        let mut state = AppState::new();
        let mut app = Collection::new("App");
        app.project = Some("/code/app".into());
        let mut other = Collection::new("Other");
        other.project = Some("/code/other".into());
        state.collections = vec![app.clone(), other.clone(), Collection::new("Shared")];
        assert!(visible_in(&app, Some("/code/app")));
        assert!(!visible_in(&other, Some("/code/app")));
        assert!(visible_in(&state.collections[2], Some("/code/app")));
        assert!(!visible_in(&app, None));
        state.project = Some("/code/app".into());
        assert_eq!(
            bar_labels(&state).1.as_ref(),
            "2 collections · No environment"
        );
        assert_eq!(project_name("/code/app"), "app");
    }
}
