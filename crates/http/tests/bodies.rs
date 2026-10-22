//! Request building against a local server: query, bodies, headers, variables.

mod common;

use common::{header, ok_server, only_request, req, send, send_vars};
use ferrumapi_domain::{
    FormField, FormFieldKind, HttpMethod, KeyValue, RawLanguage, RequestBody, VariableSet,
};
use std::io::Write as _;

#[tokio::test]
async fn get_with_query_string() {
    let server = ok_server().await;
    let mut r = req(HttpMethod::Get, format!("{}/items?a=1&b=two", server.uri()));
    // Params mirror the URL and must not be appended a second time.
    r.params = vec![KeyValue::new("a", "1"), KeyValue::new("b", "two")];
    let resp = send(&r).await.expect("response");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.status_text, "OK");
    assert_eq!(resp.http_version, "HTTP/1.1");
    assert_eq!(resp.body, b"ok");
    assert_eq!(resp.size_bytes, 2);
    let got = only_request(&server).await;
    assert_eq!(got.method.as_str(), "GET");
    assert_eq!(got.url.path(), "/items");
    assert_eq!(got.url.query(), Some("a=1&b=two"));
}

#[tokio::test]
async fn url_without_scheme_defaults_to_http() {
    let server = ok_server().await;
    let bare = server.uri().replace("http://", "");
    let resp = send(&req(HttpMethod::Get, format!("{bare}/x"))).await;
    assert_eq!(resp.expect("response").status, 200);
}

#[tokio::test]
async fn post_json_defaults_content_type_and_sends_body() {
    let server = ok_server().await;
    let mut r = req(HttpMethod::Post, server.uri());
    r.body = RequestBody::Raw {
        language: RawLanguage::Json,
        text: r#"{"hello":"world"}"#.into(),
    };
    send(&r).await.expect("response");
    let got = only_request(&server).await;
    assert_eq!(header(&got, "content-type"), Some("application/json"));
    assert_eq!(got.body, br#"{"hello":"world"}"#);
}

#[tokio::test]
async fn user_content_type_wins_for_raw() {
    let server = ok_server().await;
    let mut r = req(HttpMethod::Put, server.uri());
    r.headers = vec![KeyValue::new("Content-Type", "application/vnd.custom+json")];
    r.body = RequestBody::Raw {
        language: RawLanguage::Json,
        text: "{}".into(),
    };
    send(&r).await.expect("response");
    let got = only_request(&server).await;
    let all: Vec<_> = got.headers.get_all("content-type").iter().collect();
    assert_eq!(all, ["application/vnd.custom+json"]);
}

#[tokio::test]
async fn urlencoded_sends_enabled_rows() {
    let server = ok_server().await;
    let mut off = KeyValue::new("skip", "me");
    off.enabled = false;
    let mut r = req(HttpMethod::Post, server.uri());
    r.body = RequestBody::UrlEncoded {
        fields: vec![
            KeyValue::new("name", "Jane Doe"),
            off,
            KeyValue::new("x", "a&b"),
        ],
    };
    send(&r).await.expect("response");
    let got = only_request(&server).await;
    assert_eq!(
        header(&got, "content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(got.body, b"name=Jane+Doe&x=a%26b");
}

#[tokio::test]
async fn multipart_text_and_file() {
    let server = ok_server().await;
    let mut file = tempfile::Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("tempfile");
    file.write_all(b"file contents here").expect("write");
    let path = file.path().to_string_lossy().into_owned();
    let mut r = req(HttpMethod::Post, server.uri());
    // A boundary-less user Content-Type must not break the body.
    r.headers = vec![KeyValue::new("Content-Type", "multipart/form-data")];
    r.body = RequestBody::FormData {
        fields: vec![
            FormField {
                key: "greeting".into(),
                value: "hi there".into(),
                ..FormField::default()
            },
            FormField {
                key: "upload".into(),
                value: path,
                kind: FormFieldKind::File,
                enabled: true,
            },
        ],
    };
    send(&r).await.expect("response");
    let got = only_request(&server).await;
    let ct = header(&got, "content-type").expect("content-type");
    assert!(ct.starts_with("multipart/form-data; boundary="), "{ct}");
    let body = String::from_utf8_lossy(&got.body);
    assert!(body.contains("name=\"greeting\""));
    assert!(body.contains("hi there"));
    assert!(body.contains("name=\"upload\"; filename=\""));
    assert!(body.contains("Content-Type: text/plain"));
    assert!(body.contains("file contents here"));
}

#[tokio::test]
async fn binary_body_reads_file() {
    let server = ok_server().await;
    let mut file = tempfile::NamedTempFile::new().expect("tempfile");
    file.write_all(&[0, 1, 2, 255]).expect("write");
    let mut r = req(HttpMethod::Post, server.uri());
    r.body = RequestBody::Binary {
        path: Some(file.path().to_string_lossy().into_owned()),
    };
    send(&r).await.expect("response");
    let got = only_request(&server).await;
    assert_eq!(
        header(&got, "content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(got.body, [0, 1, 2, 255]);
}

#[tokio::test]
async fn disabled_headers_not_sent() {
    let server = ok_server().await;
    let mut off = KeyValue::new("X-Disabled", "1");
    off.enabled = false;
    let mut r = req(HttpMethod::Get, server.uri());
    r.headers = vec![KeyValue::new("X-On", "yes"), off];
    send(&r).await.expect("response");
    let got = only_request(&server).await;
    assert_eq!(header(&got, "x-on"), Some("yes"));
    assert!(header(&got, "x-disabled").is_none());
}

#[tokio::test]
async fn variables_substituted_everywhere() {
    let server = ok_server().await;
    let mut vars = VariableSet::new();
    vars.insert("base", server.uri());
    vars.insert("id", "42");
    vars.insert("hv", "header-value");
    vars.insert("name", "Ferrum");
    let mut r = req(HttpMethod::Post, "{{base}}/users/{{id}}?q={{id}}");
    r.headers = vec![KeyValue::new("X-Var", "{{hv}}")];
    r.body = RequestBody::Raw {
        language: RawLanguage::Text,
        text: "hello {{name}}".into(),
    };
    send_vars(&r, &vars).await.expect("response");
    let got = only_request(&server).await;
    assert_eq!(got.url.path(), "/users/42");
    assert_eq!(got.url.query(), Some("q=42"));
    assert_eq!(header(&got, "x-var"), Some("header-value"));
    assert_eq!(header(&got, "content-type"), Some("text/plain"));
    assert_eq!(got.body, b"hello Ferrum");
}
