//! A received response. Held in memory only; history keeps a summary.

use serde::{Deserialize, Serialize};

/// A cookie parsed from a `Set-Cookie` response header.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ResponseCookie {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
    /// Raw `Expires` attribute (or `Max-Age=N` rendered as text).
    pub expires: Option<String>,
    pub http_only: bool,
    pub secure: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpResponse {
    pub status: u16,
    /// Canonical reason phrase, e.g. "OK". May be empty.
    pub status_text: String,
    /// e.g. "HTTP/1.1", "HTTP/2.0".
    pub http_version: String,
    /// URL after redirects.
    pub final_url: String,
    /// In wire order; names may repeat.
    pub headers: Vec<(String, String)>,
    pub cookies: Vec<ResponseCookie>,
    pub body: Vec<u8>,
    /// Time from send to full body received.
    pub time_ms: u64,
    /// Body size in bytes (decoded).
    pub size_bytes: u64,
}

impl HttpResponse {
    /// First header value with this name, case-insensitive.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.header("content-type")
    }

    /// Body as text (lossy UTF-8).
    #[must_use]
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    #[must_use]
    pub fn is_json(&self) -> bool {
        self.content_type().is_some_and(|c| c.contains("json"))
    }

    #[must_use]
    pub fn is_html(&self) -> bool {
        self.content_type().is_some_and(|c| c.contains("html"))
    }
}
