//! Settings, errors, cancellation and response parsing against a local server.

mod common;

use common::{req, send};
use ferrumapi_domain::{ExecError, HttpMethod, VariableSet};
use ferrumapi_http::{CancellationToken, HttpClient};
use std::time::{Duration, Instant};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn redirect_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path("/old"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/new"))
        .mount(&server)
        .await;
    Mock::given(path("/new"))
        .respond_with(ResponseTemplate::new(200).set_body_string("landed"))
        .mount(&server)
        .await;
    Mock::given(path("/loop"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/loop"))
        .mount(&server)
        .await;
    server
}

async fn slow_server(delay: Duration) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(delay))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn follows_redirects_when_enabled() {
    let server = redirect_server().await;
    let r = req(HttpMethod::Get, format!("{}/old", server.uri()));
    let resp = send(&r).await.expect("response");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"landed");
    assert_eq!(resp.final_url, format!("{}/new", server.uri()));
}

#[tokio::test]
async fn does_not_follow_redirects_when_disabled() {
    let server = redirect_server().await;
    let mut r = req(HttpMethod::Get, format!("{}/old", server.uri()));
    r.settings.follow_redirects = false;
    let resp = send(&r).await.expect("response");
    assert_eq!(resp.status, 302);
    assert_eq!(resp.status_text, "Found");
    assert_eq!(resp.header("location"), Some("/new"));
    assert_eq!(resp.final_url, format!("{}/old", server.uri()));
}

#[tokio::test]
async fn redirect_loop_is_redirect_error() {
    let server = redirect_server().await;
    let r = req(HttpMethod::Get, format!("{}/loop", server.uri()));
    assert_eq!(send(&r).await, Err(ExecError::Redirect));
}

#[tokio::test]
async fn timeout_maps_to_timeout() {
    let server = slow_server(Duration::from_secs(5)).await;
    let mut r = req(HttpMethod::Get, server.uri());
    r.settings.timeout_ms = 200;
    let started = Instant::now();
    assert_eq!(send(&r).await, Err(ExecError::Timeout(200)));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn cancel_mid_request_returns_promptly() {
    let server = slow_server(Duration::from_secs(10)).await;
    let mut r = req(HttpMethod::Get, server.uri());
    r.settings.timeout_ms = 0;
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        trigger.cancel();
    });
    let started = Instant::now();
    let result = HttpClient::new()
        .execute(&r, &VariableSet::new(), cancel)
        .await;
    assert_eq!(result, Err(ExecError::Cancelled));
    assert!(started.elapsed() < Duration::from_millis(1200));
}

#[tokio::test]
async fn already_cancelled_token() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let r = req(HttpMethod::Get, "http://127.0.0.1:9/");
    let result = HttpClient::new()
        .execute(&r, &VariableSet::new(), cancel)
        .await;
    assert_eq!(result, Err(ExecError::Cancelled));
}

#[tokio::test]
async fn connection_refused_is_connect() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    let r = req(
        HttpMethod::Get,
        format!("http://127.0.0.1:{port}/?api_key=SECRET"),
    );
    let err = send(&r).await.expect_err("refused");
    let ExecError::Connect(msg) = &err else {
        panic!("expected Connect, got {err:?}");
    };
    assert!(
        !msg.contains("SECRET") && !msg.contains("127.0.0.1"),
        "{msg}"
    );
}

#[tokio::test]
async fn invalid_url_and_missing_file() {
    let r = req(HttpMethod::Get, "http://exa mple.com");
    assert!(matches!(send(&r).await, Err(ExecError::InvalidUrl(_))));
    let mut r = req(HttpMethod::Post, "http://127.0.0.1:9/");
    r.body = ferrumapi_domain::RequestBody::Binary {
        path: Some("/no/such/file.bin".into()),
    };
    assert!(matches!(send(&r).await, Err(ExecError::File(_))));
}

#[tokio::test]
async fn head_request_has_empty_body() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(200).insert_header("x-head", "yes"))
        .mount(&server)
        .await;
    let resp = send(&req(HttpMethod::Head, server.uri()))
        .await
        .expect("response");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.header("x-head"), Some("yes"));
    assert!(resp.body.is_empty());
    assert_eq!(resp.size_bytes, 0);
}

#[tokio::test]
async fn set_cookie_headers_parsed_and_repeats_kept() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(201)
                .append_header("set-cookie", "sid=abc; Path=/; HttpOnly")
                .append_header(
                    "set-cookie",
                    "theme=dark; domain=.example.test; max-age=60; secure",
                )
                .set_body_string("made"),
        )
        .mount(&server)
        .await;
    let resp = send(&req(HttpMethod::Get, server.uri()))
        .await
        .expect("response");
    assert_eq!((resp.status, resp.status_text.as_str()), (201, "Created"));
    let set_cookies = resp
        .headers
        .iter()
        .filter(|(k, _)| k == "set-cookie")
        .count();
    assert_eq!(set_cookies, 2);
    assert_eq!(resp.cookies.len(), 2);
    assert_eq!(resp.cookies[0].name, "sid");
    assert!(resp.cookies[0].http_only);
    assert_eq!(resp.cookies[0].path.as_deref(), Some("/"));
    assert_eq!(resp.cookies[1].domain.as_deref(), Some("example.test"));
    assert_eq!(resp.cookies[1].expires.as_deref(), Some("Max-Age=60"));
    assert!(resp.cookies[1].secure);
}

#[tokio::test]
async fn client_is_reused_across_settings() {
    let server = slow_server(Duration::ZERO).await;
    let client = HttpClient::new();
    for (verify, follow) in [(true, true), (false, false), (true, true)] {
        let mut r = req(HttpMethod::Get, server.uri());
        r.settings.verify_tls = verify;
        r.settings.follow_redirects = follow;
        let resp = client
            .execute(&r, &VariableSet::new(), CancellationToken::new())
            .await
            .expect("response");
        assert_eq!(resp.status, 200);
    }
}
