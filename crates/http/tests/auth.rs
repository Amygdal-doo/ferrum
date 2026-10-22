//! Auth application against a local server.

mod common;

use common::{header, ok_server, only_request, req, send};
use ferrumapi_domain::{ApiKeyLocation, Auth, HttpMethod, KeyValue};

#[tokio::test]
async fn basic_auth() {
    let server = ok_server().await;
    let mut r = req(HttpMethod::Get, server.uri());
    r.auth = Auth::Basic {
        username: "aladdin".into(),
        password: "opensesame".into(),
    };
    send(&r).await.expect("response");
    let got = only_request(&server).await;
    assert_eq!(
        header(&got, "authorization"),
        Some("Basic YWxhZGRpbjpvcGVuc2VzYW1l")
    );
}

#[tokio::test]
async fn bearer_auth_and_user_header_override() {
    let server = ok_server().await;
    let mut r = req(HttpMethod::Get, server.uri());
    r.auth = Auth::Bearer {
        token: "abc.def".into(),
    };
    send(&r).await.expect("response");
    assert_eq!(
        header(&only_request(&server).await, "authorization"),
        Some("Bearer abc.def")
    );

    let server = ok_server().await;
    let mut r = req(HttpMethod::Get, server.uri());
    r.auth = Auth::Bearer {
        token: "abc.def".into(),
    };
    r.headers = vec![KeyValue::new("authorization", "Token mine")];
    send(&r).await.expect("response");
    let got = only_request(&server).await;
    let all: Vec<_> = got.headers.get_all("authorization").iter().collect();
    assert_eq!(all, ["Token mine"]);
}

#[tokio::test]
async fn api_key_header() {
    let server = ok_server().await;
    let mut r = req(HttpMethod::Get, server.uri());
    r.auth = Auth::ApiKey {
        key: "X-Api-Key".into(),
        value: "k-123".into(),
        location: ApiKeyLocation::Header,
    };
    send(&r).await.expect("response");
    let got = only_request(&server).await;
    assert_eq!(header(&got, "x-api-key"), Some("k-123"));
    assert_eq!(got.url.query(), None);
}

#[tokio::test]
async fn api_key_query() {
    let server = ok_server().await;
    let mut r = req(HttpMethod::Get, format!("{}/p?a=1", server.uri()));
    r.auth = Auth::ApiKey {
        key: "api_key".into(),
        value: "k 123".into(),
        location: ApiKeyLocation::Query,
    };
    send(&r).await.expect("response");
    let got = only_request(&server).await;
    assert_eq!(got.url.query(), Some("a=1&api_key=k+123"));
    assert!(header(&got, "api_key").is_none());
}

#[tokio::test]
async fn inherit_sends_no_auth() {
    let server = ok_server().await;
    let r = req(HttpMethod::Get, server.uri());
    assert_eq!(r.auth, Auth::Inherit);
    send(&r).await.expect("response");
    assert!(header(&only_request(&server).await, "authorization").is_none());
}
