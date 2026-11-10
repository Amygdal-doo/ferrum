//! In-process application services.
//!
//! Services run on a dedicated multi-threaded Tokio runtime owned by
//! [`AppServices`], so HTTP, disk and SQLite I/O never execute on the UI
//! thread. UI code calls [`AppServices::spawn`] and awaits the handle.

mod cookies;
mod load;
mod mock;
mod runner;
mod send;
mod transfer;

pub use ferrumapi_domain as domain;
pub use ferrumapi_http as http;
pub use ferrumapi_http::CancellationToken;
pub use ferrumapi_interop as interop;
pub use ferrumapi_persistence as persistence;
pub use ferrumapi_persistence::{Store, StoreError};
pub use ferrumapi_server as server;
pub use ferrumapi_server::{MockServer, ServerError, WebhookInspector};
pub use load::{LoadEnd, LoadEvent, load_target_warning, run_load};
pub use mock::plan_mock_routes;
pub use runner::{RunEvent, RunItem, RunOutcome, plan_run, run_sequence};
pub use send::{Prepared, code_snippet, prepare, variables_for};
pub use transfer::TransferError;

use ferrumapi_http::HttpClient;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

/// Default number of history entries kept.
pub const DEFAULT_HISTORY_LIMIT: usize = 500;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("failed to start background runtime: {0}")]
    Runtime(#[from] std::io::Error),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Owns the services runtime. Dropping a Tokio runtime normally blocks, which
/// panics when the last handle is released inside the UI's async context at
/// shutdown, so it is shut down in the background instead.
struct RuntimeGuard(Option<Runtime>);

impl RuntimeGuard {
    fn get(&self) -> &Runtime {
        // Only `None` during drop.
        self.0
            .as_ref()
            .unwrap_or_else(|| unreachable!("runtime used after drop"))
    }
}

impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        if let Some(rt) = self.0.take() {
            rt.shutdown_background();
        }
    }
}

/// Root of the service graph. Cheap to clone.
#[derive(Clone)]
pub struct AppServices {
    runtime: Arc<RuntimeGuard>,
    pub store: Store,
    pub http: HttpClient,
    cookies: cookies::SharedJar,
}

impl AppServices {
    /// Starts the runtime and opens local state at `data_dir/ferrumapi.db`.
    /// Blocks the caller briefly; call before the UI event loop starts.
    pub fn start(data_dir: &Path) -> Result<Self, AppError> {
        std::fs::create_dir_all(data_dir)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ferrumapi-services")
            .enable_all()
            .build()?;
        let store = runtime.block_on(Store::open(&data_dir.join("ferrumapi.db")))?;
        Ok(Self {
            runtime: Arc::new(RuntimeGuard(Some(runtime))),
            store,
            http: HttpClient::new(),
            cookies: cookies::SharedJar::default(),
        })
    }

    /// Runs `future` on the services runtime. The handle can be awaited
    /// from any executor, including the UI's.
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.runtime.get().spawn(future)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: dropping the last handle inside an async context must not panic.
    #[test]
    fn dropping_services_inside_async_context_does_not_panic() {
        let dir = std::env::temp_dir().join(format!("ferrumapi-svc-{}", uuid::Uuid::new_v4()));
        let services = AppServices::start(&dir).expect("start");
        let outer = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("rt");
        outer.block_on(async move { drop(services) });
        let _ = std::fs::remove_dir_all(dir);
    }
}
