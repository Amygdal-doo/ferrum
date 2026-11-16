//! UI commands. Each one hands I/O to the services runtime via
//! `AppServices::spawn` and only touches view state from the UI thread.

pub mod compare;
pub mod connection;
pub mod edit;
pub mod explain;
pub mod export;
pub mod history;
pub mod keys;
pub mod query;
pub mod schema;
pub mod tools;
pub mod transfer;
pub mod workspace;

use ferrumdata_application::AppServices;
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

/// Logs when an async command finishes after its view was closed.
pub fn log_dropped(result: ferrum_ui::gpui::Result<()>) {
    if let Err(e) = result {
        tracing::debug!(error = %e, "view dropped before command finished");
    }
}
