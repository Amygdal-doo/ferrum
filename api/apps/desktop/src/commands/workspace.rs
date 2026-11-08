//! Startup, autosave, settings and tab management.

use super::{background, run};
use crate::state::{AppState, tabs};
use dioxus::prelude::*;
use ferrumapi_application::{AppServices, DEFAULT_HISTORY_LIMIT};
use ferrumapi_domain::{HttpRequest, RequestOrigin, RequestTab, TabId, WorkspaceState};
use std::time::Duration;

const AUTOSAVE_INTERVAL: Duration = Duration::from_millis(1500);

/// Loads everything. Never fails hard: on error the app starts empty and
/// shows a notice.
pub async fn bootstrap(mut state: AppState, services: AppServices) {
    if let Ok(dark) =
        document::eval("return window.matchMedia('(prefers-color-scheme: dark)').matches")
            .join::<bool>()
            .await
    {
        state.system_dark.set(dark);
    }

    let svc = services.clone();
    let loaded = background(&services, async move {
        let s = &svc.store;
        (
            s.load_settings().await,
            s.load_workspace().await,
            s.list_collections().await,
            s.list_environments().await,
            s.load_globals().await,
        )
    })
    .await;

    let mut ws = WorkspaceState::default();
    match loaded {
        Ok((settings, workspace, collections, envs, globals)) => {
            let mut errors = Vec::new();
            match settings {
                Ok(s) => state.settings.set(s),
                Err(e) => errors.push(format!("settings: {e}")),
            }
            match collections {
                Ok(c) => state.collections.set(c),
                Err(e) => errors.push(format!("collections: {e}")),
            }
            match envs {
                Ok(e) => state.environments.set(e),
                Err(e) => errors.push(format!("environments: {e}")),
            }
            match globals {
                Ok(g) => state.globals.set(g),
                Err(e) => errors.push(format!("globals: {e}")),
            }
            match workspace {
                Ok(w) => ws = w,
                Err(e) => errors.push(format!("open tabs: {e}")),
            }
            if !errors.is_empty() {
                state.notify(format!("Could not load {}", errors.join(", ")), true);
            }
        }
        Err(e) => state.notify(e, true),
    }

    let env = ws
        .active_environment
        .filter(|id| state.environments.peek().iter().any(|e| e.id == *id));
    state.active_env.set(env);
    let (mut restored, active) = tabs::restore(ws);
    tabs::reconcile_origins(&mut restored, &state.collections.peek());
    state.tabs.set(restored);
    state.active_tab.set(active);
    state.loaded.set(true);
    super::history::refresh(state, services);
}

fn snapshot(state: AppState) -> WorkspaceState {
    WorkspaceState {
        tabs: state.tabs.peek().clone(),
        active_tab: *state.active_tab.peek(),
        active_environment: *state.active_env.peek(),
    }
}

/// Persists the workspace about 1.5 s after a change.
pub async fn autosave_loop(state: AppState, services: AppServices) {
    let mut saved_rev = 0u64;
    loop {
        let _ = background(&services, tokio::time::sleep(AUTOSAVE_INTERVAL)).await;
        let rev = *state.ws_rev.peek();
        if rev == saved_rev || !*state.loaded.peek() {
            continue;
        }
        let ws = snapshot(state);
        let svc = services.clone();
        match run(
            &services,
            async move { svc.store.save_workspace(&ws).await },
        )
        .await
        {
            Ok(()) => saved_rev = rev,
            Err(e) => tracing::warn!(error = %e, "failed to save workspace"),
        }
    }
}

pub fn save_settings(state: AppState, services: AppServices) {
    let settings = *state.settings.peek();
    let svc = services.clone();
    spawn(async move {
        if let Err(e) = run(&services, async move {
            svc.store.save_settings(&settings).await
        })
        .await
        {
            state.notify(format!("Could not save settings: {e}"), true);
        }
    });
}

/// History limit from settings.
#[must_use]
pub fn history_limit(state: AppState) -> usize {
    match state.settings.peek().history_limit {
        0 => DEFAULT_HISTORY_LIMIT,
        n => n as usize,
    }
}

pub fn activate(mut state: AppState, id: TabId) {
    state.active_tab.set(Some(id));
    state.touch();
}

pub fn new_tab(state: AppState) {
    push_tab(state, tabs::blank_tab());
}

fn push_tab(mut state: AppState, tab: RequestTab) {
    let id = tab.id;
    state.tabs.write().push(tab);
    activate(state, id);
}

/// Opens `request` in a new tab (or focuses the tab already showing `origin`).
pub fn open_request(
    state: AppState,
    title: String,
    request: HttpRequest,
    origin: Option<RequestOrigin>,
) {
    if let Some(o) = origin
        && let Some(id) = tabs::find_by_origin(&state.tabs.peek(), o)
    {
        activate(state, id);
        return;
    }
    let mut tab = RequestTab::new(title, request);
    tab.origin = origin;
    push_tab(state, tab);
}

pub fn close_tab(mut state: AppState, id: TabId) {
    if let Some(token) = state.cancels.write().remove(&id) {
        token.cancel();
    }
    state.responses.write().remove(&id);
    let active = *state.active_tab.peek();
    let next = tabs::close(&mut state.tabs.write(), active, id);
    state.active_tab.set(next);
    state.touch();
}
