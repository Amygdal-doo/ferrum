//! Local HTTP servers for FerrumAPI, always bound to 127.0.0.1: the mock
//! server ([`MockServer`]) and the webhook inspector ([`WebhookInspector`]).
//!
//! Both sit on [`LocalServer`], a small hyper HTTP/1.1 accept loop that
//! hands each request, body read into memory, to an async handler. Nothing
//! here logs paths, headers or bodies.

mod mock;
mod webhook;

pub use mock::{MockServer, respond as mock_respond};
pub use tokio_util::sync::CancellationToken;
pub use webhook::{WebhookInspector, capture};

use bytes::Bytes;
use http_body_util::{BodyExt as _, Full};
use hyper::body::Incoming as HyperBody;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::future::Future;
use std::net::{Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Bytes of each request body kept in memory; the rest is read and dropped.
pub const MAX_BODY: usize = ferrumapi_domain::webhook::MAX_CAPTURE_BODY;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ServerError {
    #[error(
        "port {0} is already in use on 127.0.0.1 — stop the program using it or choose another port"
    )]
    PortInUse(u16),
    #[error("not allowed to listen on port {0} — choose a port above 1024")]
    PermissionDenied(u16),
    #[error("could not start the local server: {0}")]
    Io(String),
}

/// Binds `127.0.0.1:port` (`0` picks a free port). Never binds other interfaces.
pub async fn bind_local(port: u16) -> Result<TcpListener, ServerError> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AddrInUse => ServerError::PortInUse(port),
            std::io::ErrorKind::PermissionDenied => ServerError::PermissionDenied(port),
            _ => ServerError::Io(e.to_string()),
        })
}

/// A request as handed to a handler.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Incoming {
    pub method: String,
    pub path: String,
    /// Raw query string without `?`.
    pub query: String,
    pub headers: Vec<(String, String)>,
    /// At most [`MAX_BODY`] bytes.
    pub body: Vec<u8>,
    /// Full body size as received.
    pub size: usize,
}

/// A handler's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Outgoing {
    #[must_use]
    pub fn json(status: u16, body: String) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.into_bytes(),
        }
    }

    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

pub type BoxFuture = Pin<Box<dyn Future<Output = Outgoing> + Send>>;
pub type Handler = Arc<dyn Fn(Incoming) -> BoxFuture + Send + Sync>;

/// A running accept loop. Stops on [`LocalServer::stop`] or when dropped;
/// open connections are closed, in-flight requests abandoned.
#[derive(Debug)]
pub struct LocalServer {
    addr: SocketAddr,
    cancel: CancellationToken,
    task: Option<JoinHandle<()>>,
}

impl LocalServer {
    /// Serves `listener` with `handler` on the current Tokio runtime.
    pub fn spawn(listener: TcpListener, handler: Handler) -> Result<Self, ServerError> {
        let addr = listener
            .local_addr()
            .map_err(|e| ServerError::Io(e.to_string()))?;
        let cancel = CancellationToken::new();
        let task = tokio::spawn(accept_loop(listener, handler, cancel.clone()));
        tracing::info!(port = addr.port(), "local server started");
        Ok(Self {
            addr,
            cancel,
            task: Some(task),
        })
    }

    #[must_use]
    pub const fn addr(&self) -> SocketAddr {
        self.addr
    }

    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn stop(&self) {
        self.cancel.cancel();
    }

    /// Stops and waits until the port is released.
    pub async fn shutdown(mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

async fn accept_loop(listener: TcpListener, handler: Handler, cancel: CancellationToken) {
    loop {
        let accepted = tokio::select! {
            () = cancel.cancelled() => break,
            accepted = listener.accept() => accepted,
        };
        match accepted {
            Ok((stream, _)) => {
                let handler = handler.clone();
                let cancel = cancel.clone();
                tokio::spawn(async move {
                    let service = service_fn(move |req| {
                        let handler = handler.clone();
                        async move { Ok::<_, Infallible>(handle(&handler, req).await) }
                    });
                    let conn =
                        http1::Builder::new().serve_connection(TokioIo::new(stream), service);
                    tokio::select! {
                        () = cancel.cancelled() => {}
                        result = conn => {
                            if let Err(e) = result {
                                tracing::debug!(error = %e, "local server connection ended");
                            }
                        }
                    }
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, "local server accept failed");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
    tracing::info!("local server stopped");
}

async fn handle(handler: &Handler, req: Request<HyperBody>) -> Response<Full<Bytes>> {
    let (parts, mut body) = req.into_parts();
    let mut bytes = Vec::new();
    let mut size = 0;
    while let Some(frame) = body.frame().await {
        let Ok(frame) = frame else {
            return build(Outgoing::json(
                400,
                "{\"error\":\"unreadable body\"}".into(),
            ));
        };
        if let Some(data) = frame.data_ref() {
            size += data.len();
            let room = MAX_BODY.saturating_sub(bytes.len());
            bytes.extend_from_slice(data.get(..room.min(data.len())).unwrap_or_default());
        }
    }
    let incoming = Incoming {
        method: parts.method.as_str().to_owned(),
        path: parts.uri.path().to_owned(),
        query: parts.uri.query().unwrap_or_default().to_owned(),
        headers: parts
            .headers
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_owned(),
                    String::from_utf8_lossy(v.as_bytes()).into_owned(),
                )
            })
            .collect(),
        body: bytes,
        size,
    };
    build(handler(incoming).await)
}

/// Converts an [`Outgoing`]; invalid status codes become 500, invalid
/// header names or values are skipped.
fn build(out: Outgoing) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(out.body)));
    *response.status_mut() =
        StatusCode::from_u16(out.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let headers = response.headers_mut();
    for (k, v) in out.headers {
        if let (Ok(name), Ok(value)) = (
            hyper::header::HeaderName::from_bytes(k.trim().as_bytes()),
            hyper::header::HeaderValue::from_str(v.trim()),
        ) {
            headers.append(name, value);
        }
    }
    response
}
