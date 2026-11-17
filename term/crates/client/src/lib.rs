//! Client for the FerrumTerm server: connect over the local Unix socket (or
//! token-authenticated TCP to a remote server on your tailnet), send
//! requests, receive events. Used by `fterm`, `fterm attach` and the GUI.

pub mod paths;

use ferrumterm_protocol::{
    ClientMsg, Event, PROTOCOL_VERSION, Request, Response, ServerInfo, ServerMsg, read_msg,
    write_msg,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    Unix(PathBuf),
    Tcp { addr: String, token: Option<String> },
}

impl Endpoint {
    /// `unix:///path`, `/path`, `tcp://host:port` or `host:port`.
    #[must_use]
    pub fn parse(s: &str, token: Option<String>) -> Self {
        if let Some(p) = s.strip_prefix("unix://") {
            return Self::Unix(PathBuf::from(p));
        }
        if s.starts_with('/') || s.starts_with('~') {
            return Self::Unix(PathBuf::from(s));
        }
        let addr = s.strip_prefix("tcp://").unwrap_or(s);
        let addr = if addr.contains(':') {
            addr.to_owned()
        } else {
            format!("{addr}:{}", ferrumterm_protocol::DEFAULT_PORT)
        };
        Self::Tcp { addr, token }
    }
}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unix(p) => write!(f, "unix://{}", p.display()),
            Self::Tcp { addr, .. } => write!(f, "tcp://{addr}"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("cannot reach the FerrumTerm server at {0}: {1}")]
    Connect(String, String),
    #[error("{0}")]
    Server(String),
    #[error("connection to the server was lost")]
    Disconnected,
    #[error("the server did not answer within {0:?}")]
    Timeout(Duration),
    /// Something listens on the socket but does not speak this version's
    /// protocol: usually a server started by an older build.
    #[error("an older FerrumTerm server is running at {0} ({1}); restart it to continue")]
    Incompatible(String, String),
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Response>>>>;

/// A connection. Cheap to clone; requests may be issued concurrently.
#[derive(Clone)]
pub struct Client {
    tx: mpsc::Sender<ClientMsg>,
    pending: Pending,
    next: Arc<AtomicU64>,
    pub endpoint: Endpoint,
    /// What the server said about itself in its `Welcome`.
    pub server: Option<ServerInfo>,
}

impl Client {
    /// Connects and says hello. Events (frames, changes) arrive on the receiver.
    pub async fn connect(
        endpoint: &Endpoint,
        name: &str,
    ) -> Result<(Self, mpsc::UnboundedReceiver<Event>), ClientError> {
        let err = |e: std::io::Error| ClientError::Connect(endpoint.to_string(), e.to_string());
        let (client, events) = match endpoint {
            Endpoint::Unix(p) => {
                let s = tokio::net::UnixStream::connect(p).await.map_err(err)?;
                let (r, w) = s.into_split();
                Self::start(r, w, endpoint.clone())
            }
            Endpoint::Tcp { addr, .. } => {
                let s = tokio::time::timeout(
                    Duration::from_secs(5),
                    tokio::net::TcpStream::connect(addr),
                )
                .await
                .map_err(|_| ClientError::Connect(endpoint.to_string(), "timed out".into()))?
                .map_err(err)?;
                let _ = s.set_nodelay(true);
                let (r, w) = s.into_split();
                Self::start(r, w, endpoint.clone())
            }
        };
        let token = match endpoint {
            Endpoint::Tcp { token, .. } => token.clone(),
            Endpoint::Unix(_) => None,
        };
        let mut client = client;
        let welcome = client
            .request(Request::Hello {
                client: name.to_owned(),
                token,
                protocol: PROTOCOL_VERSION,
                version: Some(ferrumterm_protocol::build_version().to_owned()),
                revision: Some(ferrumterm_protocol::SERVER_REVISION),
            })
            .await?;
        if let Response::Welcome { info } = welcome {
            client.server = Some(info);
        }
        Ok((client, events))
    }

    fn start<R, W>(mut r: R, mut w: W, endpoint: Endpoint) -> (Self, mpsc::UnboundedReceiver<Event>)
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (tx, mut rx) = mpsc::channel::<ClientMsg>(256);
        let (ev_tx, ev_rx) = mpsc::unbounded_channel();
        let pending: Pending = Arc::default();
        tokio::spawn(async move {
            while let Some(m) = rx.recv().await {
                if write_msg(&mut w, &m).await.is_err() {
                    break;
                }
            }
        });
        let p = pending.clone();
        tokio::spawn(async move {
            while let Ok(msg) = read_msg::<_, ServerMsg>(&mut r).await {
                match msg {
                    ServerMsg::Reply { id, res } => {
                        if let Some(s) =
                            p.lock().unwrap_or_else(PoisonError::into_inner).remove(&id)
                        {
                            let _ = s.send(res);
                        }
                    }
                    ServerMsg::Event(e) => {
                        let _ = ev_tx.send(e);
                    }
                }
            }
            // Fail every outstanding request.
            p.lock().unwrap_or_else(PoisonError::into_inner).clear();
        });
        (
            Self {
                tx,
                pending,
                next: Arc::new(AtomicU64::new(1)),
                endpoint,
                server: None,
            },
            ev_rx,
        )
    }

    /// Sends a request; `Response::Error` becomes `Err`.
    pub async fn request(&self, req: Request) -> Result<Response, ClientError> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, tx);
        self.tx
            .send(ClientMsg { id, req })
            .await
            .map_err(|_| ClientError::Disconnected)?;
        match rx.await.map_err(|_| ClientError::Disconnected)? {
            Response::Error { message } => Err(ClientError::Server(message)),
            r => Ok(r),
        }
    }

    /// Like [`request`](Self::request) with a deadline.
    pub async fn request_timeout(
        &self,
        req: Request,
        timeout: Duration,
    ) -> Result<Response, ClientError> {
        tokio::time::timeout(timeout, self.request(req))
            .await
            .map_err(|_| ClientError::Timeout(timeout))?
    }

    /// How the server compares with this build (from its `Welcome`; a
    /// server that sent none counts as older).
    #[must_use]
    pub fn server_age(&self) -> ferrumterm_protocol::ServerAge {
        self.server
            .as_ref()
            .map_or(ferrumterm_protocol::ServerAge::Older, |i| {
                ferrumterm_protocol::version::compare_with_this_build(&i.version, i.revision)
            })
    }

    /// The server's [`ferrumterm_protocol::SERVER_REVISION`] (0 when it did
    /// not say). Requests it does not know end the connection, so check
    /// this before sending newer ones.
    #[must_use]
    pub fn server_revision(&self) -> u32 {
        self.server.as_ref().map_or(0, |i| i.revision)
    }

    pub async fn info(&self) -> Result<ServerInfo, ClientError> {
        match self.request(Request::ServerInfo).await? {
            Response::Info { info } => Ok(info),
            _ => Err(ClientError::Server("unexpected reply".into())),
        }
    }
}

/// The `fterm` that starts servers for this app: the one next to the current
/// executable (FerrumTerm.app ships both), else the installed FerrumTerm.app's
/// (apps that embed terminals, like FerrumIDE, don't ship one), else `PATH`.
#[must_use]
pub fn fterm_binary() -> PathBuf {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("fterm")));
    // Where the Ferrum installer and drag-installs put FerrumTerm.app.
    const APP: &str = "FerrumTerm.app/Contents/MacOS/fterm";
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let installed = ["/Applications/Ferrum", "/Applications"]
        .into_iter()
        .map(PathBuf::from)
        .chain(
            home.iter()
                .flat_map(|h| [h.join("Applications/Ferrum"), h.join("Applications")]),
        )
        .map(|dir| dir.join(APP));
    beside
        .into_iter()
        .chain(installed)
        .find(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("fterm"))
}

/// Connects to the local server, starting it with `fterm server start` when
/// nothing is listening (`fterm` from [`fterm_binary`]).
pub async fn connect_or_start(
    socket: &Path,
    name: &str,
) -> Result<(Client, mpsc::UnboundedReceiver<Event>), ClientError> {
    let endpoint = Endpoint::Unix(socket.to_path_buf());
    match Client::connect(&endpoint, name).await {
        Ok(c) => return Ok(c),
        // Something answers but refuses us: starting another server would
        // only fail on the busy socket, so say what is going on instead.
        Err(e) if socket_is_live(socket).await => {
            return Err(ClientError::Incompatible(
                endpoint.to_string(),
                e.to_string(),
            ));
        }
        Err(_) => {}
    }
    let fterm = fterm_binary();
    let status = tokio::process::Command::new(&fterm)
        .args(["server", "start"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status()
        .await
        .map_err(|e| {
            ClientError::Connect(
                endpoint.to_string(),
                format!("could not run {}: {e}", fterm.display()),
            )
        })?;
    if !status.success() {
        return Err(ClientError::Connect(
            endpoint.to_string(),
            "`fterm server start` failed (see `fterm server logs`)".into(),
        ));
    }
    let mut last = None;
    for _ in 0..50 {
        match Client::connect(&endpoint, name).await {
            Ok(c) => return Ok(c),
            Err(e) => last = Some(e),
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(last.unwrap_or(ClientError::Disconnected))
}

/// Whether a process accepts connections on `socket`.
pub async fn socket_is_live(socket: &Path) -> bool {
    tokio::net::UnixStream::connect(socket).await.is_ok()
}

/// Stops the server on `socket` even when it is too old to talk to: asks a
/// compatible one to shut down, otherwise sends SIGTERM to the process that
/// holds the socket (found with `lsof`). Every program in it is terminated.
pub async fn force_stop(socket: &Path) -> Result<(), String> {
    let endpoint = Endpoint::Unix(socket.to_path_buf());
    if let Ok((client, _)) = Client::connect(&endpoint, "fterm-stop").await {
        let _ = client
            .request_timeout(Request::Shutdown, Duration::from_secs(5))
            .await;
    } else if socket_is_live(socket).await {
        let out = tokio::process::Command::new("lsof")
            .arg("-t")
            .arg("--")
            .arg(socket)
            .output()
            .await
            .map_err(|e| format!("could not run lsof: {e}"))?;
        let own = std::process::id().to_string();
        let pids: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .filter(|p| *p != own)
            .map(str::to_owned)
            .collect();
        if pids.is_empty() {
            return Err(format!("no process found holding {}", socket.display()));
        }
        for pid in &pids {
            let _ = tokio::process::Command::new("kill")
                .args(["-TERM", pid])
                .status()
                .await;
        }
    }
    for _ in 0..50 {
        if !socket_is_live(socket).await {
            let _ = std::fs::remove_file(socket);
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!("the server on {} did not stop", socket.display()))
}

#[cfg(test)]
mod tests {
    use super::Endpoint;
    use std::path::PathBuf;

    #[test]
    fn parses_endpoints() {
        assert_eq!(
            Endpoint::parse("/tmp/s.sock", None),
            Endpoint::Unix(PathBuf::from("/tmp/s.sock"))
        );
        assert_eq!(
            Endpoint::parse("unix:///x", None),
            Endpoint::Unix(PathBuf::from("/x"))
        );
        assert_eq!(
            Endpoint::parse("tcp://100.1.2.3:9", None),
            Endpoint::Tcp {
                addr: "100.1.2.3:9".into(),
                token: None
            }
        );
        assert_eq!(
            Endpoint::parse("mac-mini", Some("t".into())),
            Endpoint::Tcp {
                addr: "mac-mini:7397".into(),
                token: Some("t".into())
            }
        );
    }
}
