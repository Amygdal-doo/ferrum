//! A tiny in-process account server implementing the parts of contract v1
//! the desktop client uses: `/oauth/authorize` (as a user who is signed in
//! and clicks Allow or Cancel), `/api/v1/oauth/token` (code, refresh with
//! rotation and reuse detection, exchange), `/api/v1/oauth/revoke` (with
//! children) and `/api/v1/me`.

#![allow(clippy::unwrap_used, dead_code)]

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug, Clone)]
pub struct Session {
    pub app: String,
    pub parent: Option<usize>,
    pub refresh: String,
    pub previous: Vec<String>,
    pub revoked: bool,
    pub device_id: String,
}

struct Code {
    client_id: String,
    redirect_uri: String,
    challenge: String,
}

#[derive(Default)]
pub struct State {
    pub sessions: Vec<Session>,
    codes: HashMap<String, Code>,
    /// access token → session index
    access: HashMap<String, usize>,
    counter: usize,
    /// `expires_in` handed out.
    pub expires_in: u64,
    /// The consent page answers Cancel.
    pub deny: bool,
    pub refresh_grants: usize,
}

impl State {
    fn token(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{prefix}{:0>43}", self.counter)
    }

    fn revoke(&mut self, ix: usize) {
        self.sessions[ix].revoked = true;
        let children: Vec<usize> = (0..self.sessions.len())
            .filter(|c| self.sessions[*c].parent == Some(ix))
            .collect();
        for child in children {
            self.revoke(child);
        }
    }

    fn live(&self, refresh: &str) -> Option<usize> {
        self.sessions
            .iter()
            .position(|s| s.refresh == refresh && !s.revoked)
    }

    fn issue(&mut self, ix: usize) -> Value {
        let access = self.token("fat_");
        self.access.insert(access.clone(), ix);
        json!({
            "access_token": access,
            "token_type": "Bearer",
            "expires_in": self.expires_in,
            "refresh_token": self.sessions[ix].refresh,
            "session_id": format!("s{ix}"),
            "user": me(),
        })
    }

    fn new_session(&mut self, app: &str, parent: Option<usize>, device_id: &str) -> usize {
        let refresh = self.token("frt_");
        self.sessions.push(Session {
            app: app.into(),
            parent,
            refresh,
            previous: Vec::new(),
            revoked: false,
            device_id: device_id.into(),
        });
        self.sessions.len() - 1
    }
}

pub fn me() -> Value {
    json!({
        "id": "6f9c0000-0000-0000-0000-000000000001",
        "username": "ada",
        "name": "Ada Lovelace",
        "email": "ada@example.com",
        "avatar_url": "https://avatars.githubusercontent.com/u/1?v=4",
        "account_url": "http://fake/account",
        "organizations": [{"id": "o1", "name": "ada", "plan": "free", "personal": true, "role": "owner"}]
    })
}

pub struct FakeServer {
    pub url: String,
    pub state: Arc<Mutex<State>>,
}

impl FakeServer {
    pub async fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State {
            expires_in: 900,
            ..State::default()
        }));
        let shared = state.clone();
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let state = shared.clone();
                tokio::spawn(async move { serve(stream, state).await });
            }
        });
        Self { url, state }
    }

    pub fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }
}

async fn serve(mut stream: TcpStream, state: Arc<Mutex<State>>) {
    let mut data = Vec::new();
    let mut buf = [0_u8; 4096];
    let head_end = loop {
        let n = stream.read(&mut buf).await.unwrap();
        if n == 0 {
            return;
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let head = String::from_utf8_lossy(&data[..head_end]).to_string();
    let mut lines = head.lines();
    let mut request_line = lines.next().unwrap().split(' ');
    let method = request_line.next().unwrap().to_owned();
    let target = request_line.next().unwrap().to_owned();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_owned());
        }
    }
    let length: usize = headers
        .get("content-length")
        .map_or(0, |v| v.parse().unwrap());
    while data.len() < head_end + length {
        let n = stream.read(&mut buf).await.unwrap();
        data.extend_from_slice(&buf[..n]);
    }
    let body: Value =
        serde_json::from_slice(&data[head_end..head_end + length]).unwrap_or(Value::Null);
    let url = url::Url::parse(&format!("http://fake{target}")).unwrap();
    let (status, extra, payload) = {
        let mut state = state.lock().unwrap();
        route(&mut state, &method, &url, &headers, &body)
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    stream.write_all(response.as_bytes()).await.unwrap();
    let _ = stream.shutdown().await;
}

fn error(status: &str, code: &str) -> (String, String, String) {
    (
        status.into(),
        String::new(),
        json!({"error": code, "error_description": code}).to_string(),
    )
}

fn ok(value: Value) -> (String, String, String) {
    ("200 OK".into(), String::new(), value.to_string())
}

fn route(
    state: &mut State,
    method: &str,
    url: &url::Url,
    headers: &HashMap<String, String>,
    body: &Value,
) -> (String, String, String) {
    let field = |key: &str| body[key].as_str().unwrap_or_default().to_owned();
    match (method, url.path()) {
        ("GET", "/oauth/authorize") => {
            let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
            let redirect = q["redirect_uri"].clone();
            assert!(redirect.starts_with("http://127.0.0.1:"), "{redirect}");
            assert!(q["client_id"].starts_with("ferrum-"));
            assert_eq!(q["code_challenge_method"], "S256");
            assert!(!q["device_name"].is_empty());
            let location = if state.deny {
                format!("{redirect}?error=access_denied&state={}", q["state"])
            } else {
                let code = state.token("fac_");
                state.codes.insert(
                    code.clone(),
                    Code {
                        client_id: q["client_id"].clone(),
                        redirect_uri: redirect.clone(),
                        challenge: q["code_challenge"].clone(),
                    },
                );
                format!("{redirect}?code={code}&state={}", q["state"])
            };
            (
                "302 Found".into(),
                format!("Location: {location}\r\n"),
                "{}".into(),
            )
        }
        ("POST", "/api/v1/oauth/token") => match field("grant_type").as_str() {
            "authorization_code" => {
                let Some(code) = state.codes.remove(&field("code")) else {
                    return error("400 Bad Request", "invalid_grant");
                };
                let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(field("code_verifier")));
                if challenge != code.challenge
                    || code.redirect_uri != field("redirect_uri")
                    || code.client_id != field("client_id")
                {
                    return error("400 Bad Request", "invalid_grant");
                }
                let app = code.client_id.trim_start_matches("ferrum-").to_owned();
                let ix = state.new_session(&app, None, &field("device_id"));
                ok(state.issue(ix))
            }
            "refresh_token" => {
                state.refresh_grants += 1;
                let presented = field("refresh_token");
                if let Some(ix) = state.live(&presented) {
                    let next = state.token("frt_");
                    let session = &mut state.sessions[ix];
                    session
                        .previous
                        .push(std::mem::replace(&mut session.refresh, next));
                    return ok(state.issue(ix));
                }
                if let Some(ix) = state
                    .sessions
                    .iter()
                    .position(|s| s.previous.contains(&presented))
                {
                    state.revoke(ix);
                }
                error("401 Unauthorized", "invalid_grant")
            }
            "exchange" => {
                let Some(parent) = state.live(&field("parent_refresh_token")) else {
                    return error("401 Unauthorized", "invalid_grant");
                };
                let app = field("client_id").trim_start_matches("ferrum-").to_owned();
                let ix = state.new_session(&app, Some(parent), &field("device_id"));
                ok(state.issue(ix))
            }
            _ => error("400 Bad Request", "invalid_request"),
        },
        ("POST", "/api/v1/oauth/revoke") => {
            if let Some(ix) = state.live(&field("refresh_token")) {
                state.revoke(ix);
            }
            ok(json!({}))
        }
        ("GET", "/api/v1/me") => {
            let token = headers
                .get("authorization")
                .and_then(|h| h.strip_prefix("Bearer "))
                .unwrap_or_default();
            match state.access.get(token) {
                Some(ix) if !state.sessions[*ix].revoked => ok(me()),
                _ => error("401 Unauthorized", "invalid_token"),
            }
        }
        _ => error("404 Not Found", "not_found"),
    }
}

/// Plays the browser: follows the authorize URL's redirect to the loopback
/// callback and returns the page the app served.
pub async fn browser(authorize_url: String) -> String {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = http.get(&authorize_url).send().await.unwrap();
    assert_eq!(response.status().as_u16(), 302);
    let location = response.headers()["location"].to_str().unwrap().to_owned();
    http.get(location)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap()
}
