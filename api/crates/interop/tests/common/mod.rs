//! Helpers shared by the integration tests.
#![allow(dead_code)]

use ferrumapi_domain::urlparams::{params_from_url, parse_query};
use ferrumapi_domain::{
    Auth, Collection, CollectionId, FormField, FormFieldKind, HttpMethod, HttpRequest, Item,
    ItemId, KeyValue, RawLanguage, RequestBody, SavedRequest,
};

pub const V21: &str = include_str!("../fixtures/postman_collection_v21.json");
pub const V20: &str = include_str!("../fixtures/postman_collection_v20.json");
pub const ENV: &str = include_str!("../fixtures/postman_environment.json");
pub const CHROME: &str = include_str!("../fixtures/chrome_copy_as_curl.txt");

pub fn import() -> Collection {
    ferrumapi_interop::postman::import_collection(V21).expect("fixture imports")
}

pub fn find<'a>(items: &'a [Item], name: &str) -> Option<&'a Item> {
    items.iter().find_map(|i| match i {
        _ if i.name() == name => Some(i),
        Item::Folder(f) => find(&f.items, name),
        Item::Request(_) => None,
    })
}

pub fn req<'a>(c: &'a Collection, name: &str) -> &'a SavedRequest {
    match find(&c.items, name) {
        Some(Item::Request(r)) => r,
        other => panic!("no request {name}: {other:?}"),
    }
}

pub fn all_requests(items: &[Item], out: &mut Vec<HttpRequest>) {
    for i in items {
        match i {
            Item::Folder(f) => all_requests(&f.items, out),
            Item::Request(r) => out.push(r.request.clone()),
        }
    }
}

pub fn strip_ids(mut c: Collection) -> Collection {
    fn items(list: &mut [Item]) {
        for i in list {
            match i {
                Item::Folder(f) => {
                    f.id = ItemId(uuid::Uuid::nil());
                    items(&mut f.items);
                }
                Item::Request(r) => r.id = ItemId(uuid::Uuid::nil()),
            }
        }
    }
    c.id = CollectionId(uuid::Uuid::nil());
    items(&mut c.items);
    c
}

/// `url` holds exactly the enabled params, in order.
pub fn assert_invariant(r: &HttpRequest) {
    let from_url: Vec<_> = parse_query(&r.url)
        .into_iter()
        .map(|p| (p.key, p.value))
        .collect();
    let enabled: Vec<_> = r
        .params
        .iter()
        .filter(|p| p.enabled)
        .map(|p| (p.key.clone(), p.value.clone()))
        .collect();
    assert_eq!(from_url, enabled, "invariant broken for {}", r.url);
}

pub fn parse(cmd: &str) -> HttpRequest {
    let r = ferrumapi_interop::curl::parse(cmd).unwrap_or_else(|e| panic!("{cmd}: {e}"));
    assert_invariant(&r);
    r
}

pub fn header<'a>(r: &'a HttpRequest, name: &str) -> Option<&'a str> {
    r.headers
        .iter()
        .find(|h| h.key.eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

pub fn raw(r: &HttpRequest) -> (RawLanguage, &str) {
    match &r.body {
        RequestBody::Raw { language, text } => (*language, text.as_str()),
        other => panic!("expected raw body, got {other:?}"),
    }
}

pub fn fields(r: &HttpRequest) -> Vec<(String, String)> {
    match &r.body {
        RequestBody::UrlEncoded { fields } => fields
            .iter()
            .map(|f| (f.key.clone(), f.value.clone()))
            .collect(),
        other => panic!("expected urlencoded body, got {other:?}"),
    }
}

pub fn request(method: HttpMethod, url: &str) -> HttpRequest {
    let mut r = HttpRequest::new(method, url);
    r.params = params_from_url(url, &[]);
    r.auth = Auth::None;
    r
}

pub fn json_post() -> HttpRequest {
    let mut r = request(HttpMethod::Post, "https://api.dev/users?notify=true");
    r.headers = vec![KeyValue::new("Accept", "application/json")];
    let mut off = KeyValue::new("X-Off", "1");
    off.enabled = false;
    r.headers.push(off);
    r.body = RequestBody::Raw {
        language: RawLanguage::Json,
        text: r#"{"name":"O'Brien","admin":false,"tags":["a"],"n":null}"#.into(),
    };
    r.auth = Auth::Bearer {
        token: "tok".into(),
    };
    r
}

pub fn form_put() -> HttpRequest {
    let mut r = request(HttpMethod::Put, "https://api.dev/files");
    r.body = RequestBody::FormData {
        fields: vec![
            FormField {
                key: "title".into(),
                value: "Q3; final".into(),
                ..FormField::default()
            },
            FormField {
                key: "doc".into(),
                value: "/tmp/report.pdf".into(),
                kind: FormFieldKind::File,
                enabled: true,
            },
            FormField {
                key: "skip".into(),
                value: "x".into(),
                enabled: false,
                ..FormField::default()
            },
        ],
    };
    r.auth = Auth::Basic {
        username: "ann".into(),
        password: "p@ss".into(),
    };
    r
}
