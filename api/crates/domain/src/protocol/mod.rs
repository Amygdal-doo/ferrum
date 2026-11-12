//! Protocol-specific request settings and how HTTP-based kinds become a
//! plain HTTP request ("lowering").
//!
//! Lowering runs after `{{variable}}` substitution, so values can be
//! percent-encoded and JSON-validated safely.

pub mod graphql;
pub mod graphql_schema;
pub mod grpc;
pub mod odata;
pub mod soap;
mod text_fields;
pub mod trpc;
pub mod websocket;

use crate::error::ExecError;
use crate::kind::RequestKind;
use crate::request::{HttpRequest, KeyValue};
use serde::{Deserialize, Serialize};

pub use graphql::GraphqlSpec;
pub use grpc::GrpcSpec;
pub use odata::ODataSpec;
pub use soap::{SoapSpec, SoapVersion};
pub use trpc::{TrpcSpec, TrpcType};
pub use websocket::{WsPayloadKind, WsSpec, WsTemplate};

/// Per-kind settings of a request. Each kind keeps its own part, so
/// switching kinds back and forth loses nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProtocolSpecs {
    #[serde(skip_serializing_if = "is_default")]
    pub graphql: GraphqlSpec,
    #[serde(skip_serializing_if = "is_default")]
    pub soap: SoapSpec,
    #[serde(skip_serializing_if = "is_default")]
    pub odata: ODataSpec,
    #[serde(skip_serializing_if = "is_default")]
    pub trpc: TrpcSpec,
    #[serde(skip_serializing_if = "is_default")]
    pub websocket: WsSpec,
    #[serde(skip_serializing_if = "is_default")]
    pub grpc: GrpcSpec,
}

pub(crate) fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// The plain HTTP request that `request` (already substituted) sends.
/// REST requests are returned unchanged; gRPC and WebSocket requests are not
/// sent over plain HTTP and are an error here.
pub fn lower(request: &HttpRequest) -> Result<HttpRequest, ExecError> {
    let mut out = match request.kind {
        RequestKind::Rest => return Ok(request.clone()),
        RequestKind::Graphql => graphql::lower(request)?,
        RequestKind::Soap => soap::lower(request),
        RequestKind::Odata => odata::lower(request),
        RequestKind::Trpc => trpc::lower(request)?,
        RequestKind::Grpc | RequestKind::WebSocket => {
            return Err(ExecError::InvalidRequest(format!(
                "{} requests are not sent as a single HTTP request",
                request.kind.label()
            )));
        }
    };
    out.kind = RequestKind::Rest;
    out.specs = Box::default();
    Ok(out)
}

/// Percent-encodes a query component: everything except RFC 3986
/// unreserved characters.
#[must_use]
pub fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Appends `key=value` pairs (already encoded) to `url`'s query string,
/// before any `#fragment`.
#[must_use]
pub fn append_query(url: &str, pairs: &[(String, String)]) -> String {
    if pairs.is_empty() {
        return url.to_owned();
    }
    let (base, fragment) = url.split_once('#').map_or((url, ""), |(b, f)| (b, f));
    let joined = pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let sep = if !base.contains('?') {
        "?"
    } else if base.ends_with(['?', '&']) {
        ""
    } else {
        "&"
    };
    let fragment = if fragment.is_empty() {
        String::new()
    } else {
        format!("#{fragment}")
    };
    format!("{base}{sep}{joined}{fragment}")
}

/// Whether an enabled header named `name` is set (case-insensitive).
#[must_use]
pub fn has_header(headers: &[KeyValue], name: &str) -> bool {
    headers
        .iter()
        .any(|h| h.is_active() && h.key.trim().eq_ignore_ascii_case(name))
}

/// Adds `name: value` unless the user already set that header.
pub fn default_header(headers: &mut Vec<KeyValue>, name: &str, value: impl Into<String>) {
    if !has_header(headers, name) {
        headers.push(KeyValue::new(name, value));
    }
}

/// Parses optional JSON text: blank is `None`.
pub(crate) fn optional_json(
    text: &str,
    what: &str,
) -> Result<Option<serde_json::Value>, ExecError> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(text)
        .map(Some)
        .map_err(|e| ExecError::InvalidRequest(format!("{what} is not valid JSON: {e}")))
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
