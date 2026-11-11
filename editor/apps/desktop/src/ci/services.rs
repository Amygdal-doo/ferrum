//! The CI page's background runtime plus everything that touches the
//! outside world (HTTP, keychain, notifications, recent projects),
//! swappable in tests.

use ferrumci_core::Transport;
use ferrumci_core::auth::{SystemTokens, TokenProvider};
use ferrumci_core::cache::EtagTransport;
use ferrumgit_forge::ReqwestTransport;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Runtime;

pub type Notifier = Arc<dyn Fn(&str, &str) + Send + Sync>;
pub type RecentProjects = Arc<dyn Fn() -> Vec<PathBuf> + Send + Sync>;

/// Cheap to clone.
#[derive(Clone)]
pub struct Services {
    runtime: Arc<Runtime>,
    pub transport: Arc<dyn Transport>,
    pub tokens: Arc<dyn TokenProvider>,
    pub notify: Notifier,
    pub recent: RecentProjects,
}

impl Services {
    pub fn start() -> std::io::Result<Self> {
        let http = ReqwestTransport::new().map_err(std::io::Error::other)?;
        Self::with(
            Arc::new(EtagTransport::new(Arc::new(http))),
            Arc::new(SystemTokens::new()),
            Arc::new(notify_macos),
            Arc::new(|| {
                ferrum_ui::project::projects_file()
                    .map(|f| ferrum_ui::project::load(&f))
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| p.path)
                    .collect()
            }),
        )
    }

    pub fn with(
        transport: Arc<dyn Transport>,
        tokens: Arc<dyn TokenProvider>,
        notify: Notifier,
        recent: RecentProjects,
    ) -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ferrumeditor-ci")
            .enable_all()
            .build()?;
        Ok(Self {
            runtime: Arc::new(runtime),
            transport,
            tokens,
            notify,
            recent,
        })
    }

    /// Runs `future` on the runtime and awaits it from the UI.
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

    /// Blocking work (keychain, `gh`) on the runtime's blocking pool.
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

/// A macOS notification through `osascript`; text goes in as arguments,
/// never into the script. Elsewhere the in-app notice is enough.
fn notify_macos(title: &str, body: &str) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let result = std::process::Command::new("osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "display notification (item 2 of argv) with title (item 1 of argv)",
            "-e",
            "end run",
            title,
            body,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if let Err(e) = result {
        tracing::debug!(error = %e, "notification failed");
    }
}
