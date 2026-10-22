//! URL mapping. Postman URLs are a string or an object with `raw` and/or the
//! split parts. Our `url` string is the source of truth for enabled params.

use super::{array, description, is_disabled, text};
use ferrumapi_domain::KeyValue;
use ferrumapi_domain::urlparams::{params_from_url, parse_query, url_with_params};
use serde_json::{Map, Value, json};

/// (url, params) satisfying the url/params invariant.
pub(super) fn import(v: Option<&Value>) -> (String, Vec<KeyValue>) {
    let obj = match v {
        Some(Value::String(s)) => return (s.clone(), params_from_url(s, &[])),
        Some(Value::Object(o)) => o,
        _ => return (String::new(), Vec::new()),
    };
    let rows: Vec<KeyValue> = array(obj.get("query"))
        .iter()
        .map(|q| KeyValue {
            key: text(q.get("key")),
            value: text(q.get("value")),
            enabled: !is_disabled(q),
            description: description(q.get("description")),
        })
        .collect();
    let mut url = match obj.get("raw").and_then(Value::as_str) {
        Some(raw) if !raw.trim().is_empty() => raw.to_owned(),
        _ => rebuild(obj),
    };
    if parse_query(&url).is_empty() && rows.iter().any(|r| r.enabled) {
        url = url_with_params(&url, &rows);
    }
    let params = params_from_url(&url, &rows);
    (url, params)
}

/// Joins `protocol://host:port/path#hash` (no query; added by the caller).
fn rebuild(o: &Map<String, Value>) -> String {
    let mut url = String::new();
    let protocol = text(o.get("protocol"));
    if !protocol.is_empty() {
        url.push_str(&protocol);
        url.push_str("://");
    }
    url.push_str(&joined(o.get("host"), "."));
    let port = text(o.get("port"));
    if !port.is_empty() {
        url.push(':');
        url.push_str(&port);
    }
    let path = joined(o.get("path"), "/");
    if !path.is_empty() {
        if !path.starts_with('/') {
            url.push('/');
        }
        url.push_str(&path);
    }
    let hash = text(o.get("hash"));
    if !hash.is_empty() {
        url.push('#');
        url.push_str(&hash);
    }
    url
}

/// A string, or an array of strings / `{ "value": ... }` segments.
fn joined(v: Option<&Value>, sep: &str) -> String {
    match v {
        Some(Value::Array(parts)) => parts
            .iter()
            .map(|p| match p {
                Value::Object(o) => text(o.get("value")),
                other => text(Some(other)),
            })
            .collect::<Vec<_>>()
            .join(sep),
        other => text(other),
    }
}

/// Best-effort split into Postman's URL object; `{{vars}}` are kept intact.
pub(super) fn export(url: &str, params: &[KeyValue]) -> Value {
    let mut out = Map::new();
    out.insert("raw".into(), json!(url));
    let (rest, hash) = url.split_once('#').unwrap_or((url, ""));
    let base = rest.split_once('?').map_or(rest, |(b, _)| b);
    let (protocol, after) = match base.split_once("://") {
        Some((p, a)) => (Some(p), a),
        None => (None, base),
    };
    let (host_port, path) = match after.split_once('/') {
        Some((h, p)) => (h, Some(p)),
        None => (after, None),
    };
    if !host_port.is_empty() && !host_port.contains(char::is_whitespace) {
        if let Some(p) = protocol {
            out.insert("protocol".into(), json!(p));
        }
        let (host, port) = match host_port.rsplit_once(':') {
            Some((h, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => (h, Some(p)),
            _ => (host_port, None),
        };
        let host: Vec<&str> = if host.contains("{{") {
            vec![host]
        } else {
            host.split('.').collect()
        };
        out.insert("host".into(), json!(host));
        if let Some(port) = port {
            out.insert("port".into(), json!(port));
        }
        if let Some(path) = path {
            out.insert("path".into(), json!(path.split('/').collect::<Vec<_>>()));
        }
        if !hash.is_empty() {
            out.insert("hash".into(), json!(hash));
        }
    }
    if !params.is_empty() {
        out.insert("query".into(), params.iter().map(query_row).collect());
    }
    Value::Object(out)
}

fn query_row(p: &KeyValue) -> Value {
    let mut row = json!({ "key": p.key, "value": p.value });
    if !p.enabled {
        row["disabled"] = json!(true);
    }
    if !p.description.is_empty() {
        row["description"] = json!(p.description);
    }
    row
}
