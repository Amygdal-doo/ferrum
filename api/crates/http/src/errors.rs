//! Maps `reqwest` errors onto [`ExecError`] without leaking URLs.

use ferrumapi_domain::ExecError;
use std::error::Error as _;

pub(crate) fn map_reqwest(err: reqwest::Error, timeout_ms: u64) -> ExecError {
    let err = err.without_url();
    if err.is_timeout() {
        return ExecError::Timeout(timeout_ms);
    }
    if err.is_redirect() {
        return ExecError::Redirect;
    }
    let msg = describe(&err);
    let lower = msg.to_ascii_lowercase();
    if lower.contains("certificate") || lower.contains("tls") || lower.contains("ssl") {
        ExecError::Tls(msg)
    } else if err.is_connect() {
        ExecError::Connect(msg)
    } else {
        ExecError::Other(msg)
    }
}

/// The error and its source chain joined with `": "`, skipping repeats.
fn describe(err: &reqwest::Error) -> String {
    let mut parts: Vec<String> = vec![err.to_string()];
    let mut source = err.source();
    while let Some(s) = source {
        let text = s.to_string();
        if !parts.iter().any(|p| p.contains(&text)) {
            parts.push(text);
        }
        source = s.source();
    }
    parts.join(": ")
}

/// Short variant label for logs (never the message, which may contain paths).
pub(crate) const fn kind(err: &ExecError) -> &'static str {
    match err {
        ExecError::InvalidUrl(_) => "invalid_url",
        ExecError::InvalidRequest(_) => "invalid_request",
        ExecError::File(_) => "file",
        ExecError::Cancelled => "cancelled",
        ExecError::Timeout(_) => "timeout",
        ExecError::Connect(_) => "connect",
        ExecError::Tls(_) => "tls",
        ExecError::Redirect => "redirect",
        ExecError::Other(_) => "other",
    }
}
