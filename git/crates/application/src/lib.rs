//! In-process application services.
//!
//! Services run on a dedicated Tokio runtime owned by [`AppServices`], so git
//! processes, keychain calls and SQLite I/O never block the UI thread. UI
//! code calls [`AppServices::spawn`] and awaits the handle.

mod profiles;
mod pulls;
mod repos;

pub use ferrumgit_domain as domain;
pub use ferrumgit_forge as forge;
pub use ferrumgit_git::{
    CancellationToken, GitError, MAX_PATCH_BYTES, Snapshot, askpass_main, patch,
};
pub use ferrumgit_graph::{Edge, GraphRow};
pub use ferrumgit_persistence::{Store, StoreError};
pub use profiles::PassphraseChange;
pub use pulls::{ForgeInfo, GhLookup, PrChanges};
pub use repos::RepoData;

use ferrumgit_credentials::CredentialStore;
use ferrumgit_git::{Git, Repo};
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

/// Default number of commits loaded into the graph.
pub const DEFAULT_LOG_LIMIT: usize = 2000;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("failed to start background runtime: {0}")]
    Runtime(#[from] std::io::Error),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Forge(#[from] ferrumgit_forge::ForgeError),
    #[error("{0}")]
    Credentials(String),
    #[error("{0}")]
    Invalid(String),
}

pub type AppResult<T> = Result<T, AppError>;

/// Shuts the runtime down in the background on drop (blocking drop panics
/// inside the UI's async context at shutdown).
struct RuntimeGuard(Option<Runtime>);

impl RuntimeGuard {
    fn get(&self) -> &Runtime {
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
    pub git: Git,
    creds: Arc<dyn CredentialStore>,
    /// Opened repositories by path (saves a `rev-parse` per call).
    repos: Arc<Mutex<HashMap<String, Repo>>>,
    /// Forge tokens, HTTP transport and rate limits (pull requests).
    forge: Arc<pulls::ForgeState>,
}

impl AppServices {
    /// Starts the runtime and opens `data_dir/ferrumgit.db`. The current
    /// executable doubles as the SSH askpass helper.
    pub fn start(data_dir: &Path) -> AppResult<Self> {
        let mut git = Git::new();
        if let Ok(exe) = std::env::current_exe() {
            git = git.with_askpass(exe);
        }
        Ok(
            Self::start_with(data_dir, git, ferrumgit_credentials::default_store())?
                .with_token_store(ferrumgit_credentials::default_token_store()),
        )
    }

    /// Forge tokens stay in session memory unless [`Self::with_token_store`] is used.
    pub fn start_with(
        data_dir: &Path,
        git: Git,
        creds: Arc<dyn CredentialStore>,
    ) -> AppResult<Self> {
        std::fs::create_dir_all(data_dir)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ferrumgit-services")
            .enable_all()
            .build()?;
        let store = runtime.block_on(Store::open(&data_dir.join("ferrumgit.db")))?;
        Ok(Self {
            runtime: Arc::new(RuntimeGuard(Some(runtime))),
            store,
            git,
            creds,
            repos: Arc::default(),
            forge: Arc::new(pulls::ForgeState::new(Arc::new(
                ferrumgit_credentials::SessionStore::default(),
            ))),
        })
    }

    /// Runs `future` on the services runtime; the handle can be awaited from any executor.
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.runtime.get().spawn(future)
    }

    pub(crate) async fn repo(&self, path: &str) -> AppResult<Repo> {
        if let Some(r) = self
            .repos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
        {
            return Ok(r.clone());
        }
        let repo = Repo::open(&self.git, Path::new(path)).await?;
        self.repos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(path.to_owned(), repo.clone());
        Ok(repo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropping_services_inside_async_context_does_not_panic() {
        let dir = tempfile_dir();
        let services = AppServices::start(&dir).expect("start");
        let outer = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("rt");
        outer.block_on(async move { drop(services) });
        let _ = std::fs::remove_dir_all(dir);
    }

    fn tempfile_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ferrumgit-svc-{}", uuid::Uuid::new_v4()))
    }
}
