//! WebSocket client against a local tokio-tungstenite echo server.

use ferrumapi_domain::protocol::WsPayloadKind;
use ferrumapi_domain::{Auth, HttpRequest, KeyValue, RequestKind, VariableSet};
use ferrumapi_http::ws::{WsIncoming, WsOutgoing, connect, outgoing, parse_ws_url};
use futures::{SinkExt as _, StreamExt as _};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::http::HeaderValue;

type Seen = Arc<Mutex<Vec<(String, String)>>>;

/// Echo server (the handshake callback's error type is tungstenite's):
/// records handshake headers, picks the first offered
/// subprotocol, echoes text/binary, closes with 4000 on "bye".
async fn echo_server() -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let seen: Seen = Arc::default();
    let record = seen.clone();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let record = record.clone();
            tokio::spawn(async move {
                #[allow(clippy::result_large_err)]
                let callback = |req: &Request, mut resp: Response| {
                    let mut rec = record.lock().expect("lock");
                    for (k, v) in req.headers() {
                        rec.push((k.to_string(), v.to_str().unwrap_or_default().to_owned()));
                    }
                    rec.push(("path".into(), req.uri().to_string()));
                    if let Some(p) = req.headers().get("sec-websocket-protocol") {
                        let first = p
                            .to_str()
                            .unwrap_or_default()
                            .split(',')
                            .next()
                            .unwrap_or_default();
                        resp.headers_mut().insert(
                            "sec-websocket-protocol",
                            HeaderValue::from_str(first.trim()).expect("value"),
                        );
                    }
                    Ok(resp)
                };
                let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(tcp, callback).await else {
                    return;
                };
                while let Some(Ok(msg)) = ws.next().await {
                    match msg {
                        Message::Text(t) if t.as_str() == "bye" => {
                            let frame = tokio_tungstenite::tungstenite::protocol::CloseFrame {
                                code: 4000.into(),
                                reason: "server says bye".into(),
                            };
                            let _ = ws.send(Message::Close(Some(frame))).await;
                            break;
                        }
                        Message::Text(_) | Message::Binary(_) => {
                            if ws.send(msg).await.is_err() {
                                break;
                            }
                        }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
            });
        }
    });
    (format!("127.0.0.1:{}", addr.port()), seen)
}

async fn next(rx: &mut tokio::sync::mpsc::UnboundedReceiver<WsIncoming>) -> WsIncoming {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timely frame")
        .expect("open channel")
}

fn request(url: &str) -> HttpRequest {
    let mut r = HttpRequest::of_kind(RequestKind::WebSocket);
    r.url = url.into();
    r
}

#[tokio::test]
async fn echoes_text_json_and_binary_with_headers_auth_and_subprotocol() {
    let (addr, seen) = echo_server().await;
    let mut r = request("ws://{{host}}/chat?room=1");
    r.headers.push(KeyValue::new("X-Team", "{{team}}"));
    r.auth = Auth::Bearer {
        token: "{{token}}".into(),
    };
    r.specs.websocket.subprotocols = "chat.v2, chat.v1".into();
    let mut vars = VariableSet::new();
    vars.insert("host", addr.as_str());
    vars.insert("team", "core");
    vars.insert("token", "s3cret");
    let (connected, client, mut rx) = connect(&r, &vars).await.expect("connect");
    assert_eq!(connected.status, 101);
    assert_eq!(connected.protocol.as_deref(), Some("chat.v2"));
    {
        let seen = seen.lock().expect("lock");
        let get = |k: &str| seen.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(get("x-team").as_deref(), Some("core"));
        assert_eq!(get("authorization").as_deref(), Some("Bearer s3cret"));
        assert_eq!(get("path").as_deref(), Some("/chat?room=1"));
    }

    assert!(client.send(outgoing(WsPayloadKind::Text, "hello").expect("text")));
    assert_eq!(next(&mut rx).await, WsIncoming::Text("hello".into()));
    assert!(client.send(outgoing(WsPayloadKind::Json, "{\"a\": 1}").expect("json")));
    assert_eq!(next(&mut rx).await, WsIncoming::Text("{\"a\": 1}".into()));
    assert!(client.send(outgoing(WsPayloadKind::Binary, "AAEC /w==").expect("binary")));
    assert_eq!(next(&mut rx).await, WsIncoming::Binary(vec![0, 1, 2, 255]));

    assert!(client.send(WsOutgoing::Text("bye".into())));
    assert_eq!(
        next(&mut rx).await,
        WsIncoming::Closed {
            code: Some(4000),
            reason: "server says bye".into()
        }
    );
}

#[tokio::test]
async fn dropping_the_client_closes_normally() {
    let (addr, _) = echo_server().await;
    let (_, client, mut rx) = connect(
        &request(&format!("http://{addr}/")),
        &VariableSet::default(),
    )
    .await
    .expect("connect (http:// maps to ws://)");
    drop(client);
    assert!(matches!(
        next(&mut rx).await,
        WsIncoming::Closed {
            code: Some(1000),
            ..
        }
    ));
}

#[tokio::test]
async fn errors_are_reported_without_connecting() {
    assert!(outgoing(WsPayloadKind::Json, "{oops").is_err());
    assert!(outgoing(WsPayloadKind::Binary, "!!!").is_err());
    assert!(parse_ws_url("ftp://x").is_err());
    assert!(parse_ws_url("").is_err());
    assert_eq!(
        parse_ws_url("localhost:1/a").expect("url").as_str(),
        "ws://localhost:1/a"
    );
    assert_eq!(parse_ws_url("https://h/s").expect("url").scheme(), "wss");
    // Nothing listens on port 9 on loopback.
    let err = connect(&request("ws://127.0.0.1:9/"), &VariableSet::default())
        .await
        .expect_err("refused");
    assert!(
        matches!(err, ferrumapi_domain::ExecError::Connect(_)),
        "{err:?}"
    );
}
