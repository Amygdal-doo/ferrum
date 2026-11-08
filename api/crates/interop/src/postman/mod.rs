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

/// Appends a human-readable import note to a description.
fn append_note(desc: &mut String, note: &str) {
    if !desc.is_empty() {
        desc.push_str("\n\n");
    }
    desc.push_str(note);
}
