//! MCP against a tiny stdio server (this test binary re-run with
//! `FERRUM_FAKE_MCP=1`) and a tiny streamable-HTTP server, directly and
//! through a session whose model calls an MCP tool.

mod common;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use common::{Events, Peer, fake_process};
use ferrumagent_core::mcp::{McpClient, McpConfig, McpServerConfig};
use ferrumagent_core::permissions::Decision;
use ferrumagent_core::provider::mock::MockProvider;
use ferrumagent_core::session::{ChatSession, SessionConfig, SessionEvent};
use ferrumagent_core::thread::ToolStatus;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The fake stdio server (only when re-run as one).
#[test]
fn fake_mcp_server() {
    if std::env::var("FERRUM_FAKE_MCP").is_err() {
        return;
    }
    let mut peer = Peer::new();
    println!("a banner line that is not JSON");
    while let Some(message) = peer.read() {
        let id = message["id"].clone();
        match message["method"].as_str().unwrap_or_default() {
            "initialize" => peer.reply(
                &id,
                json!({"protocolVersion": "2025-06-18", "capabilities": {"tools": {}}, "serverInfo": {"name": "fake", "version": "1"}}),
            ),
            "tools/list" if message["params"]["cursor"] == "p2" => peer.reply(
                &id,
                json!({"tools": [{"name": "add", "description": "Adds", "inputSchema": {"type": "object"}}]}),
            ),
            "tools/list" => peer.reply(
                &id,
                json!({"tools": [{"name": "echo", "description": "Echoes", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}}], "nextCursor": "p2"}),
            ),
            "tools/call" => {
                let args = &message["params"]["arguments"];
                let result = match message["params"]["name"].as_str() {
                    Some("echo") => json!({"content": [{"type": "text", "text": args["text"]}]}),
                    Some("add") => json!({"content": [{"type": "text", "text": (args["a"].as_i64().unwrap_or(0) + args["b"].as_i64().unwrap_or(0)).to_string()}]}),
                    _ => json!({"content": [{"type": "text", "text": "no such tool"}], "isError": true}),
                };
                peer.reply(&id, result);
            }
            _ if id.is_null() => {}
            _ => peer.send(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "nope"}})),
        }
    }
    std::process::exit(0);
}

fn stdio_config() -> McpServerConfig {
    let (command, args) = fake_process("fake_mcp_server");
    let mut env = BTreeMap::new();
    env.insert("FERRUM_FAKE_MCP".into(), "1".into());
    McpServerConfig {
        command: Some(command),
        args,
        env,
        ..Default::default()
    }
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("rt")
}

#[test]
fn stdio_server_lists_pages_and_calls_tools() {
    let dir = tempfile::tempdir().expect("tempdir");
    rt().block_on(async {
        let client = McpClient::connect("fake", &stdio_config(), dir.path())
            .await
            .expect("connect");
        assert_eq!(client.server_info["name"], "fake");
        let tools = client.list_tools().await.expect("tools");
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["echo", "add"]);
        let out = client
            .call_tool("echo", json!({"text": "hello"}))
            .await
            .expect("call");
        assert_eq!((out.text.as_str(), out.is_error), ("hello", false));
        let sum = client
            .call_tool("add", json!({"a": 2, "b": 3}))
            .await
            .expect("call");
        assert_eq!(sum.text, "5");
        let bad = client.call_tool("nope", json!({})).await.expect("call");
        assert!(bad.is_error);
        client.shutdown().await;
    });
}

#[test]
fn session_routes_mcp_tools_with_permission() {
    let project = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("tempdir");
    let mock = MockProvider::new();
    mock.push_tool("m1", "mcp__fake__echo", json!({"text": "from mcp"}));
    mock.push_text("ok");
    let mut config = SessionConfig::isolated(project.path(), home.path());
    config.provider_override = Some(Arc::new(mock.clone()));
    let mut servers = BTreeMap::new();
    servers.insert("fake".to_string(), stdio_config());
    config.mcp = McpConfig { servers };
    let (session, rx) = ChatSession::new(config).expect("session");
    let mut events = Events::new(rx);
    session.send("use the tool", vec![]);
    let status = events.wait("MCP status", |e| match e {
        SessionEvent::Mcp(s) => Some(s.clone()),
        _ => None,
    });
    assert_eq!(status[0].result, Ok(2));
    let request = events.wait("the MCP permission prompt", |e| match e {
        SessionEvent::Permission(r) => Some(r.clone()),
        _ => None,
    });
    assert_eq!(request.tool, "mcp__fake__echo");
    session.respond_permission(request.id, Decision::AllowOnce);
    events.idle();
    let call = session.thread().tool("m1").cloned().expect("call");
    assert_eq!(call.status, ToolStatus::Completed);
    assert_eq!(call.output.as_deref(), Some("from mcp"));
    assert_eq!(call.title, "MCP fake: echo");
    assert!(
        mock.requests()[0]
            .tools
            .iter()
            .any(|t| t.name == "mcp__fake__add")
    );
}

/// A one-endpoint streamable-HTTP MCP server: JSON for most answers, SSE
/// for `tools/list`, a session id after `initialize`.
/// (method, Mcp-Session-Id header) per request.
type Seen = Arc<Mutex<Vec<(String, Option<String>)>>>;

async fn serve_http(listener: tokio::net::TcpListener, seen: Seen) {
    loop {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        let seen = seen.clone();
        tokio::spawn(async move {
            let mut buf = Vec::new();
            loop {
                let head_end = loop {
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                    let mut chunk = [0u8; 4096];
                    match socket.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                };
                let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                let header = |name: &str| {
                    head.lines()
                        .find(|l| l.to_lowercase().starts_with(&format!("{name}:")))
                        .map(|l| l[name.len() + 1..].trim().to_string())
                };
                let len: usize = header("content-length")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                while buf.len() < head_end + len {
                    let mut chunk = [0u8; 4096];
                    match socket.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let body: Value =
                    serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or(Value::Null);
                buf.drain(..head_end + len);
                let method = body["method"].as_str().unwrap_or_default().to_string();
                seen.lock()
                    .expect("lock")
                    .push((method.clone(), header("mcp-session-id")));
                let id = body["id"].clone();
                let (status, content_type, extra, payload) = match method.as_str() {
                    "initialize" => ("200 OK", "application/json", "Mcp-Session-Id: sess-42\r\n", json!({"jsonrpc": "2.0", "id": id, "result": {"protocolVersion": "2025-06-18", "capabilities": {}, "serverInfo": {"name": "http-fake"}}}).to_string()),
                    "tools/list" => ("200 OK", "text/event-stream", "", format!(
                        "event: message\ndata: {}\n\nevent: message\ndata: {}\n\n",
                        json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": {}}),
                        json!({"jsonrpc": "2.0", "id": id, "result": {"tools": [{"name": "time", "inputSchema": {"type": "object"}}]}}),
                    )),
                    "tools/call" => ("200 OK", "application/json", "", json!({"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": "noon"}]}}).to_string()),
                    _ => ("202 Accepted", "application/json", "", String::new()),
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\n{extra}Content-Length: {}\r\n\r\n{payload}",
                    payload.len()
                );
                if socket.write_all(response.as_bytes()).await.is_err() {
                    return;
                }
            }
        });
    }
}

#[test]
fn streamable_http_server_with_sse_and_session_id() {
    let dir = tempfile::tempdir().expect("tempdir");
    rt().block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let url = format!("http://{}/mcp", listener.local_addr().expect("addr"));
        let seen = Arc::new(Mutex::new(Vec::new()));
        tokio::spawn(serve_http(listener, seen.clone()));
        let config = McpServerConfig {
            url: Some(url),
            ..Default::default()
        };
        let client = McpClient::connect("web", &config, dir.path())
            .await
            .expect("connect");
        let tools = client.list_tools().await.expect("tools");
        assert_eq!(tools[0].name, "time");
        let out = client.call_tool("time", json!({})).await.expect("call");
        assert_eq!(out.text, "noon");
        let seen = seen.lock().expect("lock").clone();
        assert_eq!(
            seen,
            [
                ("initialize".to_string(), None),
                (
                    "notifications/initialized".to_string(),
                    Some("sess-42".to_string())
                ),
                ("tools/list".to_string(), Some("sess-42".to_string())),
                ("tools/call".to_string(), Some("sess-42".to_string())),
            ]
        );
    });
}
