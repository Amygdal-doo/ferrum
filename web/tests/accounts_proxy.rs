//! The account paths: proxied to `ACCOUNTS_URL` (a tiny upstream started
//! in-test), or 404 / "coming soon" without it.
#![allow(clippy::unwrap_used)]

use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::Request,
    http::{HeaderMap, Method, StatusCode, header},
    response::{AppendHeaders, IntoResponse},
    routing::get,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use web_ferrum::{Config, releases::Releases, routes::app_with};

struct Res {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Res {
    fn header(&self, name: header::HeaderName) -> &str {
        self.headers.get(name).map_or("", |v| v.to_str().unwrap())
    }
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("{e}: {} {}", self.status, self.body))
    }
}

async fn send(accounts_url: Option<&str>, req: Request<Body>) -> Res {
    let vars = [
        ("SITE_URL", Some("https://tryferrum.test")),
        ("ACCOUNTS_URL", accounts_url),
    ];
    let config = Config::from_vars(|k| {
        vars.iter()
            .find(|(n, _)| *n == k)
            .and_then(|(_, v)| v.map(str::to_owned))
    });
    let res = app_with(config, Releases::offline())
        .oneshot(req)
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    Res {
        status,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    }
}

fn get_req(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

/// Echoes what it received as JSON; `/login` answers with a redirect and
/// two cookies, like the real service after sign-in.
async fn upstream() -> String {
    async fn echo(req: Request) -> Json<Value> {
        let (parts, body) = req.into_parts();
        let body: Bytes = body.collect().await.unwrap().to_bytes();
        let h = |name: &str| {
            parts
                .headers
                .get(name)
                .map(|v| v.to_str().unwrap().to_owned())
        };
        Json(json!({
            "method": parts.method.as_str(),
            "uri": parts.uri.to_string(),
            "cookie": h("cookie"),
            "origin": h("origin"),
            "authorization": h("authorization"),
            "accept_encoding": h("accept-encoding"),
            "x_forwarded_for": h("x-forwarded-for"),
            "x_forwarded_proto": h("x-forwarded-proto"),
            "x_forwarded_host": h("x-forwarded-host"),
            "body": String::from_utf8_lossy(&body),
        }))
    }
    let app = Router::new()
        .route(
            "/auth/github/callback",
            get(|| async {
                (
                    StatusCode::FOUND,
                    AppendHeaders([
                        (header::LOCATION, "/account"),
                        (header::SET_COOKIE, "ferrum_session=frt_x; Path=/; HttpOnly"),
                        (header::SET_COOKIE, "ferrum_oauth_state=; Path=/; Max-Age=0"),
                    ]),
                )
                    .into_response()
            }),
        )
        .route(
            "/account/big",
            get(|| async { "x".repeat(3 * 1024 * 1024) }),
        )
        .fallback(echo);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/")
}

#[tokio::test]
async fn without_accounts_url_login_is_coming_soon_and_the_rest_is_404() {
    for path in ["/login", "/signup?next=/account"] {
        let res = send(None, get_req(path)).await;
        assert_eq!(res.status, StatusCode::OK, "{path}");
        assert!(res.body.contains("Sign-in is coming soon"), "{path}");
        assert!(
            res.body
                .contains(r#"<meta name="robots" content="noindex">"#)
        );
        assert_eq!(res.header(header::CACHE_CONTROL), "no-store");
    }
    for path in [
        "/account",
        "/account/sessions",
        "/auth/github",
        "/oauth/authorize?client_id=ferrum-git",
        "/api/v1/me",
    ] {
        let res = send(None, get_req(path)).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{path}");
        assert!(res.body.contains("rusted away"), "{path}");
    }
    let res = send(None, Request::post("/logout").body(Body::empty()).unwrap()).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn forwards_method_path_query_body_and_headers() {
    let up = upstream().await;
    let req = Request::post("/account/username?x=1&y=%2F")
        .header(header::HOST, "tryferrum.test")
        .header(header::COOKIE, "ferrum_session=frt_abc")
        .header(header::ORIGIN, "https://tryferrum.test")
        .header("x-forwarded-for", "203.0.113.7")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("username=abdul"))
        .unwrap();
    let res = send(Some(&up), req).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.header(header::CACHE_CONTROL), "no-store");
    let echo = res.json();
    assert_eq!(echo["method"], "POST");
    assert_eq!(echo["uri"], "/account/username?x=1&y=%2F");
    assert_eq!(echo["cookie"], "ferrum_session=frt_abc");
    assert_eq!(echo["origin"], "https://tryferrum.test");
    assert_eq!(echo["body"], "username=abdul");
    assert_eq!(echo["accept_encoding"], Value::Null);
    assert_eq!(echo["x_forwarded_for"], "203.0.113.7");
    assert_eq!(echo["x_forwarded_proto"], "https");
    assert_eq!(echo["x_forwarded_host"], "tryferrum.test");

    for (method, path) in [
        (Method::GET, "/login?next=%2Faccount"),
        (Method::GET, "/signup"),
        (Method::POST, "/logout"),
        (Method::GET, "/account"),
        (Method::GET, "/oauth/authorize?client_id=ferrum-git"),
        (Method::POST, "/api/v1/oauth/token"),
    ] {
        let req = Request::builder()
            .method(method.clone())
            .uri(path)
            .header(header::AUTHORIZATION, "Bearer fat_x")
            .body(Body::empty())
            .unwrap();
        let echo = send(Some(&up), req).await.json();
        assert_eq!(echo["method"], method.as_str(), "{path}");
        assert_eq!(echo["uri"], path);
        assert_eq!(echo["authorization"], "Bearer fat_x");
    }
}

#[tokio::test]
async fn passes_back_redirects_and_every_cookie() {
    let up = upstream().await;
    let res = send(Some(&up), get_req("/auth/github/callback?code=c&state=s")).await;
    assert_eq!(res.status, StatusCode::FOUND);
    assert_eq!(res.header(header::LOCATION), "/account");
    let cookies: Vec<_> = res.headers.get_all(header::SET_COOKIE).iter().collect();
    assert_eq!(cookies.len(), 2);
    assert_eq!(cookies[0], "ferrum_session=frt_x; Path=/; HttpOnly");
    assert_eq!(res.header(header::CACHE_CONTROL), "no-store");
}

#[tokio::test]
async fn limits_and_failures() {
    let up = upstream().await;
    let big = Request::post("/api/v1/oauth/token")
        .body(Body::from(vec![b'x'; 65 * 1024]))
        .unwrap();
    assert_eq!(
        send(Some(&up), big).await.status,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let res = send(Some(&up), get_req("/account/big")).await;
    assert_eq!(res.status, StatusCode::BAD_GATEWAY);
    // Nothing listens on this port any more.
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", l.local_addr().unwrap())
    };
    let res = send(Some(&closed), get_req("/account")).await;
    assert_eq!(res.status, StatusCode::BAD_GATEWAY);
    assert_eq!(res.header(header::CACHE_CONTROL), "no-store");
    // Other paths are untouched by the proxy.
    assert_eq!(
        send(Some(&up), get_req("/accounts")).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(Some(&up), get_req("/about")).await.status,
        StatusCode::OK
    );
}
