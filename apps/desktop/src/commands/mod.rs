//! UI commands. Each one hands I/O to the services runtime via
//! `AppServices::spawn` and only touches signals from the UI task.

pub mod collections;
pub mod environments;
pub mod files;
pub mod history;
pub mod send;
pub mod workspace;

use ferrumapi_application::AppServices;
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
