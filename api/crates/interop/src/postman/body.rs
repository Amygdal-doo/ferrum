//! Body mapping plus the shared key/value row helpers (headers, urlencoded).

use super::{array, description, is_disabled, text};
use ferrumapi_domain::protocol::GraphqlSpec;
use ferrumapi_domain::{FormField, FormFieldKind, KeyValue, RawLanguage, RequestBody};
use serde_json::{Value, json};

/// `[{key, value, disabled, description}]` → rows.
pub(super) fn kv_rows(v: Option<&Value>) -> Vec<KeyValue> {
    array(v)
        .iter()
        .filter(|r| r.is_object())
        .map(|r| KeyValue {
            key: text(r.get("key")),
            value: text(r.get("value")),
            enabled: !is_disabled(r),
            description: description(r.get("description")),
        })
        .collect()
}

pub(super) fn kv_export(rows: &[KeyValue]) -> Value {
    rows.iter()
        .map(|r| {
            let mut row = json!({ "key": r.key, "value": r.value, "type": "text" });
            if !r.enabled {
                row["disabled"] = json!(true);
            }
            if !r.description.is_empty() {
                row["description"] = json!(r.description);
            }
            row
        })
        .collect()
}

/// `headers` are used to guess the raw language when `options` is absent.
pub(super) fn import(v: Option<&Value>, headers: &[KeyValue]) -> RequestBody {
    let Some(body) = v.filter(|b| b.is_object()) else {
        return RequestBody::None;
    };
    match body.get("mode").and_then(Value::as_str).unwrap_or("") {
        "raw" => RequestBody::Raw {
            language: raw_language(body, headers),
            text: text(body.get("raw")),
        },
        "urlencoded" => RequestBody::UrlEncoded {
            fields: kv_rows(body.get("urlencoded")),
        },
        "formdata" => RequestBody::FormData {
            fields: array(body.get("formdata")).iter().map(form_field).collect(),
        },
        "file" => {
            let path = src(body.get("file").and_then(|f| f.get("src")));
            RequestBody::Binary {
                path: (!path.is_empty()).then_some(path),
            }
        }
        "graphql" => graphql(body.get("graphql")),
        _ => RequestBody::None,
    }
}

fn raw_language(body: &Value, headers: &[KeyValue]) -> RawLanguage {
    let declared = body
        .pointer("/options/raw/language")
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase);
    let hint = match declared {
        Some(lang) => lang,
        None => headers
            .iter()
            .find(|h| h.enabled && h.key.eq_ignore_ascii_case("content-type"))
            .map(|h| h.value.to_ascii_lowercase())
            .unwrap_or_default(),
    };
    if hint.contains("json") {
        RawLanguage::Json
    } else if hint.contains("xml") {
        RawLanguage::Xml
    } else if hint.contains("html") {
        RawLanguage::Html
    } else {
        RawLanguage::Text
    }
}

fn form_field(r: &Value) -> FormField {
    let is_file = r.get("type").and_then(Value::as_str) == Some("file");
    FormField {
        key: text(r.get("key")),
        value: if is_file {
            src(r.get("src"))
        } else {
            text(r.get("value"))
        },
        kind: if is_file {
            FormFieldKind::File
        } else {
            FormFieldKind::Text
        },
        enabled: !is_disabled(r),
    }
}

/// `src` is a string, an array of strings (first wins), or null.
fn src(v: Option<&Value>) -> String {
    match v {
        Some(Value::Array(items)) => text(items.first()),
        other => text(other),
    }
}

/// A `graphql` body as a GraphQL request spec (Postman stores the
/// variables as text).
pub(super) fn graphql_spec(v: Option<&Value>) -> Option<GraphqlSpec> {
    let body = v.filter(|b| b.get("mode").and_then(Value::as_str) == Some("graphql"))?;
    let g = body.get("graphql");
    let variables = match g.and_then(|g| g.get("variables")) {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Null) | None => String::new(),
        Some(other) => serde_json::to_string_pretty(other).unwrap_or_default(),
    };
    Some(GraphqlSpec {
        query: text(g.and_then(|g| g.get("query"))),
        variables,
        operation_name: String::new(),
        use_get: false,
    })
}

/// GraphQL body → JSON `{"query", "variables"}`. Only reached through
/// [`import`] directly; requests use [`graphql_spec`].
fn graphql(v: Option<&Value>) -> RequestBody {
    let query = text(v.and_then(|g| g.get("query")));
    let variables = match v.and_then(|g| g.get("variables")) {
        Some(Value::String(s)) if s.trim().is_empty() => json!({}),
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or_else(|_| json!(s)),
        Some(Value::Null) | None => json!({}),
        Some(other) => other.clone(),
    };
    let doc = json!({ "query": query, "variables": variables });
    RequestBody::Raw {
        language: RawLanguage::Json,
        text: serde_json::to_string_pretty(&doc).unwrap_or_default(),
    }
}

/// Postman's `graphql` body mode.
pub(super) fn export_graphql(spec: &GraphqlSpec) -> Value {
    json!({
        "mode": "graphql",
        "graphql": { "query": spec.query, "variables": spec.variables },
    })
}

/// `None` for [`RequestBody::None`] (the field is omitted).
pub(super) fn export(body: &RequestBody) -> Option<Value> {
    Some(match body {
        RequestBody::None => return None,
        RequestBody::Raw { language, text } => {
            let lang = match language {
                RawLanguage::Json => "json",
                RawLanguage::Text => "text",
                RawLanguage::Xml => "xml",
                RawLanguage::Html => "html",
            };
            json!({ "mode": "raw", "raw": text, "options": { "raw": { "language": lang } } })
        }
        RequestBody::UrlEncoded { fields } => {
            json!({ "mode": "urlencoded", "urlencoded": kv_export(fields) })
        }
        RequestBody::FormData { fields } => {
            let rows: Vec<Value> = fields
                .iter()
                .map(|f| {
                    let mut row = match f.kind {
                        FormFieldKind::Text => {
                            json!({ "key": f.key, "value": f.value, "type": "text" })
                        }
                        FormFieldKind::File => {
                            json!({ "key": f.key, "src": f.value, "type": "file" })
                        }
                    };
                    if !f.enabled {
                        row["disabled"] = json!(true);
                    }
                    row
                })
                .collect();
            json!({ "mode": "formdata", "formdata": rows })
        }
        RequestBody::Binary { path } => json!({ "mode": "file", "file": { "src": path } }),
    })
}
