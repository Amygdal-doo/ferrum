//! Mock server and webhook inspector over real sockets (127.0.0.1, ephemeral ports).

use ferrumapi_domain::mock::{MockResponse, MockRoute};
use ferrumapi_domain::webhook::WebhookReply;
use ferrumapi_domain::{HttpMethod, KeyValue};
use ferrumapi_server::{MockServer, ServerError, WebhookInspector, bind_local};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::sync::mpsc::unbounded_channel;

struct Reply {
    status: u16,
    head: String,
    body: String,
}

/// Minimal HTTP/1.1 client: one request per connection.
async fn send(
    addr: SocketAddr,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Reply {
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let mut req = format!("{method} {target} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    stream.write_all(req.as_bytes()).await.expect("write");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").expect("head/body");
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("status");
    Reply {
        status,
        head: head.to_ascii_lowercase(),
        body: body.to_owned(),
    }
}

fn route(name: &str, method: HttpMethod, path: &str, status: u16, body: &str) -> MockRoute {
    MockRoute {
        name: name.into(),
        method,
        path: path.into(),
        response: MockResponse {
            status,
            body: body.into(),
            ..MockResponse::default()
        },
    }
}

#[tokio::test]
async fn mock_server_matches_routes_logs_and_lists_on_404() {
    let mut admin = route(
        "Get user",
        HttpMethod::Get,
        "/users/:id",
        200,
        r#"{"role":"admin"}"#,
    );
    admin.response.name = "admin".into();
    admin.response.match_headers = vec![KeyValue::new("X-Role", "admin")];
    admin.response.headers.push(KeyValue::new("X-Extra", "yes"));
    let routes = vec![
        route(
            "Get user",
            HttpMethod::Get,
            "/users/{id}",
            200,
            r#"{"id":1}"#,
        ),
        admin,
        route("Create", HttpMethod::Post, "/users", 201, "created"),
    ];
    let (tx, mut rx) = unbounded_channel();
    let server = MockServer::start(0, routes, tx).await.expect("start");
    assert!(server.addr().ip().is_loopback());
    let addr = server.addr();

    let r = send(addr, "GET", "/users/42", &[], "").await;
    assert_eq!((r.status, r.body.as_str()), (200, r#"{"id":1}"#));
    assert!(r.head.contains("content-type: application/json"));

    let r = send(addr, "GET", "/users/42", &[("x-role", "admin")], "").await;
    assert_eq!(r.body, r#"{"role":"admin"}"#);
    assert!(r.head.contains("x-extra: yes"));

    let r = send(addr, "POST", "/users?x=1", &[], "{}").await;
    assert_eq!(r.status, 201);

    let r = send(addr, "DELETE", "/users/1", &[], "").await;
    assert_eq!(r.status, 404);
    let listing: serde_json::Value = serde_json::from_str(&r.body).expect("json");
    assert_eq!(listing["routes"].as_array().map(Vec::len), Some(3));

    let log: Vec<_> = (0..4).map(|_| rx.try_recv().expect("log entry")).collect();
    assert_eq!(log[0].matched.as_deref(), Some("Get user"));
    assert_eq!(log[1].matched.as_deref(), Some("Get user · admin"));
    assert_eq!((log[2].target.as_str(), log[2].status), ("/users?x=1", 201));
    assert_eq!((log[3].matched.as_ref(), log[3].status), (None, 404));

    // Routes can be swapped while running.
    server.set_routes(vec![route("Health", HttpMethod::Get, "/health", 204, "")]);
    assert_eq!(server.route_count(), 1);
    assert_eq!(send(addr, "GET", "/health", &[], "").await.status, 204);
    assert_eq!(send(addr, "GET", "/users/1", &[], "").await.status, 404);
    server.shutdown().await;
    assert!(TcpStream::connect(addr).await.is_err(), "port released");
}

#[tokio::test]
async fn mock_delay_is_applied() {
    let mut slow = route("Slow", HttpMethod::Get, "/slow", 200, "ok");
    slow.response.delay_ms = 150;
    // No explicit Content-Type: one is inferred from the body.
    slow.response.headers.clear();
    let (tx, _rx) = unbounded_channel();
    let server = MockServer::start(0, vec![slow], tx).await.expect("start");
    let started = Instant::now();
    let r = send(server.addr(), "GET", "/slow", &[], "").await;
    assert_eq!(r.body, "ok");
    assert!(r.head.contains("content-type: text/plain"));
    assert!(started.elapsed() >= Duration::from_millis(150));
    server.shutdown().await;
}

#[tokio::test]
async fn taken_port_is_a_clear_error() {
    let first = bind_local(0).await.expect("bind");
    let port = first.local_addr().expect("addr").port();
    let (tx, _rx) = unbounded_channel();
    let err = MockServer::start(port, Vec::new(), tx)
        .await
        .expect_err("taken");
    assert_eq!(err, ServerError::PortInUse(port));
    assert!(
        err.to_string()
            .contains(&format!("port {port} is already in use"))
    );
    let (tx, _rx) = unbounded_channel();
    let err = WebhookInspector::start(port, WebhookReply::default(), 10, tx)
        .await
        .expect_err("taken");
    assert_eq!(err, ServerError::PortInUse(port));
}

#[tokio::test]
async fn inspector_records_any_path_and_uses_the_reply() {
    let (tx, mut rx) = unbounded_channel();
    let hook = WebhookInspector::start(0, WebhookReply::default(), 2, tx)
        .await
        .expect("start");
    let addr = hook.addr();
    let r = send(
        addr,
        "POST",
        "/stripe/events?live=1",
        &[
            ("Content-Type", "application/json"),
            ("Stripe-Signature", "t=1"),
        ],
        r#"{"type":"charge.succeeded"}"#,
    )
    .await;
    assert_eq!((r.status, r.body.as_str()), (200, r#"{"ok":true}"#));
    assert_eq!(rx.recv().await, Some(1));

    let captures = hook.captures();
    let c = &captures[0];
    assert_eq!(
        (c.method.as_str(), c.path.as_str(), c.query.as_str()),
        ("POST", "/stripe/events", "live=1")
    );
    assert_eq!(c.header("stripe-signature"), Some("t=1"));
    assert_eq!(c.size, 27);
    assert!(c.display_body().contains("\"type\": \"charge.succeeded\""));

    hook.set_reply(WebhookReply {
        status: 503,
        content_type: "text/plain".into(),
        body: "busy".into(),
    });
    let r = send(addr, "GET", "/a", &[], "").await;
    assert_eq!((r.status, r.body.as_str()), (503, "busy"));
    send(addr, "PUT", "/b", &[], "x").await;
    // Capped at 2: the oldest was dropped.
    let paths: Vec<String> = hook.captures().into_iter().map(|c| c.path).collect();
    assert_eq!(paths, ["/b", "/a"]);
    hook.clear();
    assert!(hook.captures().is_empty());
    hook.shutdown().await;
}
