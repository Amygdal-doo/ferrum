//! The tokio runtime FerrumChat's network and process work runs on. GPUI
//! has its own executor, so the core keeps a small shared runtime and the
//! UI awaits channels and join handles that work on any executor.

use std::sync::OnceLock;

use tokio::runtime::{Builder, Handle, Runtime};

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// The current runtime when called inside one, else the shared one
/// (started on first use, two worker threads).
pub fn handle() -> Handle {
    if let Ok(handle) = Handle::try_current() {
        return handle;
    }
    RUNTIME
        .get_or_init(|| {
            Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("ferrumagent")
                .enable_all()
                .build()
                .unwrap_or_else(|e| panic!("cannot start the FerrumChat runtime: {e}"))
        })
        .handle()
        .clone()
}
