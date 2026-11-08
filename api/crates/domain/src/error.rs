//! Errors from executing a request. Messages must never contain secrets.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum ExecError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("could not read file: {0}")]
    File(String),
    #[error("request cancelled")]
    Cancelled,
    #[error("request timed out after {0} ms")]
    Timeout(u64),
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("TLS error: {0}")]
    Tls(String),
    #[error("too many redirects")]
    Redirect,
    #[error("request failed: {0}")]
    Other(String),
}
