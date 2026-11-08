//! Sending and cancelling requests.

use super::background;
use crate::app::FerrumApi;
use crate::state::TabResponse;
use ferrumapi_application::CancellationToken;
use ferrumapi_domain::{ExecError, TabId};
use gpui::Context;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static NEXT_RUN: AtomicU64 = AtomicU64::new(1);

impl FerrumApi {
    /// Sends the active tab's request, or cancels it if one is in flight.
    pub fn send_or_cancel_active(&mut self, cx: &mut Context<Self>) {
        let Some(tab_id) = self.state.active_tab else {
            return;
        };
        if self.state.is_loading(tab_id) {
            self.cancel(tab_id);
        } else {
            self.send(tab_id, cx);
        }
    }

    pub fn cancel(&mut self, tab_id: TabId) {
        if let Some(token) = self.state.cancels.remove(&tab_id) {
            token.cancel();
        }
    }

    pub fn send(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        let Some(tab) = self.state.tab(tab_id).cloned() else {
            return;
        };
        if tab.request.url.trim().is_empty() {
            self.notify("Enter a URL first.", true, cx);
            return;
        }
        let prepared = self.state.prepare_tab(&tab);
        let token = CancellationToken::new();
        let run_id = NEXT_RUN.fetch_add(1, Ordering::Relaxed);
        self.state.cancels.insert(tab_id, token.clone());
        self.state.responses.insert(
            tab_id,
            TabResponse::Loading {
                run: run_id,
                started: Instant::now(),
            },
        );
        tracing::info!(tab = %tab_id, method = %tab.request.method, "request started");

        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let (authored, origin) = (tab.request.clone(), tab.origin);
            let result = background(&services, async move {
                svc.send(authored, prepared, origin, token).await
            })
            .await;
            let update = this.update(cx, |this, cx| {
                let state = &mut this.state;
                let still_current = matches!(
                    state.responses.get(&tab_id),
                    Some(TabResponse::Loading { run, .. }) if *run == run_id
                );
                if !still_current {
                    return;
                }
                state.cancels.remove(&tab_id);
                let next = match result {
                    Ok(Ok(resp)) => {
                        tracing::info!(tab = %tab_id, status = resp.status, "request finished");
                        TabResponse::Done(Box::new(resp))
                    }
                    Ok(Err(ExecError::Cancelled)) => TabResponse::Cancelled,
                    Ok(Err(e)) => TabResponse::Failed(e.to_string()),
                    Err(e) => TabResponse::Failed(e),
                };
                state.responses.insert(tab_id, next);
                this.refresh_history(cx);
                cx.notify();
            });
            if let Err(error) = update {
                tracing::warn!(%error, "window closed while a request was in flight");
            }
        })
        .detach();
    }
}
