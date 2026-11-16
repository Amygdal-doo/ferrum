//! UI commands. Each one hands I/O to the services runtime via
//! `AppServices::spawn` and only touches state from the UI thread.

pub mod auth;
pub mod collections;
pub mod cookies;
pub mod drafts;
pub mod environments;
pub mod files;
pub mod history;
pub mod load;
pub mod mock;
pub mod projects;
pub mod protocols;
pub mod runner;
pub mod scripts;
pub mod send;
pub mod webhook;
pub mod websocket;
pub mod workspace;

use crate::app::FerrumApi;
use ferrumapi_application::AppServices;
use gpui::Context;
use std::future::Future;

/// Runs `future` on the services runtime and awaits it from the UI task.
/// A panic in the background task is surfaced as an error string.
pub async fn background<F, T>(services: &AppServices, future: F) -> Result<T, String>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    services
        .spawn(future)
        .await
        .map_err(|e| format!("background task failed: {e}"))
}

/// Flattens a background result whose inner error is displayable.
pub async fn run<F, T, E>(services: &AppServices, future: F) -> Result<T, String>
where
    F: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    background(services, future)
        .await
        .and_then(|r| r.map_err(|e| e.to_string()))
}

impl FerrumApi {
    /// Runs fallible I/O on the services runtime, then hands the result to
    /// `then` on the UI thread and re-renders.
    pub fn io<F, T, E>(
        &self,
        cx: &mut Context<Self>,
        future: F,
        then: impl FnOnce(&mut Self, Result<T, String>, &mut Context<Self>) + 'static,
    ) where
        F: Future<Output = Result<T, E>> + Send + 'static,
        T: Send + 'static,
        E: std::fmt::Display + Send + 'static,
    {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let result = run(&services, future).await;
            if let Err(error) = this.update(cx, |this, cx| {
                then(this, result, cx);
                cx.notify();
            }) {
                tracing::warn!(%error, "window closed before background task finished");
            }
        })
        .detach();
    }

    /// `then` for fire-and-forget saves: errors become a notice.
    pub fn report<T>(
        prefix: &'static str,
    ) -> impl FnOnce(&mut Self, Result<T, String>, &mut Context<Self>) {
        move |this, result, cx| {
            if let Err(e) = result {
                this.notify(format!("{prefix}: {e}"), true, cx);
            }
        }
    }
}
