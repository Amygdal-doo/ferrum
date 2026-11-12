//! UI state of the protocol kinds: fetched descriptions per tab and live
//! WebSocket sessions.

use base64::Engine as _;
use ferrumapi_application::domain::protocol::graphql_schema::GqlSchema;
use ferrumapi_application::domain::protocol::grpc::GrpcMethodInfo;
use ferrumapi_application::domain::protocol::websocket::{WsDirection, WsFrame, WsLog, WsLogEntry};
use ferrumapi_application::http::ws::{WsClient, WsIncoming};
use ferrumapi_application::interop::odata::ODataMetadata;
use ferrumapi_application::interop::wsdl::WsdlOperation;
use ferrumapi_domain::TabId;
use std::collections::HashMap;

/// Something fetched in the background.
#[derive(Debug, Clone, PartialEq)]
pub enum Loadable<T> {
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Loadable<T> {
    #[must_use]
    pub const fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsStatus {
    Connecting,
    Open,
    Closed,
}

/// One tab's WebSocket session (kept after it closes, for the log).
pub struct WsSession {
    pub status: WsStatus,
    pub client: Option<WsClient>,
    pub protocol: Option<String>,
    pub log: WsLog,
    /// Bumped per connect so a stale pump stops writing.
    pub generation: u64,
}

impl WsSession {
    #[must_use]
    pub fn connecting(generation: u64) -> Self {
        Self {
            status: WsStatus::Connecting,
            client: None,
            protocol: None,
            log: WsLog::default(),
            generation,
        }
    }

    /// Logs a frame we sent.
    pub fn sent(&mut self, frame: WsFrame, text: String, size: usize) {
        self.log.push(WsLogEntry {
            at: chrono::Utc::now(),
            direction: WsDirection::Sent,
            frame,
            text,
            size,
        });
    }

    /// Applies an incoming event; returns `true` when the session ended.
    pub fn receive(&mut self, event: WsIncoming) -> bool {
        let (frame, text, size) = match event {
            WsIncoming::Text(t) => {
                let n = t.len();
                (WsFrame::Text, t, n)
            }
            WsIncoming::Binary(b) => (
                WsFrame::Binary,
                base64::engine::general_purpose::STANDARD.encode(&b),
                b.len(),
            ),
            WsIncoming::Ping(p) => (WsFrame::Ping, String::new(), p.len()),
            WsIncoming::Pong(p) => (WsFrame::Pong, String::new(), p.len()),
            WsIncoming::Error(e) => {
                self.log.push(WsLogEntry::event(WsFrame::Error, e));
                return false;
            }
            WsIncoming::Closed { code, reason } => {
                let code = code.map_or_else(|| "no code".to_owned(), |c| c.to_string());
                let text = if reason.is_empty() {
                    format!("Disconnected ({code})")
                } else {
                    format!("Disconnected ({code}): {reason}")
                };
                self.log.push(WsLogEntry::event(WsFrame::Close, text));
                self.status = WsStatus::Closed;
                self.client = None;
                return true;
            }
        };
        self.log.push(WsLogEntry {
            at: chrono::Utc::now(),
            direction: WsDirection::Received,
            frame,
            text,
            size,
        });
        false
    }
}

#[derive(Default)]
pub struct ProtocolState {
    pub graphql: HashMap<TabId, Loadable<GqlSchema>>,
    pub wsdl: HashMap<TabId, Loadable<Vec<WsdlOperation>>>,
    pub odata: HashMap<TabId, Loadable<ODataMetadata>>,
    pub grpc: HashMap<TabId, Loadable<Vec<GrpcMethodInfo>>>,
    pub ws: HashMap<TabId, WsSession>,
    /// Filter of the GraphQL schema panel.
    pub schema_filter: String,
    /// Type expanded in the GraphQL schema panel.
    pub schema_open: Option<String>,
    pub ws_generation: u64,
    /// Selected WebSocket log row per tab (entry time in nanoseconds).
    pub ws_selected: HashMap<TabId, i64>,
}

impl ProtocolState {
    #[must_use]
    pub fn ws_status(&self, tab: TabId) -> Option<WsStatus> {
        self.ws.get(&tab).map(|s| s.status)
    }

    /// Forgets everything about a closed tab (closing its socket).
    pub fn forget(&mut self, tab: TabId) {
        self.graphql.remove(&tab);
        self.wsdl.remove(&tab);
        self.odata.remove(&tab);
        self.grpc.remove(&tab);
        self.ws.remove(&tab);
        self.ws_selected.remove(&tab);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_log_frames_and_close() {
        let mut s = WsSession::connecting(1);
        s.status = WsStatus::Open;
        s.sent(WsFrame::Text, "hi".into(), 2);
        assert!(!s.receive(WsIncoming::Text("hello".into())));
        assert!(!s.receive(WsIncoming::Binary(vec![0, 255])));
        assert!(!s.receive(WsIncoming::Error("boom".into())));
        assert!(s.receive(WsIncoming::Closed {
            code: Some(1000),
            reason: "bye".into()
        }));
        assert_eq!(s.status, WsStatus::Closed);
        assert_eq!((s.log.sent, s.log.received), (1, 2));
        let texts: Vec<_> = s.log.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            ["hi", "hello", "AP8=", "boom", "Disconnected (1000): bye"]
        );
    }
}
