//! tRPC over HTTP (v10/v11 wire format without a transformer):
//! queries are `GET /proc?input=<json>`, mutations `POST /proc` with a JSON
//! body; batching wraps the input as `{"0": input}` with `?batch=1`.

use super::{append_query, encode_component, optional_json};
use crate::body::{RawLanguage, RequestBody};
use crate::error::ExecError;
use crate::request::{HttpMethod, HttpRequest};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrpcType {
    #[default]
    Query,
    Mutation,
}

impl TrpcType {
    pub const ALL: [Self; 2] = [Self::Query, Self::Mutation];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Query => "Query",
            Self::Mutation => "Mutation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TrpcSpec {
    /// e.g. `user.byId`.
    pub procedure: String,
    pub procedure_type: TrpcType,
    /// JSON input; blank sends no input.
    pub input: String,
    pub batch: bool,
}

pub(super) fn lower(request: &HttpRequest) -> Result<HttpRequest, ExecError> {
    let spec = &request.specs.trpc;
    let procedure = spec.procedure.trim().trim_matches('/');
    if procedure.is_empty() {
        return Err(ExecError::InvalidRequest(
            "enter the tRPC procedure, e.g. user.byId".into(),
        ));
    }
    let input = optional_json(&spec.input, "tRPC input")?;
    let (base, query) = request
        .url
        .split_once('?')
        .map_or((request.url.as_str(), None), |(b, q)| (b, Some(q)));
    let mut url = format!("{}/{procedure}", base.trim_end_matches('/'));
    if let Some(q) = query.filter(|q| !q.is_empty()) {
        url = format!("{url}?{q}");
    }
    let mut pairs = Vec::new();
    if spec.batch {
        pairs.push(("batch".to_owned(), "1".to_owned()));
    }
    let wrapped = if spec.batch {
        Some(json!({ "0": input.clone().unwrap_or_else(|| json!({})) }))
    } else {
        input
    };
    let mut out = request.clone();
    match spec.procedure_type {
        TrpcType::Query => {
            if let Some(v) = &wrapped {
                pairs.push(("input".into(), encode_component(&v.to_string())));
            }
            out.method = HttpMethod::Get;
            out.body = RequestBody::None;
        }
        TrpcType::Mutation => {
            out.method = HttpMethod::Post;
            out.body = wrapped.map_or(RequestBody::None, |v| RequestBody::Raw {
                language: RawLanguage::Json,
                text: v.to_string(),
            });
        }
    }
    out.url = append_query(&url, &pairs);
    Ok(out)
}

/// What a tRPC response carried.
#[derive(Debug, Clone, PartialEq)]
pub enum TrpcOutcome {
    /// `result.data` (`.json` unwrapped when a superjson-style envelope).
    Data(Value),
    /// The `error` object (`.json` unwrapped likewise).
    Error(Value),
}

fn unwrap_json(v: &Value) -> Value {
    match v {
        Value::Object(o)
            if o.contains_key("json") && o.keys().all(|k| k == "json" || k == "meta") =>
        {
            o.get("json").cloned().unwrap_or(Value::Null)
        }
        other => other.clone(),
    }
}

/// Unwraps a tRPC response body (batched: the first call). `None` when the
/// body is not JSON in the tRPC shape.
#[must_use]
pub fn unwrap_response(body: &[u8]) -> Option<TrpcOutcome> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let first = match &value {
        Value::Array(items) => items.first()?.clone(),
        other => other.clone(),
    };
    if let Some(error) = first.get("error") {
        return Some(TrpcOutcome::Error(unwrap_json(error)));
    }
    let result = first.get("result")?;
    Some(TrpcOutcome::Data(
        result.get("data").map(unwrap_json).unwrap_or(Value::Null),
    ))
}
