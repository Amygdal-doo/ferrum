//! WebSocket client (tokio-tungstenite + rustls). [`connect`] performs the
//! handshake and spawns a task that pumps frames both ways; dropping the
//! returned [`WsClient`] closes the connection.
//!
//! Nothing here logs URLs, headers or payloads.

use crate::auth::apply_auth;
use crate::build::user_headers;
use crate::substitute::substitute;
use base64::Engine as _;
use ferrumapi_domain::protocol::WsPayloadKind;
use ferrumapi_domain::protocol::websocket::validate_payload;
use ferrumapi_domain::{ExecError, HttpRequest, VariableSet};
use futures::{SinkExt as _, StreamExt as _};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{Bytes, Error as WsError, Message};
use tokio_tungstenite::{Connector, connect_async_tls_with_config};
use url::Url;

/// What the server answered to the handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WsConnected {
    pub status: u16,
    /// The subprotocol the server picked.
    pub protocol: Option<String>,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsOutgoing {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsIncoming {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    /// The connection ended (by either side). Always the last event.
    Closed {
        code: Option<u16>,
        reason: String,
    },
    /// A transport error; `Closed` follows.
    Error(String),
}

/// Sends frames; dropping it closes the connection with code 1000.
#[derive(Debug, Clone)]
pub struct WsClient {
    tx: mpsc::UnboundedSender<WsOutgoing>,
}

impl WsClient {
    /// `false` when the connection is already gone.
    pub fn send(&self, message: WsOutgoing) -> bool {
        self.tx.send(message).is_ok()
    }
}

/// A frame from the composer: JSON must parse, binary is base64.
pub fn outgoing(kind: WsPayloadKind, payload: &str) -> Result<WsOutgoing, String> {
    validate_payload(kind, payload)?;
    Ok(match kind {
        WsPayloadKind::Text | WsPayloadKind::Json => WsOutgoing::Text(payload.to_owned()),
        WsPayloadKind::Binary => {
            let compact: String = payload.split_whitespace().collect();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(compact)
                .map_err(|e| format!("not valid base64: {e}"))?;
            WsOutgoing::Binary(bytes)
        }
    })
}

/// `ws`/`wss` URL from what the user typed (`http(s)` is mapped, no scheme is `ws`).
pub fn parse_ws_url(raw: &str) -> Result<Url, ExecError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(ExecError::InvalidUrl("URL is empty".into()));
    }
    let with_scheme = if let Some(rest) = raw.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = raw.strip_prefix("http://") {
        format!("ws://{rest}")
    } else if raw.contains("://") {
        raw.to_owned()
    } else {
        format!("ws://{raw}")
    };
    let url = Url::parse(&with_scheme).map_err(|e| ExecError::InvalidUrl(e.to_string()))?;
    if !matches!(url.scheme(), "ws" | "wss") {
        return Err(ExecError::InvalidUrl(format!(
            "unsupported scheme {:?} (use ws:// or wss://)",
            url.scheme()
        )));
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(ExecError::InvalidUrl("missing host".into()));
    }
    Ok(url)
}

/// Substitutes `vars`, applies headers, auth and subprotocols, and
/// connects. `request.auth` must already be effective.
pub async fn connect(
    request: &HttpRequest,
    vars: &VariableSet,
) -> Result<(WsConnected, WsClient, mpsc::UnboundedReceiver<WsIncoming>), ExecError> {
    let request = substitute(request, vars);
    let mut url = parse_ws_url(&request.url)?;
    let mut headers = user_headers(&request.headers)?;
    apply_auth(&request.auth, &mut headers, &mut url)?;
    let mut handshake = url
        .as_str()
        .into_client_request()
        .map_err(|e| ExecError::InvalidRequest(e.to_string()))?;
    for (name, value) in &headers {
        handshake.headers_mut().append(name.clone(), value.clone());
    }
    let protocols = request.specs.websocket.subprotocol_list();
    if !protocols.is_empty() {
        let value = HeaderValue::from_str(&protocols.join(", "))
            .map_err(|_| ExecError::InvalidRequest("invalid subprotocol".into()))?;
        handshake
            .headers_mut()
            .insert("sec-websocket-protocol", value);
    }
    let connector = if url.scheme() == "wss" {
        Connector::Rustls(crate::tls::client_config(
            request.settings.verify_tls,
            &[b"http/1.1"],
        )?)
    } else {
        Connector::Plain
    };
    let attempt = connect_async_tls_with_config(handshake, None, true, Some(connector));
    let timeout_ms = request.settings.timeout_ms;
    let (stream, response) = if timeout_ms == 0 {
        attempt.await
    } else {
        tokio::time::timeout(Duration::from_millis(timeout_ms), attempt)
            .await
            .map_err(|_| ExecError::Timeout(timeout_ms))?
    }
    .map_err(map_ws_error)?;
    let connected = WsConnected {
        status: response.status().as_u16(),
        protocol: response
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
        headers: response
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    String::from_utf8_lossy(v.as_bytes()).into_owned(),
                )
            })
            .collect(),
    };
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<WsOutgoing>();
    let (in_tx, in_rx) = mpsc::unbounded_channel::<WsIncoming>();
    tokio::spawn(async move {
        let (mut sink, mut source) = stream.split();
        let closed = loop {
            tokio::select! {
                command = out_rx.recv() => {
                    let frame = match command {
                        Some(WsOutgoing::Text(t)) => Message::Text(t.into()),
                        Some(WsOutgoing::Binary(b)) => Message::Binary(Bytes::from(b)),
                        Some(WsOutgoing::Ping(p)) => Message::Ping(Bytes::from(p)),
                        None => {
                            let close = CloseFrame { code: CloseCode::Normal, reason: "".into() };
                            let _ = sink.send(Message::Close(Some(close))).await;
                            break WsIncoming::Closed { code: Some(1000), reason: "closed by FerrumAPI".into() };
                        }
                    };
                    if let Err(e) = sink.send(frame).await {
                        let _ = in_tx.send(WsIncoming::Error(map_ws_error(e).to_string()));
                        break WsIncoming::Closed { code: None, reason: String::new() };
                    }
                }
                frame = source.next() => match frame {
                    Some(Ok(Message::Text(t))) => { let _ = in_tx.send(WsIncoming::Text(t.to_string())); }
                    Some(Ok(Message::Binary(b))) => { let _ = in_tx.send(WsIncoming::Binary(b.to_vec())); }
                    Some(Ok(Message::Ping(p))) => { let _ = in_tx.send(WsIncoming::Ping(p.to_vec())); }
                    Some(Ok(Message::Pong(p))) => { let _ = in_tx.send(WsIncoming::Pong(p.to_vec())); }
                    Some(Ok(Message::Close(frame))) => {
                        break WsIncoming::Closed {
                            code: frame.as_ref().map(|f| u16::from(f.code)),
                            reason: frame.map(|f| f.reason.to_string()).unwrap_or_default(),
                        };
                    }
                    Some(Ok(Message::Frame(_))) => {}
                    Some(Err(e)) => {
                        let _ = in_tx.send(WsIncoming::Error(map_ws_error(e).to_string()));
                        break WsIncoming::Closed { code: None, reason: String::new() };
                    }
                    None => break WsIncoming::Closed { code: None, reason: "connection ended".into() },
                }
            }
        };
        let _ = in_tx.send(closed);
        tracing::debug!("websocket session ended");
    });
    Ok((connected, WsClient { tx: out_tx }, in_rx))
}

fn map_ws_error(e: WsError) -> ExecError {
    match e {
        WsError::Io(io) => ExecError::Connect(io.kind().to_string()),
        WsError::Tls(t) => ExecError::Tls(t.to_string()),
        WsError::Http(resp) => ExecError::Other(format!(
            "handshake rejected with HTTP {}",
            resp.status().as_u16()
        )),
        WsError::Url(u) => ExecError::InvalidUrl(u.to_string()),
        WsError::ConnectionClosed | WsError::AlreadyClosed => {
            ExecError::Other("connection closed".into())
        }
        other => ExecError::Other(other.to_string()),
    }
}
