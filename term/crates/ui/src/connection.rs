//! Where the terminals live: the shared `fterm` server (started on demand,
//! so sessions outlive the host app), or — when it can't be reached or
//! started, or the host asked for it — a private server inside this process,
//! shared by every view of the process and gone when the process exits.

use ferrumterm_client::{Client, Endpoint, connect_or_start, paths};
use ferrumterm_protocol::{AwakeMode, Event};
use ferrumterm_server::ServerConfig;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::{OnceCell, mpsc};

/// Which server a view is connected to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// The shared, persistent `fterm` server: terminals survive the host app
    /// closing, and show in FerrumTerm and `fterm attach`.
    Server,
    /// A private server inside the host process: terminals end with it.
    Local,
}

/// The tokio runtime behind every view's networking (two workers, started
/// on first use).
pub(crate) fn runtime() -> tokio::runtime::Handle {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("ferrumterm-ui")
                .enable_all()
                .build()
                .expect("FerrumTerm: could not start the tokio runtime")
        })
        .handle()
        .clone()
}

/// Prefix of the error [`connect`] returns for a server from an older build.
pub(crate) const INCOMPATIBLE: &str = "incompatible:";

/// Stops the shared server even if it is too old to talk to.
pub(crate) async fn restart_shared() -> Result<(), String> {
    ferrumterm_client::force_stop(&paths::socket()).await
}

pub(crate) type Connected = (Client, mpsc::UnboundedReceiver<Event>, Backend);

/// Connects to the shared server (starting it when `attach`), else — or,
/// with `fallback`, when that fails — to the private one.
pub(crate) async fn connect(attach: bool, fallback: bool, name: &str) -> Result<Connected, String> {
    if attach {
        match connect_or_start(&paths::socket(), name).await {
            Ok((client, events)) => return Ok((client, events, Backend::Server)),
            Err(error @ ferrumterm_client::ClientError::Incompatible(..)) if !fallback => {
                return Err(format!("{INCOMPATIBLE}{error}"));
            }
            Err(error) if !fallback => return Err(error.to_string()),
            Err(error) => {
                tracing::warn!(%error, "shared FerrumTerm server unavailable; using a private one");
            }
        }
    }
    let socket = local_server().await?;
    let (client, events) = Client::connect(&Endpoint::Unix(socket), "ferrumterm-embedded")
        .await
        .map_err(|e| e.to_string())?;
    Ok((client, events, Backend::Local))
}

/// The private server's socket, starting the server on first use.
async fn local_server() -> Result<PathBuf, String> {
    static LOCAL: OnceCell<PathBuf> = OnceCell::const_new();
    LOCAL.get_or_try_init(start_local).await.cloned()
}

/// A short, process-private directory for the private server's socket and
/// token (socket paths are limited to ~104 bytes).
fn local_dir() -> PathBuf {
    let name = format!("fterm-embed-{}", std::process::id());
    let preferred = std::env::temp_dir().join(&name);
    if preferred.as_os_str().len() < 80 {
        preferred
    } else {
        PathBuf::from("/tmp").join(name)
    }
}

async fn start_local() -> Result<PathBuf, String> {
    let dir = local_dir();
    let socket = dir.join("server.sock");
    let config = ServerConfig {
        state_dir: dir.clone(),
        socket: socket.clone(),
        tcp: Vec::new(),
        awake: AwakeMode::Auto,
        notify: false,
    };
    let (failed_tx, mut failed) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        // Runs for the rest of the process; no signal handlers are installed.
        if let Err(error) = ferrumterm_server::run_until(config, std::future::pending()).await {
            tracing::warn!(%error, "private terminal server failed");
            let _ = failed_tx.send(error.to_string());
        }
    });
    for _ in 0..100 {
        if tokio::net::UnixStream::connect(&socket).await.is_ok() {
            return Ok(socket);
        }
        if let Ok(error) = failed.try_recv() {
            return Err(format!("could not start a terminal server: {error}"));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("the private terminal server did not start".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_socket_paths_stay_short() {
        let socket = local_dir().join("server.sock");
        assert!(socket.as_os_str().len() < 100, "{}", socket.display());
    }

    #[test]
    fn the_private_server_accepts_clients() {
        let rt = runtime();
        let result = rt.block_on(async {
            let (client, _events, backend) = connect(false, true, "test").await?;
            client.info().await.map_err(|e| e.to_string())?;
            Ok::<_, String>(backend)
        });
        assert_eq!(result, Ok(Backend::Local));
    }
}
