//! In-process application services.
//!
//! Services run on a dedicated multi-threaded Tokio runtime owned by
//! [`AppServices`], so database and disk I/O never execute on the UI thread.
//! UI code calls [`AppServices::spawn`] and awaits the returned handle.

pub mod browse;
mod connections;
mod queries;
pub mod safety;

pub use connections::{ConnectionService, PasswordChange};
pub use ferrumdata_persistence::HistoryQuery;
pub use queries::QueryService;

use ferrumdata_persistence::{AppStore, StoreError};
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

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
    pub store: AppStore,
    pub connections: ConnectionService,
    pub queries: QueryService,
}

impl AppServices {
    /// Starts the runtime and opens local state at `data_dir/ferrumdata.db`.
    /// Blocks the caller briefly; call before the UI event loop starts.
    pub fn start(data_dir: &Path) -> Result<Self, AppError> {
        std::fs::create_dir_all(data_dir)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ferrumdata-services")
            .enable_all()
            .build()?;
        let store = runtime.block_on(AppStore::open(&data_dir.join("ferrumdata.db")))?;
        let credentials = ferrumdata_credentials::default_store();
        let connections = ConnectionService::new(store.clone(), credentials);
        let queries = QueryService::new(connections.clone());
        Ok(Self {
            runtime: Arc::new(RuntimeGuard(Some(runtime))),
            store,
            connections,
            queries,
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
        let dir = std::env::temp_dir().join(format!("ferrumdata-svc-{}", uuid::Uuid::new_v4()));
        let services = AppServices::start(&dir).expect("start");
        let outer = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("rt");
        outer.block_on(async move { drop(services) });
        let _ = std::fs::remove_dir_all(dir);
    }
}
