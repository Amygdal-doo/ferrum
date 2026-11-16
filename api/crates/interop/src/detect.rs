//! One entry point for imported text: a cURL command, a Postman
//! collection, an Insomnia export or an OpenAPI 3 document (JSON or YAML),
//! recognised by its content.

use crate::{InteropError, curl, insomnia, openapi, postman};
use ferrumapi_domain::{Collection, HttpRequest};
use serde_json::{Map, Number, Value};

/// What an imported text turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Imported {
    /// A Postman / Insomnia / OpenAPI collection, and which format it was.
    Collection(Collection, &'static str),
    /// A pasted cURL command.
    Request(HttpRequest),
}

/// Imports `text` in whichever supported format it is in.
pub fn import_text(text: &str) -> Result<Imported, InteropError> {
    let trimmed = text.trim_start();
    if trimmed.starts_with("curl ") || trimmed.starts_with("curl\t") {
        return curl::parse(trimmed).map(Imported::Request);
    }
    let doc = parse_document(text)?;
    if insomnia::detect(&doc) {
        return insomnia::import(&doc).map(|c| Imported::Collection(c, "Insomnia"));
    }
    if doc.get("openapi").is_some() || doc.get("swagger").is_some() {
        return openapi::import(&doc).map(|c| Imported::Collection(c, "OpenAPI"));
    }
    if doc.get("info").is_some() && doc.get("item").is_some() {
        let json = serde_json::to_string(&doc).map_err(|e| InteropError::Json(e.to_string()))?;
        return postman::import_collection(&json).map(|c| Imported::Collection(c, "Postman"));
    }
    Err(InteropError::Unsupported(
        "expected a Postman collection, an Insomnia export, an OpenAPI 3 document or a cURL command"
            .into(),
    ))
}

/// Imports `text` as a collection (a cURL command is not one).
pub fn import_collection(text: &str) -> Result<(Collection, &'static str), InteropError> {
    match import_text(text)? {
        Imported::Collection(c, format) => Ok((c, format)),
        Imported::Request(_) => Err(InteropError::Unsupported(
            "a cURL command is a single request, not a collection".into(),
        )),
    }
}

/// JSON, else YAML (map keys become strings).
pub fn parse_document(text: &str) -> Result<Value, InteropError> {
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        return Ok(v);
    }
    let yaml: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(text).map_err(|e| InteropError::Parse(e.to_string()))?;
    let v = to_json(yaml);
    if v.is_object() {
        Ok(v)
    } else {
        Err(InteropError::Parse(
            "expected a JSON or YAML document".into(),
        ))
    }
}

fn to_json(v: serde_yaml_ng::Value) -> Value {
    use serde_yaml_ng::Value as Y;
    match v {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(b),
        Y::Number(n) => n
            .as_i64()
            .map(Value::from)
            .or_else(|| n.as_u64().map(Value::from))
            .or_else(|| n.as_f64().and_then(Number::from_f64).map(Value::Number))
            .unwrap_or(Value::Null),
        Y::String(s) => Value::String(s),
        Y::Sequence(items) => Value::Array(items.into_iter().map(to_json).collect()),
        Y::Mapping(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let key = match to_json(k) {
                        Value::String(s) => s,
                        other => other.to_string(),
                    };
                    (key, to_json(v))
                })
                .collect::<Map<_, _>>(),
        ),
        Y::Tagged(t) => to_json(t.value),
    }
}
