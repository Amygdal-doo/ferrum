//! Postman collection JSON → domain tree.

use super::{append_note, array, auth, body, description, text, url};
use crate::InteropError;
use ferrumapi_domain::{
    Collection, Folder, HttpMethod, HttpRequest, Item, KeyValue, RequestBody, RequestKind,
    RequestSettings, SavedRequest, Variable,
};
use serde_json::Value;

pub(super) fn collection(v: &Value) -> Result<Collection, InteropError> {
    let info = v
        .get("info")
        .filter(|i| i.is_object())
        .ok_or_else(|| unsupported(v))?;
    let schema = text(info.get("schema"));
    if !(schema.contains("/collection/v2.1") || schema.contains("/collection/v2.0")) {
        return Err(InteropError::Unsupported(format!(
            "only Postman Collection v2.0/v2.1 is supported (schema: {})",
            if schema.is_empty() {
                "missing"
            } else {
                &schema
            }
        )));
    }
    let mut c = Collection::new(text(info.get("name")));
    c.description = description(info.get("description"));
    let (auth, note) = auth::import(v.get("auth"));
    c.auth = auth;
    if let Some(note) = note {
        append_note(&mut c.description, &note);
    }
    c.variables = array(v.get("variable"))
        .iter()
        .filter(|r| r.is_object())
        .map(|r| Variable {
            key: text(r.get("key").or_else(|| r.get("id"))),
            value: text(r.get("value")),
            enabled: !super::is_disabled(r),
            secret: r.get("type").and_then(Value::as_str) == Some("secret"),
        })
        .collect();
    c.items = items(v.get("item"));
    Ok(c)
}

fn unsupported(v: &Value) -> InteropError {
    let msg = if v.get("requests").is_some() && v.get("order").is_some() {
        "Postman Collection v1 is not supported; re-export it as v2.1"
    } else if v.get("values").is_some() {
        "this looks like a Postman environment, not a collection"
    } else {
        "not a Postman collection (missing `info`)"
    };
    InteropError::Unsupported(msg.into())
}

fn items(v: Option<&Value>) -> Vec<Item> {
    array(v).iter().filter_map(item).collect()
}

fn item(v: &Value) -> Option<Item> {
    let name = text(v.get("name"));
    let mut desc = description(v.get("description"));
    if v.get("item").is_some_and(Value::is_array) {
        let (auth, note) = auth::import(v.get("auth"));
        if let Some(note) = note {
            append_note(&mut desc, &note);
        }
        return Some(Item::Folder(Folder {
            name,
            description: desc,
            auth,
            items: items(v.get("item")),
            ..Folder::default()
        }));
    }
    let req = v.get("request")?;
    // v2.0/2.1: the request description usually lives on `request`.
    if desc.is_empty() {
        desc = description(req.get("description"));
    }
    let mut request = request(req, &mut desc);
    request.settings = settings(v);
    let mut saved = SavedRequest::new(name, request);
    saved.description = desc;
    Some(Item::Request(saved))
}

/// `notes` receives human-readable notes about anything that was dropped.
fn request(v: &Value, notes: &mut String) -> HttpRequest {
    if let Value::String(url) = v {
        let (url, params) = url::import(Some(&Value::String(url.clone())));
        return HttpRequest {
            url,
            params,
            ..HttpRequest::default()
        };
    }
    let method_text = text(v.get("method"));
    let method = if method_text.is_empty() {
        HttpMethod::Get
    } else {
        HttpMethod::parse(&method_text).unwrap_or_else(|| {
            append_note(
                notes,
                &format!(
                    "Imported from Postman: method `{method_text}` is not supported; using GET."
                ),
            );
            HttpMethod::Get
        })
    };
    let (url, params) = url::import(v.get("url"));
    let headers = headers(v.get("header"));
    let graphql = body::graphql_spec(v.get("body"));
    let body = if graphql.is_some() {
        RequestBody::None
    } else {
        body::import(v.get("body"), &headers)
    };
    let (auth, note) = auth::import(v.get("auth"));
    if let Some(note) = note {
        append_note(notes, &note);
    }
    let mut request = HttpRequest {
        method,
        url,
        params,
        body,
        headers,
        auth,
        settings: RequestSettings::default(),
        ..HttpRequest::default()
    };
    if let Some(spec) = graphql {
        request.kind = RequestKind::Graphql;
        request.specs.graphql = spec;
    }
    request
}

/// Header rows, or the v2.0 `"Key: value\n..."` string form.
fn headers(v: Option<&Value>) -> Vec<KeyValue> {
    match v {
        Some(Value::String(s)) => s
            .lines()
            .filter_map(|l| l.split_once(':'))
            .map(|(k, val)| KeyValue::new(k.trim(), val.trim()))
            .collect(),
        other => body::kv_rows(other),
    }
}

/// Item-level `protocolProfileBehavior.{followRedirects, strictSSL}`.
fn settings(v: &Value) -> RequestSettings {
    let mut s = RequestSettings::default();
    if let Some(ppb) = v.get("protocolProfileBehavior") {
        if let Some(f) = ppb.get("followRedirects").and_then(Value::as_bool) {
            s.follow_redirects = f;
        }
        if let Some(strict) = ppb.get("strictSSL").and_then(Value::as_bool) {
            s.verify_tls = strict;
        }
    }
    s
}
