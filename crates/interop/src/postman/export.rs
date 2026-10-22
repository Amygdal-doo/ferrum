//! Domain tree → Postman Collection v2.1 JSON.

use super::{SCHEMA_V2_1, auth, body, url};
use ferrumapi_domain::{Collection, HttpRequest, Item, RequestSettings, Variable};
use serde_json::{Map, Value, json};

pub(super) fn collection(c: &Collection) -> Value {
    let mut out = Map::new();
    out.insert(
        "info".into(),
        json!({
            "_postman_id": c.id.to_string(),
            "name": c.name,
            "description": c.description,
            "schema": SCHEMA_V2_1,
        }),
    );
    out.insert("item".into(), items(&c.items));
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

fn items(items: &[Item]) -> Value {
    items.iter().map(item).collect()
}

fn item(item: &Item) -> Value {
    let mut out = Map::new();
    match item {
        Item::Folder(f) => {
            out.insert("name".into(), json!(f.name));
            if !f.description.is_empty() {
                out.insert("description".into(), json!(f.description));
            }
            out.insert("item".into(), items(&f.items));
            if let Some(a) = auth::export(&f.auth) {
                out.insert("auth".into(), a);
            }
        }
        Item::Request(r) => {
            out.insert("name".into(), json!(r.name));
            if let Some(ppb) = protocol_profile(&r.request.settings) {
                out.insert("protocolProfileBehavior".into(), ppb);
            }
            out.insert("request".into(), request(&r.request, &r.description));
            out.insert("response".into(), json!([]));
        }
    }
    Value::Object(out)
}

fn request(r: &HttpRequest, description: &str) -> Value {
    let mut out = Map::new();
    if let Some(a) = auth::export(&r.auth) {
        out.insert("auth".into(), a);
    }
    out.insert("method".into(), json!(r.method.as_str()));
    out.insert("header".into(), body::kv_export(&r.headers));
    if let Some(b) = body::export(&r.body) {
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
