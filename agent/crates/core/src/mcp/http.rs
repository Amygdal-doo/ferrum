//! MCP's streamable HTTP transport: each message is a POST; the answer is
//! JSON or an SSE stream carrying it. The server's `Mcp-Session-Id` is
//! sent back on later requests.

use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::{AtomicU64, Ordering};

use futures::StreamExt;
use serde_json::{Value, json};

use super::McpError;
use crate::jsonrpc::RpcError;
use crate::provider::sse;

pub struct HttpTransport {
    url: String,
    headers: Vec<(String, String)>,
    http: reqwest::Client,
    session: Mutex<Option<String>>,
    protocol: Mutex<Option<String>>,
    next_id: AtomicU64,
}

impl HttpTransport {
    pub fn new(url: &str, headers: Vec<(String, String)>) -> Self {
        Self {
            url: url.to_string(),
            headers,
            http: crate::provider::http_client(),
            session: Mutex::new(None),
            protocol: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    pub fn set_protocol_version(&self, version: &str) {
        *self.protocol.lock().unwrap_or_else(PoisonError::into_inner) = Some(version.into());
    }

    fn post(&self, body: &Value) -> reqwest::RequestBuilder {
        let mut request = self
            .http
            .post(&self.url)
            .header("Accept", "application/json, text/event-stream")
            .json(body);
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }
        if let Some(session) = self
            .session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        {
            request = request.header("Mcp-Session-Id", session);
        }
        if let Some(version) = self
            .protocol
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        {
            request = request.header("MCP-Protocol-Version", version);
        }
        request
    }

    async fn send(&self, body: &Value) -> Result<reqwest::Response, McpError> {
        let response = self
            .post(body)
            .send()
            .await
            .map_err(|e| McpError::Transport(e.without_url().to_string()))?;
        if let Some(session) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
        {
            *self.session.lock().unwrap_or_else(PoisonError::into_inner) = Some(session.into());
        }
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(McpError::Transport(format!(
                "HTTP {status}: {}",
                text.chars().take(300).collect::<String>()
            )));
        }
        Ok(response)
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value, McpError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let response = self
            .send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await?;
        let is_sse = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|t| t.starts_with("text/event-stream"));
        if is_sse {
            let mut events = Box::pin(sse::events(response.bytes_stream().boxed()));
            while let Some(event) = events.next().await {
                let event = event.map_err(|e| McpError::Transport(e.to_string()))?;
                if let Ok(message) = serde_json::from_str::<Value>(&event.data)
                    && let Some(result) = answer(&message, id)
                {
                    return result;
                }
            }
            Err(McpError::Transport(
                "the stream ended without an answer".into(),
            ))
        } else {
            let body: Value = response
                .json()
                .await
                .map_err(|e| McpError::Transport(e.to_string()))?;
            let messages = match body {
                Value::Array(items) => items,
                single => vec![single],
            };
            messages
                .iter()
                .find_map(|m| answer(m, id))
                .unwrap_or_else(|| Err(McpError::Transport("no answer in the response".into())))
        }
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<(), McpError> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
            .await
            .map(|_| ())
    }
}

fn answer(message: &Value, id: u64) -> Option<Result<Value, McpError>> {
    if message.get("method").is_some() || message["id"].as_u64() != Some(id) {
        return None;
    }
    Some(match message.get("error").filter(|e| !e.is_null()) {
        Some(error) => Err(McpError::Rpc(RpcError::new(
            error["code"].as_i64().unwrap_or(-32603),
            error["message"].as_str().unwrap_or("error"),
        ))),
        None => Ok(message["result"].clone()),
    })
}
