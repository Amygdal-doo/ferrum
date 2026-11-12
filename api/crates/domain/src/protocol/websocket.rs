//! WebSocket request settings and the message log model.

use crate::request::KeyValue;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WsPayloadKind {
    #[default]
    Text,
    /// Text that must parse as JSON (sent as typed, in a text frame).
    Json,
    /// Base64 text, sent as a binary frame.
    Binary,
}

impl WsPayloadKind {
    pub const ALL: [Self; 3] = [Self::Text, Self::Json, Self::Binary];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Json => "JSON",
            Self::Binary => "Binary (base64)",
        }
    }
}

/// A saved message the user can send again.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WsTemplate {
    pub name: String,
    pub kind: WsPayloadKind,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WsSpec {
    /// Comma-separated `Sec-WebSocket-Protocol` values.
    pub subprotocols: String,
    /// The message being composed.
    pub draft: String,
    pub draft_kind: WsPayloadKind,
    pub templates: Vec<WsTemplate>,
}

impl WsSpec {
    /// Non-empty, trimmed subprotocols.
    #[must_use]
    pub fn subprotocol_list(&self) -> Vec<String> {
        self.subprotocols
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    }
}

/// Checks a payload before sending: JSON must parse. Binary is checked by
/// the transport (base64).
pub fn validate_payload(kind: WsPayloadKind, payload: &str) -> Result<(), String> {
    match kind {
        WsPayloadKind::Json => serde_json::from_str::<serde_json::Value>(payload)
            .map(|_| ())
            .map_err(|e| format!("not valid JSON: {e}")),
        WsPayloadKind::Text | WsPayloadKind::Binary => Ok(()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WsDirection {
    Sent,
    Received,
    /// Connection events (connected, closed, errors).
    Event,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WsFrame {
    Text,
    Binary,
    Ping,
    Pong,
    Close,
    Info,
    Error,
}

/// One line of a WebSocket session log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsLogEntry {
    pub at: DateTime<Utc>,
    pub direction: WsDirection,
    pub frame: WsFrame,
    /// Text frames as is; binary as base64; events as a message.
    pub text: String,
    /// Payload size in bytes (0 for events).
    pub size: usize,
}

impl WsLogEntry {
    #[must_use]
    pub fn event(frame: WsFrame, text: impl Into<String>) -> Self {
        Self {
            at: Utc::now(),
            direction: WsDirection::Event,
            frame,
            text: text.into(),
            size: 0,
        }
    }
}

/// Bounded session log: keeps the newest `cap` entries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WsLog {
    pub entries: Vec<WsLogEntry>,
    pub sent: usize,
    pub received: usize,
}

/// Most entries kept per session.
pub const WS_LOG_CAP: usize = 2_000;

impl WsLog {
    pub fn push(&mut self, entry: WsLogEntry) {
        match entry.direction {
            WsDirection::Sent => self.sent += 1,
            WsDirection::Received => self.received += 1,
            WsDirection::Event => {}
        }
        self.entries.push(entry);
        if self.entries.len() > WS_LOG_CAP {
            let extra = self.entries.len() - WS_LOG_CAP;
            self.entries.drain(..extra);
        }
    }
}

/// Handshake headers: enabled request headers (auth is added by the caller).
#[must_use]
pub fn handshake_headers(headers: &[KeyValue]) -> Vec<(String, String)> {
    headers
        .iter()
        .filter(|h| h.is_active())
        .map(|h| (h.key.trim().to_owned(), h.value.trim().to_owned()))
        .collect()
}
