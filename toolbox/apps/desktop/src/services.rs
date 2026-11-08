//! Background runtime for network and file work.

use ferrumtoolbox_core::{GitHubClient, Paths, Settings, ToolboxError, auth};
use std::future::Future;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Owns the tokio runtime. Cheap to clone.
#[derive(Clone)]
pub struct Services {
    runtime: Arc<Runtime>,
    pub paths: Paths,
}

impl Services {
    pub fn start(paths: Paths) -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ferrum-toolbox")
            .enable_all()
            .build()?;
        Ok(Self {
            runtime: Arc::new(runtime),
            paths,
        })
    }

    /// Runs `future` on the runtime and awaits it from the UI task. A panic
    /// in the task surfaces as an error string.
    pub async fn run<F, T>(&self, future: F) -> Result<T, String>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.runtime
            .spawn(future)
            .await
            .map_err(|e| format!("background task failed: {e}"))
    }

    /// Runs blocking work (keychain, file system) on the runtime's blocking pool.
    pub async fn blocking<F, T>(&self, work: F) -> Result<T, String>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        self.run(async move { tokio::task::spawn_blocking(work).await })
            .await?
            .map_err(|e| format!("background task failed: {e}"))
    }
}

/// Builds a client for `settings`, resolving the token (blocking: keychain/`gh`).
pub fn client_for(settings: &Settings) -> Result<GitHubClient, ToolboxError> {
    GitHubClient::new(
        &settings.repo_owner,
        &settings.repo_name,
        auth::resolve_token(),
    )
}
