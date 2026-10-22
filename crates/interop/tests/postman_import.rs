//! Postman collection import from realistic fixtures.

mod common;

use common::*;
use ferrumapi_domain::{
    ApiKeyLocation, Auth, FormFieldKind, HttpMethod, Item, RawLanguage, RequestBody,
};
use ferrumapi_interop::postman;
use serde_json::Value;

#[test]
fn imports_collection_metadata_auth_and_variables() {
    let c = import();
    assert_eq!(c.name, "Acme Store API");
    assert_eq!(c.description, "Public API for the Acme store.");
    assert_eq!(
        c.auth,
        Auth::ApiKey {
            key: "X-Api-Key".into(),
            value: "{{apiKey}}".into(),
            location: ApiKeyLocation::Header
        }
    );
    let vars: Vec<_> = c
        .variables
        .iter()
        .map(|v| (v.key.as_str(), v.value.as_str(), v.enabled, v.secret))
        .collect();
    assert_eq!(
        vars,
        [
            ("baseUrl", "https://api.acme.com/v1", true, false),
            ("limit", "20", true, false),
            ("apiKey", "k-123", true, true),
            ("legacy", "old", false, false),
        ]
    );
    let names: Vec<_> = c.items.iter().map(Item::name).collect();
    assert_eq!(
        names,
        [
            "Products",
            "Login",
            "Search (GraphQL)",
            "Notes (xml)",
            "Script body",
            "Health"
        ]
    );
}

#[test]
fn imports_nested_folders_with_auth() {
    let c = import();
    let Some(Item::Folder(products)) = find(&c.items, "Products") else {
        panic!("folder")
    };
    assert_eq!(products.description, "Catalogue endpoints");
    assert_eq!(products.auth, Auth::Inherit);
    let Some(Item::Folder(admin)) = find(&c.items, "Admin") else {
        panic!("folder")
    };
    assert_eq!(
        admin.auth,
        Auth::Basic {
            username: "admin".into(),
            password: "s3cret".into()
        }
    );
    assert_eq!(admin.items.len(), 3);
}

#[test]
fn imports_params_headers_and_disabled_rows() {
    let c = import();
    let r = req(&c, "List products");
    assert_eq!(r.description, "Paged product list");
    assert_eq!(r.request.method, HttpMethod::Get);
    assert_eq!(r.request.url, "{{baseUrl}}/products?page=1&limit={{limit}}");
    let params: Vec<_> = r
        .request
        .params
        .iter()
        .map(|p| (p.key.as_str(), p.value.as_str(), p.enabled))
        .collect();
    assert_eq!(
        params,
        [
            ("page", "1", true),
            ("limit", "{{limit}}", true),
            ("sort", "price", false)
        ]
    );
    assert_eq!(r.request.params[0].description, "1-based page");
    assert_eq!(r.request.headers.len(), 2);
    assert!(!r.request.headers[1].enabled);
    assert_eq!(r.request.headers[1].description, "verbose logs");
    assert_eq!(r.request.auth, Auth::Inherit);
}

#[test]
fn imports_every_body_mode() {
    let c = import();
    let create = &req(&c, "Create product").request;
    assert!(
        matches!(&create.body, RequestBody::Raw { language: RawLanguage::Json, text } if text.contains("Anvil"))
    );

    let RequestBody::FormData { fields } = &req(&c, "Upload image").request.body else {
        panic!("formdata")
    };
    assert_eq!(fields.len(), 3);
    assert_eq!(
        (fields[0].kind, fields[0].value.as_str()),
        (FormFieldKind::Text, "Front view")
    );
    assert_eq!(
        (fields[1].kind, fields[1].value.as_str()),
        (FormFieldKind::File, "/tmp/anvil.png")
    );
    assert!(!fields[2].enabled);

    let manual = &req(&c, "Replace manual").request;
    assert_eq!(manual.method, HttpMethod::Patch);
    assert_eq!(
        manual.body,
        RequestBody::Binary {
            path: Some("/tmp/manual.pdf".into())
        }
    );
    assert_eq!(manual.auth, Auth::None);

    let RequestBody::UrlEncoded { fields } = &req(&c, "Login").request.body else {
        panic!("urlencoded")
    };
    assert_eq!(
        (fields[0].key.as_str(), fields[0].value.as_str()),
        ("grant_type", "password")
    );
    assert!(!fields[1].enabled);

    let RequestBody::Raw {
        language: RawLanguage::Json,
        text,
    } = &req(&c, "Search (GraphQL)").request.body
    else {
        panic!("graphql")
    };
    let doc: Value = serde_json::from_str(text).expect("graphql json");
    assert!(doc["query"].as_str().is_some_and(|q| q.contains("search")));
    assert_eq!(doc["variables"]["q"], "anvil");

    assert!(matches!(
        req(&c, "Notes (xml)").request.body,
        RequestBody::Raw {
            language: RawLanguage::Xml,
            ..
        }
    ));
    assert!(matches!(
        req(&c, "Script body").request.body,
        RequestBody::Raw {
            language: RawLanguage::Text,
            ..
        }
    ));
}

#[test]
fn imports_auth_types_and_notes_unsupported() {
    let c = import();
    assert_eq!(
        req(&c, "Login").request.auth,
        Auth::Bearer {
            token: "{{token}}".into()
        }
    );
    assert_eq!(
        req(&c, "Notes (xml)").request.auth,
        Auth::ApiKey {
            key: "api_key".into(),
            value: "abc123".into(),
            location: ApiKeyLocation::Query
        }
    );
    let script = req(&c, "Script body");
    assert_eq!(script.request.auth, Auth::Inherit);
    assert!(
        script.description.contains("oauth2"),
        "{}",
        script.description
    );
}

#[test]
fn imports_url_object_without_raw_and_string_requests() {
    let c = import();
    let login = &req(&c, "Login").request;
    assert_eq!(
        login.url,
        "https://auth.acme.com:8443/oauth/token?client=web"
    );
    assert_eq!(login.params.len(), 1);
    let health = &req(&c, "Health").request;
    assert_eq!(
        (health.method, health.url.as_str()),
        (HttpMethod::Get, "https://status.acme.com/health?verbose")
    );
    assert_eq!(health.params[0].key, "verbose");
    let mut reqs = Vec::new();
    all_requests(&c.items, &mut reqs);
    assert_eq!(reqs.len(), 9);
    reqs.iter().for_each(assert_invariant);
}

#[test]
fn import_generates_fresh_ids() {
    let (a, b) = (import(), import());
    assert_ne!(a.id, b.id);
    assert_ne!(a.items[0].id(), b.items[0].id());
}

#[test]
fn imports_v20_string_headers_and_object_auth() {
    let c = postman::import_collection(V20).expect("v2.0");
    let r = req(&c, "Get user");
    let h: Vec<_> = r
        .request
        .headers
        .iter()
        .map(|h| (h.key.as_str(), h.value.as_str()))
        .collect();
    assert_eq!(h, [("Accept", "application/json"), ("X-Trace", "on")]);
    assert_eq!(
        r.request.auth,
        Auth::Basic {
            username: "alice".into(),
            password: "pw".into()
        }
    );
    assert_invariant(&r.request);
}
