//! Postman export shape, round trips, environments and errors.

mod common;

use common::*;
use ferrumapi_domain::{Auth, Collection, HttpMethod, HttpRequest, Item, SavedRequest};
use ferrumapi_interop::{InteropError, postman};
use serde_json::Value;

#[test]
fn round_trip_export_import_is_lossless() {
    let original = import();
    let json = postman::export_collection(&original).expect("export");
    let again = postman::import_collection(&json).expect("re-import");
    assert_eq!(strip_ids(again), strip_ids(original));
}

#[test]
fn round_trip_keeps_settings() {
    let mut r = HttpRequest::new(HttpMethod::Get, "https://x.dev/a");
    r.settings.follow_redirects = false;
    r.settings.verify_tls = false;
    let mut c = Collection::new("S");
    c.auth = Auth::None;
    c.items.push(Item::Request(SavedRequest::new("r", r)));
    let json = postman::export_collection(&c).expect("export");
    assert!(json.contains("\"strictSSL\": false"));
    let again = postman::import_collection(&json).expect("re-import");
    assert_eq!(strip_ids(again), strip_ids(c));
}

#[test]
fn export_shape_is_postman_v21() {
    let json = postman::export_collection(&import()).expect("export");
    let v: Value = serde_json::from_str(&json).expect("valid json");
    assert_eq!(v["info"]["schema"], postman::SCHEMA_V2_1);
    assert!(v["info"]["_postman_id"].is_string());
    assert_eq!(v["auth"]["type"], "apikey");
    assert_eq!(v["auth"]["apikey"][0]["key"], "key");
    let list = &v["item"][0]["item"][0]["request"];
    assert_eq!(list["url"]["host"], serde_json::json!(["{{baseUrl}}"]));
    assert_eq!(list["url"]["path"], serde_json::json!(["products"]));
    assert_eq!(list["url"]["query"][2]["disabled"], true);
    assert!(list.get("auth").is_none(), "inherit is omitted");
    let login = &v["item"][1]["request"]["url"];
    assert_eq!(login["host"], serde_json::json!(["auth", "acme", "com"]));
    assert_eq!(login["port"], "8443");
    assert_eq!(login["protocol"], "https");
    assert_eq!(
        v["item"][0]["item"][1]["item"][2]["request"]["auth"]["type"],
        "noauth"
    );
    assert_eq!(v["variable"][2]["type"], "secret");
}

#[test]
fn rejects_bad_input() {
    assert!(matches!(
        postman::import_collection("{nope"),
        Err(InteropError::Json(_))
    ));
    assert!(matches!(
        postman::import_collection("[1,2]"),
        Err(InteropError::Unsupported(_))
    ));
    assert!(matches!(
        postman::import_collection(r#"{"foo": 1}"#),
        Err(InteropError::Unsupported(_))
    ));
    let v1 = r#"{"id": "x", "name": "old", "order": [], "requests": []}"#;
    assert!(
        matches!(postman::import_collection(v1), Err(InteropError::Unsupported(m)) if m.contains("v1"))
    );
    let other =
        r#"{"info": {"name": "x", "schema": "https://example.com/openapi.json"}, "item": []}"#;
    assert!(matches!(
        postman::import_collection(other),
        Err(InteropError::Unsupported(_))
    ));
    assert!(matches!(
        postman::import_collection(ENV),
        Err(InteropError::Unsupported(_))
    ));
    assert!(matches!(
        postman::import_environment(V21),
        Err(InteropError::Unsupported(_))
    ));
    assert!(matches!(
        postman::import_environment("nope"),
        Err(InteropError::Json(_))
    ));
}

#[test]
fn imports_environment() {
    let env = postman::import_environment(ENV).expect("env");
    assert_eq!(env.name, "Acme Staging");
    let vars: Vec<_> = env
        .variables
        .iter()
        .map(|v| (v.key.as_str(), v.value.as_str(), v.enabled, v.secret))
        .collect();
    assert_eq!(
        vars,
        [
            ("baseUrl", "https://staging.acme.com/v1", true, false),
            ("token", "eyJhbGciOi", true, true),
            ("limit", "50", false, false),
            ("retries", "3", true, false),
        ]
    );
}
