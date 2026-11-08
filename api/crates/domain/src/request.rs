//! The editable request model.

use crate::auth::Auth;
use crate::body::RequestBody;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl HttpMethod {
    pub const ALL: [Self; 7] = [
        Self::Get,
        Self::Post,
        Self::Put,
        Self::Patch,
        Self::Delete,
        Self::Head,
        Self::Options,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }

    /// Case-insensitive parse; `None` for unsupported methods.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|m| m.as_str().eq_ignore_ascii_case(s.trim()))
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row of a params / headers / urlencoded table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
    pub enabled: bool,
    pub description: String,
}

impl Default for KeyValue {
    fn default() -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            enabled: true,
            description: String::new(),
        }
    }
}

impl KeyValue {
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            ..Self::default()
        }
    }

    /// Enabled and with a non-empty key.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.enabled && !self.key.trim().is_empty()
    }
}

/// Per-request transport settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RequestSettings {
    /// 0 means no timeout.
    pub timeout_ms: u64,
    pub follow_redirects: bool,
    pub verify_tls: bool,
}

impl Default for RequestSettings {
    fn default() -> Self {
        Self {
            timeout_ms: 30_000,
            follow_redirects: true,
            verify_tls: true,
        }
    }
}

/// A request as authored. `url` is the source of truth for the query string;
/// `params` mirrors it (see [`crate::urlparams`]) and additionally keeps
/// disabled rows, which are not in the URL.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub params: Vec<KeyValue>,
    pub headers: Vec<KeyValue>,
    pub body: RequestBody,
    pub auth: Auth,
    pub settings: RequestSettings,
}

impl HttpRequest {
    #[must_use]
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            ..Self::default()
        }
    }

    /// Short label for tabs and history, e.g. `GET example.com/users`.
    #[must_use]
    pub fn label(&self) -> String {
        let url = self.url.trim();
        let url = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(url);
        if url.is_empty() {
            "Untitled request".to_owned()
        } else {
            url.to_owned()
        }
    }
}
