//! A small Tokio runtime for the account's network and keychain work
//! (`ferrum-account` needs Tokio; the apps' UI runs on GPUI's executor).
//! Started on first use; awaiting [`run`] from a GPUI task never blocks the
//! UI thread.

use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::{Builder, Runtime};

static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();

fn runtime() -> Result<&'static Runtime, String> {
    RUNTIME
        .get_or_init(|| {
            Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("ferrum-account")
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Runs `future` on the account runtime and waits for it.
pub async fn run<F>(future: F) -> Result<F::Output, String>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let handle = runtime()?.spawn(future);
    handle.await.map_err(|e| e.to_string())
}
