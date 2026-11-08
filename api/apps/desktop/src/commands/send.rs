//! Sending and cancelling requests.

use super::background;
use crate::state::{AppState, TabResponse};
use dioxus::prelude::*;
use ferrumapi_application::{AppServices, CancellationToken};
use ferrumapi_domain::{ExecError, TabId};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static NEXT_RUN: AtomicU64 = AtomicU64::new(1);

/// Sends the active tab's request, or cancels it if one is in flight.
pub fn send_or_cancel_active(state: AppState, services: AppServices) {
    let Some(tab_id) = *state.active_tab.peek() else {
        return;
    };
    let loading = state
        .responses
        .peek()
        .get(&tab_id)
        .is_some_and(TabResponse::is_loading);
    if loading {
        cancel(state, tab_id);
    } else {
        send(state, services, tab_id);
    }
}

pub fn cancel(mut state: AppState, tab_id: TabId) {
    if let Some(token) = state.cancels.write().remove(&tab_id) {
        token.cancel();
    }
}

pub fn send(mut state: AppState, services: AppServices, tab_id: TabId) {
    let Some(tab) = state.tabs.peek().iter().find(|t| t.id == tab_id).cloned() else {
        return;
    };
    if tab.request.url.trim().is_empty() {
        state.notify("Enter a URL first.", true);
        return;
    }
    let prepared = state.prepare_tab(&tab);
    let token = CancellationToken::new();
    let run_id = NEXT_RUN.fetch_add(1, Ordering::Relaxed);
    state.cancels.write().insert(tab_id, token.clone());
    state.responses.write().insert(
        tab_id,
        TabResponse::Loading {
            run: run_id,
            started: Instant::now(),
        },
    );
    tracing::info!(tab = %tab_id, method = %tab.request.method, "request started");

    spawn(async move {
        let svc = services.clone();
        let (authored, origin) = (tab.request.clone(), tab.origin);
        let result = background(&services, async move {
            svc.send(authored, prepared, origin, token).await
        })
        .await;
        let still_current = matches!(
            state.responses.peek().get(&tab_id),
            Some(TabResponse::Loading { run, .. }) if *run == run_id
        );
        if !still_current {
            return;
        }
        state.cancels.write().remove(&tab_id);
        let next = match result {
            Ok(Ok(resp)) => {
                tracing::info!(tab = %tab_id, status = resp.status, "request finished");
                TabResponse::Done(Box::new(resp))
            }
            Ok(Err(ExecError::Cancelled)) => TabResponse::Cancelled,
            Ok(Err(e)) => TabResponse::Failed(e.to_string()),
            Err(e) => TabResponse::Failed(e),
        };
        state.responses.write().insert(tab_id, next);
        super::history::refresh(state, services);
    });
}
