//! Loading and saving settings and editor tabs.

use super::background;
use crate::state::{AppState, EditorTab};
use dioxus::prelude::*;
use ferrum_application::AppServices;
use std::time::Duration;

const AUTOSAVE_INTERVAL: Duration = Duration::from_millis(1500);

/// Loads settings, profiles and tabs. Never fails hard: on error the app
/// starts empty and shows a notice.
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
        let settings = svc.store.load_settings().await;
        let profiles = svc.connections.list_profiles().await;
        let tabs = svc.store.load_tabs().await;
        (settings, profiles, tabs)
    })
    .await;

    match loaded {
        Ok((settings, profiles, tabs)) => {
            match settings {
                Ok(s) => state.settings.set(s),
                Err(e) => state.notify(format!("Could not load settings: {e}"), true),
            }
            match profiles {
                Ok(p) => state.profiles.set(p),
                Err(e) => state.notify(format!("Could not load connections: {e}"), true),
            }
            let mut tabs: Vec<EditorTab> = tabs
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect();
            // Drop references to profiles that no longer exist.
            let known: Vec<_> = state.profiles.peek().iter().map(|p| p.id).collect();
            for tab in &mut tabs {
                tab.profile_id = tab.profile_id.filter(|id| known.contains(id));
            }
            if tabs.is_empty() {
                tabs.push(EditorTab::new(1, known.first().copied()));
            }
            state.active_tab.set(tabs.first().map(|t| t.id));
            state.tabs.set(tabs);
        }
        Err(e) => state.notify(e, true),
    }
    state.loaded.set(true);
}

/// Periodically persists edited tabs. Restored on next launch.
pub async fn autosave_loop(state: AppState, services: AppServices) {
    loop {
        let _ = background(&services, tokio::time::sleep(AUTOSAVE_INTERVAL)).await;
        if state.tabs.peek().iter().any(|t| t.unsaved) {
            save_tabs(state, services.clone()).await;
        }
    }
}

pub async fn save_tabs(mut state: AppState, services: AppServices) {
    let snapshot: Vec<_> = state
        .tabs
        .peek()
        .iter()
        .enumerate()
        .map(|(i, t)| t.to_state(i))
        .collect();
    let saved_ids: Vec<_> = snapshot.iter().map(|t| (t.id, t.sql.clone())).collect();
    let svc = services.clone();
    match background(
        &services,
        async move { svc.store.save_tabs(&snapshot).await },
    )
    .await
    {
        Ok(Ok(())) => {
            for tab in state.tabs.write().iter_mut() {
                // Only clear the flag if nothing changed since the snapshot.
                if saved_ids
                    .iter()
                    .any(|(id, sql)| *id == tab.id && *sql == tab.sql)
                {
                    tab.unsaved = false;
                }
            }
        }
        Ok(Err(e)) => tracing::warn!(error = %e, "failed to save tabs"),
        Err(e) => tracing::warn!(error = %e, "failed to save tabs"),
    }
}

pub fn save_settings(state: AppState, services: AppServices) {
    let settings = state.settings.peek().clone();
    let svc = services.clone();
    spawn(async move {
        if let Ok(Err(e)) = background(&services, async move {
            svc.store.save_settings(&settings).await
        })
        .await
        {
            state.notify(format!("Could not save settings: {e}"), true);
        }
    });
}

pub fn new_tab(mut state: AppState, services: AppServices) {
    let profile = state.active_tab().and_then(|t| t.profile_id);
    let n = state.tabs.peek().len() + 1;
    let tab = EditorTab::new(n, profile);
    state.active_tab.set(Some(tab.id));
    state.tabs.write().push(tab);
    spawn(save_tabs(state, services));
}

pub fn close_tab(mut state: AppState, services: AppServices, id: ferrum_domain::EditorTabId) {
    if let Some(run) = state.runs.write().remove(&id) {
        services.queries.cancel(run.execution_id);
    }
    let mut tabs = state.tabs.write();
    let Some(idx) = tabs.iter().position(|t| t.id == id) else {
        return;
    };
    tabs.remove(idx);
    if tabs.is_empty() {
        tabs.push(EditorTab::new(1, None));
    }
    let next = tabs
        .get(idx.saturating_sub(1))
        .or(tabs.first())
        .map(|t| t.id);
    drop(tabs);
    if *state.active_tab.peek() == Some(id) {
        state.active_tab.set(next);
    }
    spawn(save_tabs(state, services));
}
