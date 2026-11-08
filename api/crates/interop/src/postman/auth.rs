//! Auth mapping. v2.1 stores parameters as `[{key, value, type}]`; v2.0 as a
//! plain object. Absent auth means "inherit from parent".

use super::text;
use ferrumapi_domain::{ApiKeyLocation, Auth};
use serde_json::{Value, json};

/// Imported auth plus an optional note for unsupported auth types.
pub(super) fn import(v: Option<&Value>) -> (Auth, Option<String>) {
    let Some(obj) = v.filter(|v| v.is_object()) else {
        return (Auth::Inherit, None);
    };
    let kind = obj.get("type").and_then(Value::as_str).unwrap_or("");
    let p = |key: &str| param(obj.get(kind), key);
    let auth = match kind {
        "noauth" => Auth::None,
        "basic" => Auth::Basic {
            username: p("username"),
            password: p("password"),
        },
        "bearer" => Auth::Bearer { token: p("token") },
        "apikey" => Auth::ApiKey {
            key: p("key"),
            value: p("value"),
            location: if p("in") == "query" {
                ApiKeyLocation::Query
            } else {
                ApiKeyLocation::Header
            },
        },
        "inherit" | "" => Auth::Inherit,
        other => {
            let note = format!(
                "Imported from Postman: auth type `{other}` is not supported, so this item inherits auth from its parent."
            );
            return (Auth::Inherit, Some(note));
        }
    };
    (auth, None)
}

/// Looks up `key` in the v2.1 array form or the v2.0 object form.
fn param(params: Option<&Value>, key: &str) -> String {
    match params {
        Some(Value::Array(rows)) => rows
            .iter()
            .find(|r| r.get("key").and_then(Value::as_str) == Some(key))
            .map(|r| text(r.get("value")))
            .unwrap_or_default(),
        Some(Value::Object(o)) => text(o.get(key)),
        _ => String::new(),
    }
}

/// `None` for [`Auth::Inherit`] (the field is omitted).
pub(super) fn export(auth: &Auth) -> Option<Value> {
    let rows = |pairs: &[(&str, &str)]| -> Value {
        pairs
            .iter()
            .map(|(k, v)| json!({ "key": k, "value": v, "type": "string" }))
            .collect()
    };
    Some(match auth {
        Auth::Inherit => return None,
        Auth::None => json!({ "type": "noauth" }),
        Auth::Basic { username, password } => json!({
            "type": "basic",
            "basic": rows(&[("username", username), ("password", password)]),
        }),
        Auth::Bearer { token } => json!({ "type": "bearer", "bearer": rows(&[("token", token)]) }),
        Auth::ApiKey {
            key,
            value,
            location,
        } => {
            let place = match location {
                ApiKeyLocation::Header => "header",
                ApiKeyLocation::Query => "query",
            };
            json!({
                "type": "apikey",
                "apikey": rows(&[("key", key), ("value", value), ("in", place)]),
            })
        }
    })
}
