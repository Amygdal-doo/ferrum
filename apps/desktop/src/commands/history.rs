//! History list refresh and deletion.

use super::run;
use super::workspace::history_limit;
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumapi_application::AppServices;
use ferrumapi_domain::HistoryId;

pub fn refresh(mut state: AppState, services: AppServices) {
    let limit = history_limit(state);
    let svc = services.clone();
    spawn(async move {
        match run(
            &services,
            async move { svc.store.list_history(limit).await },
        )
        .await
        {
            Ok(list) => state.history.set(list),
            Err(e) => tracing::warn!(error = %e, "failed to load history"),
        }
    });
}

pub fn delete(mut state: AppState, services: AppServices, id: HistoryId) {
    state.history.write().retain(|h| h.id != id);
    let svc = services.clone();
    spawn(async move {
        if let Err(e) = run(&services, async move { svc.store.delete_history(id).await }).await {
            state.notify(format!("Could not delete history entry: {e}"), true);
        }
    });
}

pub fn clear(mut state: AppState, services: AppServices) {
    state.history.write().clear();
    let svc = services.clone();
    spawn(async move {
        if let Err(e) = run(&services, async move { svc.store.clear_history().await }).await {
            state.notify(format!("Could not clear history: {e}"), true);
        }
    });
}
