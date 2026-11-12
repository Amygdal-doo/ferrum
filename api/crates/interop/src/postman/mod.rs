//! Postman Collection v2.1 (v2.0 accepted where compatible) and Postman
//! environment files.
//!
//! Import works on loosely typed [`serde_json::Value`]s because Postman files
//! use many "string or object" / "string or array" shapes.

mod auth;
mod body;
mod environment;
mod export;
mod import;
mod url;

use crate::InteropError;
use ferrumapi_domain::{Collection, Environment};
use serde_json::Value;

/// Schema URL written on export.
pub const SCHEMA_V2_1: &str =
    "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

/// Postman Collection v2.1 (v2.0 accepted where compatible) → Collection.
/// Every collection, folder and request gets a fresh id.
pub fn import_collection(json: &str) -> Result<Collection, InteropError> {
    import::collection(&parse_json(json)?)
}

/// Collection → pretty-printed Postman Collection v2.1 JSON.
pub fn export_collection(c: &Collection) -> Result<String, InteropError> {
    serde_json::to_string_pretty(&export::collection(c))
        .map_err(|e| InteropError::Json(e.to_string()))
}

/// Postman environment export (`{"name", "values": [...]}`) → Environment.
pub fn import_environment(json: &str) -> Result<Environment, InteropError> {
    environment::import(&parse_json(json)?)
}

fn parse_json(json: &str) -> Result<Value, InteropError> {
    serde_json::from_str(json).map_err(|e| InteropError::Json(e.to_string()))
}

/// A scalar as text: strings verbatim, numbers/bools printed, null/absent → "".
fn text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// `description` may be a string or `{ "content": "...", "type": "text/markdown" }`.
fn description(v: Option<&Value>) -> String {
    match v {
        Some(Value::Object(o)) => text(o.get("content")),
        other => text(other),
    }
}

fn is_disabled(v: &Value) -> bool {
    v.get("disabled").and_then(Value::as_bool).unwrap_or(false)
}

fn array(v: Option<&Value>) -> &[Value] {
    v.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// [`ferrumapi_domain::protocol::lower`] without breaking `{{variables}}`:
/// they are swapped for digit tokens (which survive percent-encoding and
/// are valid both inside JSON strings and as bare JSON values) and put
/// back afterwards. Falls back to the request as authored.
fn lower_keeping_variables(r: &ferrumapi_domain::HttpRequest) -> ferrumapi_domain::HttpRequest {
    let lowered = (|| {
        let mut text = serde_json::to_string(r).ok()?;
        let mut vars: Vec<String> = Vec::new();
        while let Some(start) = text.find("{{") {
            let end = text[start..].find("}}").map(|e| start + e + 2)?;
            let var = text[start..end].to_owned();
            let token = format!("9876543210{:04}", vars.len());
            text = text.replace(&var, &token);
            vars.push(var);
        }
        let parsed: ferrumapi_domain::HttpRequest = serde_json::from_str(&text).ok()?;
        let mut out =
            serde_json::to_string(&ferrumapi_domain::protocol::lower(&parsed).ok()?).ok()?;
        for (i, var) in vars.iter().enumerate().rev() {
            out = out.replace(&format!("9876543210{i:04}"), var);
        }
        serde_json::from_str(&out).ok()
    })();
    lowered.unwrap_or_else(|| r.clone())
}

/// Appends a human-readable import note to a description.
fn append_note(desc: &mut String, note: &str) {
    if !desc.is_empty() {
        desc.push_str("\n\n");
    }
    desc.push_str(note);
}
