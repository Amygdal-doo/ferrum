//! Digest, OAuth 1.0a and AWS SigV4 against a local server: each signature
//! the server receives is recomputed from what it received.

mod common;

use common::{header, ok_server, only_request, req, send, send_vars};
use ferrumapi_domain::{
    Auth, AwsSigV4Auth, DigestAuth, HttpMethod, KeyValue, OAuth1Auth, RawLanguage, RequestBody,
    Variable, VariableSet,
};
use ferrumapi_http::signing::{aws, digest, oauth1};
use wiremock::matchers::{any, header_exists};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The URL as sent (wiremock reports `localhost` without the port).
fn real_url(server: &MockServer, got: &wiremock::Request) -> url::Url {
    let mut url = url::Url::parse(&server.uri()).expect("server url");
    url.set_path(got.url.path());
    url.set_query(got.url.query());
    url
}

/// `key="value"` from an auth header.
fn field(h: &str, key: &str) -> String {
    let start = h.find(&format!("{key}=\"")).expect(key) + key.len() + 2;
    let len = h[start..].find('"').expect("closing quote");
    h[start..start + len].to_owned()
}

#[tokio::test]
async fn digest_answers_the_challenge_once() {
    let server = MockServer::start().await;
    Mock::given(header_exists("authorization"))
        .respond_with(ResponseTemplate::new(200).set_body_string("in"))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(401).insert_header(
            "www-authenticate",
            r#"Digest realm="api", qop="auth", nonce="n0nce", opaque="op""#,
        ))
        .mount(&server)
        .await;
    let mut r = req(HttpMethod::Post, format!("{}/dir/x?a=1", server.uri()));
    r.body = RequestBody::Raw {
        language: RawLanguage::Json,
        text: "{\"k\":1}".into(),
    };
    r.auth = Auth::Digest(Box::new(DigestAuth {
        username: "Mufasa".into(),
        password: "{{pw}}".into(),
        ..DigestAuth::default()
    }));
    let vars = VariableSet::from_layers(&[&[Variable::new("pw", "Circle Of Life")]]);
    let resp = send_vars(&r, &vars).await.expect("response");
    assert_eq!(resp.status, 200);
    let all = server.received_requests().await.expect("recorded");
    assert_eq!(all.len(), 2);
    assert!(all[0].headers.get("authorization").is_none());
    assert_eq!(all[1].body, b"{\"k\":1}", "the body is sent again");
    let h = header(&all[1], "authorization").expect("digest").to_owned();
    let c =
        digest::parse_challenge([r#"Digest realm="api", qop="auth", nonce="n0nce", opaque="op""#])
            .expect("challenge");
    let expected = digest::authorization(
        &c,
        "Mufasa",
        "Circle Of Life",
        "POST",
        "/dir/x?a=1",
        &field(&h, "cnonce"),
        1,
    );
    assert_eq!(h, expected);
}

#[tokio::test]
async fn digest_without_challenge_returns_the_401() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let mut r = req(HttpMethod::Get, server.uri());
    r.auth = Auth::Digest(Box::default());
    assert_eq!(send(&r).await.expect("response").status, 401);
    only_request(&server).await;
}

#[tokio::test]
async fn oauth1_signs_query_and_form_body() {
    let server = ok_server().await;
    let cfg = OAuth1Auth {
        consumer_key: "ck".into(),
        consumer_secret: "cs".into(),
        token: "tk".into(),
        token_secret: "ts".into(),
        ..OAuth1Auth::default()
    };
    let mut r = req(HttpMethod::Post, format!("{}/statuses?b=2", server.uri()));
    r.body = RequestBody::UrlEncoded {
        fields: vec![KeyValue::new("status", "hi there!")],
    };
    r.auth = Auth::OAuth1(Box::new(cfg.clone()));
    send(&r).await.expect("response");
    let got = only_request(&server).await;
    let h = header(&got, "authorization").expect("oauth").to_owned();
    let form = vec![("status".to_owned(), "hi there!".to_owned())];
    let expected = oauth1::authorization(
        "POST",
        &real_url(&server, &got),
        &form,
        &cfg,
        &field(&h, "oauth_nonce"),
        field(&h, "oauth_timestamp").parse().expect("timestamp"),
    );
    assert_eq!(h, expected);
}

#[tokio::test]
async fn aws_signs_body_hash_and_content_type() {
    let server = ok_server().await;
    let cfg = AwsSigV4Auth {
        access_key_id: "AKID".into(),
        secret_access_key: "secret".into(),
        region: "eu-west-1".into(),
        service: "execute-api".into(),
        ..AwsSigV4Auth::default()
    };
    let mut r = req(HttpMethod::Put, format!("{}/items/1?z=1&a=2", server.uri()));
    r.body = RequestBody::Raw {
        language: RawLanguage::Json,
        text: "{\"n\":1}".into(),
    };
    r.auth = Auth::AwsSigV4(Box::new(cfg.clone()));
    send(&r).await.expect("response");
    let got = only_request(&server).await;
    let h = header(&got, "authorization").expect("aws").to_owned();
    assert!(
        h.contains("SignedHeaders=content-type;host;x-amz-date"),
        "{h}"
    );
    let date = header(&got, "x-amz-date").expect("date");
    let payload = {
        use sha2::Digest as _;
        let d = sha2::Sha256::digest(b"{\"n\":1}");
        d.iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    let extra = vec![("content-type".to_owned(), "application/json".to_owned())];
    let expected = aws::sign(
        "PUT",
        &real_url(&server, &got),
        &extra,
        &payload,
        &cfg,
        date,
    );
    assert_eq!(h, expected.authorization);
}

#[tokio::test]
async fn user_authorization_header_beats_signing() {
    let server = ok_server().await;
    let mut r = req(HttpMethod::Get, server.uri());
    r.headers = vec![KeyValue::new("Authorization", "Mine")];
    r.auth = Auth::AwsSigV4(Box::default());
    send(&r).await.expect("response");
    let got = only_request(&server).await;
    assert_eq!(header(&got, "authorization"), Some("Mine"));
    assert!(got.headers.get("x-amz-date").is_none());
}
