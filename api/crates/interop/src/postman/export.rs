//! Domain tree → Postman Collection v2.1 JSON.

use super::{SCHEMA_V2_1, append_note, auth, body, url};
use ferrumapi_domain::{Collection, HttpRequest, Item, RequestKind, RequestSettings, Variable};
use serde_json::{Map, Value, json};

pub(super) fn collection(c: &Collection) -> Value {
    let mut out = Map::new();
    let mut skipped = Vec::new();
    let item = items(&c.items, &mut skipped);
    let mut description = c.description.clone();
    if !skipped.is_empty() {
        append_note(
            &mut description,
            &format!(
                "FerrumAPI: {} gRPC / WebSocket request(s) not exported (Postman Collection \
                 v2.1 has no format for them): {}.",
                skipped.len(),
                skipped.join(", ")
            ),
        );
    }
    out.insert(
        "info".into(),
        json!({
            "_postman_id": c.id.to_string(),
            "name": c.name,
            "description": description,
            "schema": SCHEMA_V2_1,
        }),
    );
    out.insert("item".into(), item);
    if let Some(a) = auth::export(&c.auth) {
        out.insert("auth".into(), a);
    }
    if !c.variables.is_empty() {
        out.insert(
            "variable".into(),
            c.variables.iter().map(variable).collect(),
        );
    }
    Value::Object(out)
}

fn variable(v: &Variable) -> Value {
    let mut row = json!({
        "key": v.key,
        "value": v.value,
        "type": if v.secret { "secret" } else { "string" },
    });
    if !v.enabled {
        row["disabled"] = json!(true);
    }
    row
}

fn items(items: &[Item], skipped: &mut Vec<String>) -> Value {
    items.iter().filter_map(|i| item(i, skipped)).collect()
}

/// `None` for requests Postman cannot represent (named in `skipped`).
fn item(item: &Item, skipped: &mut Vec<String>) -> Option<Value> {
    let mut out = Map::new();
    match item {
        Item::Folder(f) => {
            out.insert("name".into(), json!(f.name));
            if !f.description.is_empty() {
                out.insert("description".into(), json!(f.description));
            }
            out.insert("item".into(), items(&f.items, skipped));
            if let Some(a) = auth::export(&f.auth) {
                out.insert("auth".into(), a);
            }
        }
        Item::Request(r) => {
            let (request, note) = match r.request.kind {
                RequestKind::Grpc | RequestKind::WebSocket => {
                    skipped.push(r.name.clone());
                    return None;
                }
                RequestKind::Rest | RequestKind::Graphql => (r.request.clone(), None),
                kind => (
                    super::lower_keeping_variables(&r.request),
                    Some(format!(
                        "FerrumAPI {} request, exported as the plain HTTP request it sends.",
                        kind.label()
                    )),
                ),
            };
            let mut description = r.description.clone();
            if let Some(note) = note {
                append_note(&mut description, &note);
            }
            out.insert("name".into(), json!(r.name));
            if let Some(ppb) = protocol_profile(&r.request.settings) {
                out.insert("protocolProfileBehavior".into(), ppb);
            }
            out.insert("request".into(), self::request(&request, &description));
            out.insert("response".into(), json!([]));
        }
    }
    Some(Value::Object(out))
}

fn request(r: &HttpRequest, description: &str) -> Value {
    let mut out = Map::new();
    if let Some(a) = auth::export(&r.auth) {
        out.insert("auth".into(), a);
    }
    let graphql = r.kind == RequestKind::Graphql;
    let method = if graphql { "POST" } else { r.method.as_str() };
    out.insert("method".into(), json!(method));
    out.insert("header".into(), body::kv_export(&r.headers));
    let exported = if graphql {
        Some(body::export_graphql(&r.specs.graphql))
    } else {
        body::export(&r.body)
    };
    if let Some(b) = exported {
        out.insert("body".into(), b);
    }
    out.insert("url".into(), url::export(&r.url, &r.params));
    if !description.is_empty() {
        out.insert("description".into(), json!(description));
    }
    Value::Object(out)
}

/// Only written when a setting differs from the default.
fn protocol_profile(s: &RequestSettings) -> Option<Value> {
    let d = RequestSettings::default();
    let mut out = Map::new();
    if s.follow_redirects != d.follow_redirects {
        out.insert("followRedirects".into(), json!(s.follow_redirects));
    }
    if s.verify_tls != d.verify_tls {
        out.insert("strictSSL".into(), json!(s.verify_tls));
    }
    (!out.is_empty()).then_some(Value::Object(out))
}
