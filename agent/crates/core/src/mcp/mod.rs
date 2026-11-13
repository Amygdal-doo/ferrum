//! Model Context Protocol client: connects to the servers in
//! `~/.ferrum/agent/mcp.json` (the `mcpServers` shape Claude and Cursor
//! use) over stdio or streamable HTTP, lists their tools and calls them.
//! `${VAR}` in `env`, `headers` and `url` is taken from the environment so
//! tokens need not be written to the file.
//!
//! Project-level MCP configs are deliberately not loaded: a cloned
//! repository must not be able to start processes.

mod http;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::jsonrpc::{BasicHandler, RpcError, RpcProcess};
use crate::provider::ToolSpec;
use crate::tools::ToolOutput;
pub use http::HttpTransport;

/// Protocol revision we ask for (servers may answer an older one).
pub const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerConfig {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// `stdio`, `http` / `streamable-http` (inferred when missing).
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub disabled: bool,
}

impl McpServerConfig {
    pub fn is_http(&self) -> bool {
        match self.kind.as_deref() {
            Some("http" | "streamable-http" | "streamableHttp" | "sse") => true,
            Some(_) => false,
            None => self.command.is_none() && self.url.is_some(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(rename = "mcpServers", default)]
    pub servers: BTreeMap<String, McpServerConfig>,
}

impl McpConfig {
    /// The config at `file` (missing or unreadable: no servers).
    pub fn load(file: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(file) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", file.display())),
        }
    }

    pub fn enabled(&self) -> impl Iterator<Item = (&String, &McpServerConfig)> {
        self.servers.iter().filter(|(_, s)| !s.disabled)
    }

    /// The servers in ACP's `session/new` shape.
    pub fn for_acp(&self, http_supported: bool) -> Vec<Value> {
        self.enabled()
            .filter_map(|(name, s)| {
                let pairs = |map: &BTreeMap<String, String>| -> Vec<Value> {
                    map.iter()
                        .map(|(k, v)| json!({"name": k, "value": expand(v)}))
                        .collect()
                };
                if s.is_http() {
                    http_supported.then(|| {
                        json!({"type": "http", "name": name, "url": expand(s.url.as_deref().unwrap_or_default()), "headers": pairs(&s.headers)})
                    })
                } else {
                    s.command.as_ref().map(|command| {
                        json!({"name": name, "command": command, "args": s.args, "env": pairs(&s.env)})
                    })
                }
            })
            .collect()
    }
}

/// `${VAR}` / `${VAR:-default}` from the environment.
pub fn expand(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let inner = &rest[start + 2..start + end];
        let (name, default) = inner.split_once(":-").unwrap_or((inner, ""));
        out.push_str(&std::env::var(name).unwrap_or_else(|_| default.to_string()));
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum McpError {
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    Transport(String),
    #[error("{0}")]
    Rpc(#[from] RpcError),
}

enum Transport {
    Stdio(RpcProcess),
    Http(HttpTransport),
}

pub struct McpClient {
    pub name: String,
    transport: Transport,
    pub server_info: Value,
}

impl McpClient {
    /// Starts (stdio) or reaches (HTTP) a server and initializes it.
    pub async fn connect(
        name: &str,
        config: &McpServerConfig,
        cwd: &Path,
    ) -> Result<Arc<Self>, McpError> {
        let transport = if config.is_http() {
            let url = config
                .url
                .as_deref()
                .ok_or_else(|| McpError::Config(format!("{name}: no url")))?;
            let headers = config
                .headers
                .iter()
                .map(|(k, v)| (k.clone(), expand(v)))
                .collect();
            Transport::Http(HttpTransport::new(&expand(url), headers))
        } else {
            let command = config
                .command
                .as_deref()
                .ok_or_else(|| McpError::Config(format!("{name}: no command")))?;
            let env = config
                .env
                .iter()
                .map(|(k, v)| (k.clone(), expand(v)))
                .collect();
            Transport::Stdio(
                RpcProcess::spawn(command, &config.args, &env, cwd, Arc::new(BasicHandler))
                    .map_err(McpError::Transport)?,
            )
        };
        let mut client = Self {
            name: name.to_string(),
            transport,
            server_info: Value::Null,
        };
        let init = client
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "ferrum-chat", "version": env!("CARGO_PKG_VERSION")},
                }),
            )
            .await
            .map_err(|e| client.with_stderr(e))?;
        if let Transport::Http(http) = &client.transport
            && let Some(version) = init["protocolVersion"].as_str()
        {
            http.set_protocol_version(version);
        }
        client.server_info = init["serverInfo"].clone();
        client
            .notify("notifications/initialized", json!({}))
            .await?;
        Ok(Arc::new(client))
    }

    fn with_stderr(&self, error: McpError) -> McpError {
        match &self.transport {
            Transport::Stdio(process) if !process.stderr_tail().is_empty() => {
                McpError::Transport(format!("{error}\n{}", process.stderr_tail()))
            }
            _ => error,
        }
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, McpError> {
        match &self.transport {
            Transport::Stdio(process) => Ok(process.connection.request(method, params).await?),
            Transport::Http(http) => http.request(method, params).await,
        }
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), McpError> {
        match &self.transport {
            Transport::Stdio(process) => Ok(process.connection.notify(method, params).await?),
            Transport::Http(http) => http.notify(method, params).await,
        }
    }

    /// Every tool (following `nextCursor`).
    pub async fn list_tools(&self) -> Result<Vec<ToolSpec>, McpError> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..20 {
            let params = match &cursor {
                Some(c) => json!({"cursor": c}),
                None => json!({}),
            };
            let page = self.request("tools/list", params).await?;
            for tool in page["tools"].as_array().into_iter().flatten() {
                let Some(name) = tool["name"].as_str() else {
                    continue;
                };
                tools.push(ToolSpec {
                    name: name.to_string(),
                    description: tool["description"].as_str().unwrap_or_default().to_string(),
                    input_schema: tool
                        .get("inputSchema")
                        .cloned()
                        .unwrap_or_else(|| json!({"type": "object"})),
                });
            }
            cursor = page["nextCursor"].as_str().map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok(tools)
    }

    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<ToolOutput, McpError> {
        let result = self
            .request("tools/call", json!({"name": name, "arguments": arguments}))
            .await?;
        let mut text = String::new();
        for item in result["content"].as_array().into_iter().flatten() {
            match item["type"].as_str() {
                Some("text") => text.push_str(item["text"].as_str().unwrap_or_default()),
                Some("resource") => text.push_str(
                    item["resource"]["text"]
                        .as_str()
                        .unwrap_or("[binary resource]"),
                ),
                Some(other) => text.push_str(&format!("[{other} content]")),
                None => {}
            }
            text.push('\n');
        }
        if text.trim().is_empty()
            && let Some(structured) = result.get("structuredContent")
        {
            text = structured.to_string();
        }
        Ok(ToolOutput {
            text: text.trim_end().to_string(),
            is_error: result["isError"].as_bool().unwrap_or(false),
            changed: Vec::new(),
        })
    }

    pub async fn shutdown(&self) {
        if let Transport::Stdio(process) = &self.transport {
            process.kill().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_shapes_and_expansion() {
        let config: McpConfig = serde_json::from_str(
            r#"{"mcpServers": {
                "fs": {"command": "npx", "args": ["-y", "server"], "env": {"TOKEN": "${FERRUM_TEST_MCP_TOKEN:-none}"}},
                "remote": {"url": "https://example.com/mcp", "headers": {"Authorization": "Bearer x"}},
                "off": {"command": "x", "disabled": true}
            }}"#,
        )
        .expect("parse");
        assert_eq!(config.enabled().count(), 2);
        assert!(config.servers["remote"].is_http() && !config.servers["fs"].is_http());
        let acp = config.for_acp(true);
        assert_eq!(acp.len(), 2);
        assert_eq!(acp[0]["env"][0], json!({"name": "TOKEN", "value": "none"}));
        assert_eq!(acp[1]["type"], "http");
        assert_eq!(config.for_acp(false).len(), 1);
        assert_eq!(expand("a${FERRUM_TEST_SURELY_UNSET}b${X:-d}"), "abd");
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            McpConfig::load(&dir.path().join("missing.json")),
            Ok(McpConfig::default())
        );
    }
}
