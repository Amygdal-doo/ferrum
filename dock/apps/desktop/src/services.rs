//! The Tokio runtime engine I/O runs on (GPUI has its own executor).

use std::future::Future;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

/// Shuts the runtime down in the background on drop (a blocking drop panics
/// inside the UI's async context at shutdown).
struct RuntimeGuard(Option<Runtime>);

impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        if let Some(rt) = self.0.take() {
            rt.shutdown_background();
        }
    }
}

/// Cheap to clone.
#[derive(Clone)]
pub struct Services {
    runtime: Arc<RuntimeGuard>,
}

impl Services {
    pub fn start() -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ferrumdock-engine")
            .enable_all()
            .build()?;
        Ok(Self {
            runtime: Arc::new(RuntimeGuard(Some(runtime))),
        })
    }

    /// Runs `future` on the runtime; the handle can be awaited from any executor.
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        match &self.runtime.0 {
            Some(rt) => rt.spawn(future),
            None => unreachable!("runtime used after shutdown"),
        }
    }
}
