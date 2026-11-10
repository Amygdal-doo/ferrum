//! Webhook inspector model: captured requests, a bounded in-memory log and
//! conversion of a capture into an editable request for replay.

use crate::body::{RawLanguage, RequestBody};
use crate::mock::query_pairs;
use crate::request::{HttpMethod, HttpRequest, KeyValue};
use crate::text::pretty_json;
use crate::urlparams::parse_query;
use chrono::{DateTime, Utc};
use std::collections::VecDeque;

/// Captures kept by default; older ones are dropped.
pub const DEFAULT_CAPTURE_LIMIT: usize = 500;
/// Largest body stored per capture; the rest is cut off.
pub const MAX_CAPTURE_BODY: usize = 1024 * 1024;

/// Headers that describe one hop and must not be replayed verbatim.
const HOP_HEADERS: [&str; 7] = [
    "host",
    "content-length",
    "connection",
    "keep-alive",
    "transfer-encoding",
    "upgrade",
    "te",
];

/// One request received by the inspector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedRequest {
    /// Increasing per inspector session.
    pub id: u64,
    pub received_at: DateTime<Utc>,
    pub method: String,
    pub path: String,
    /// Raw query string without `?`.
    pub query: String,
    /// In wire order; names may repeat.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Size of the body as received (may exceed `body.len()` when truncated).
    pub size: usize,
}

impl CapturedRequest {
    /// First header value with this name, case-insensitive.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    #[must_use]
    pub fn truncated(&self) -> bool {
        self.size > self.body.len()
    }

    /// Path plus `?query` when present.
    #[must_use]
    pub fn target(&self) -> String {
        if self.query.is_empty() {
            self.path.clone()
        } else {
            format!("{}?{}", self.path, self.query)
        }
    }

    /// Decoded query parameters.
    #[must_use]
    pub fn query_pairs(&self) -> Vec<(String, String)> {
        query_pairs(&self.query)
    }

    /// The body for display: pretty JSON when it parses, the text otherwise,
    /// or a placeholder for binary data.
    #[must_use]
    pub fn display_body(&self) -> String {
        if self.body.is_empty() {
            return String::new();
        }
        let Ok(text) = std::str::from_utf8(&self.body) else {
            return format!("<{} bytes of binary data>", self.body.len());
        };
        pretty_json(text).unwrap_or_else(|| text.to_owned())
    }

    /// Raw-body language guessed from `Content-Type` (or the body itself).
    #[must_use]
    pub fn body_language(&self) -> RawLanguage {
        let ct = self
            .header("content-type")
            .unwrap_or_default()
            .to_ascii_lowercase();
        if ct.contains("json") {
            RawLanguage::Json
        } else if ct.contains("xml") {
            RawLanguage::Xml
        } else if ct.contains("html") {
            RawLanguage::Html
        } else if ct.is_empty() && pretty_json(&String::from_utf8_lossy(&self.body)).is_some() {
            RawLanguage::Json
        } else {
            RawLanguage::Text
        }
    }

    /// An editable request that replays this capture against `base_url`
    /// (e.g. `http://127.0.0.1:9000`). Hop-by-hop headers are dropped.
    #[must_use]
    pub fn to_request(&self, base_url: &str) -> HttpRequest {
        let url = format!("{}{}", base_url.trim_end_matches('/'), self.target());
        let headers = self
            .headers
            .iter()
            .filter(|(k, _)| !HOP_HEADERS.iter().any(|h| k.eq_ignore_ascii_case(h)))
            .map(|(k, v)| KeyValue::new(k, v))
            .collect();
        let body = if self.body.is_empty() {
            RequestBody::None
        } else {
            let mut language = self.body_language();
            let mut text = String::from_utf8_lossy(&self.body).into_owned();
            if language == RawLanguage::Json
                && let Some(pretty) = pretty_json(&text)
            {
                text = pretty;
            } else if language == RawLanguage::Json {
                language = RawLanguage::Text;
            }
            RequestBody::Raw { language, text }
        };
        HttpRequest {
            method: HttpMethod::parse(&self.method).unwrap_or(HttpMethod::Post),
            params: parse_query(&url),
            url,
            headers,
            body,
            ..HttpRequest::default()
        }
    }
}

/// Newest-last ring buffer of captures with a fixed capacity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureLog {
    limit: usize,
    next_id: u64,
    items: VecDeque<CapturedRequest>,
}

impl Default for CaptureLog {
    fn default() -> Self {
        Self::new(DEFAULT_CAPTURE_LIMIT)
    }
}

impl CaptureLog {
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            next_id: 1,
            items: VecDeque::new(),
        }
    }

    /// Stores `capture` with a fresh id (dropping the oldest when full) and
    /// returns the id.
    pub fn push(&mut self, mut capture: CapturedRequest) -> u64 {
        capture.id = self.next_id;
        self.next_id += 1;
        while self.items.len() >= self.limit {
            self.items.pop_front();
        }
        let id = capture.id;
        self.items.push_back(capture);
        id
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: u64) -> Option<&CapturedRequest> {
        self.items.iter().find(|c| c.id == id)
    }

    /// Newest first.
    #[must_use]
    pub fn newest_first(&self) -> Vec<CapturedRequest> {
        self.items.iter().rev().cloned().collect()
    }
}

/// What the inspector answers to every request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookReply {
    pub status: u16,
    pub content_type: String,
    pub body: String,
}

impl Default for WebhookReply {
    fn default() -> Self {
        Self {
            status: 200,
            content_type: "application/json".into(),
            body: "{\"ok\":true}".into(),
        }
    }
}

#[cfg(test)]
#[path = "webhook_tests.rs"]
mod tests;
