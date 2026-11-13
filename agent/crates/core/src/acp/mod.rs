//! Agent Client Protocol client (protocol version 1, JSON-RPC over the
//! agent's stdio): runs external agents — Claude Code
//! (`@agentclientprotocol/claude-agent-acp`, formerly
//! `@zed-industries/claude-code-acp`), Codex (`@zed-industries/codex-acp`),
//! Gemini CLI (`gemini --experimental-acp`) — in the same thread model as
//! the built-in agent.
//!
//! We are the client: we send `initialize`, `session/new` (or
//! `session/load`), `session/prompt`, `session/set_mode` and the
//! `session/cancel` notification; the agent streams `session/update`
//! notifications and calls us for `session/request_permission`,
//! `fs/read_text_file`, `fs/write_text_file` (routed through the change
//! set) and `terminal/*`. See [`bridge`].

pub mod bridge;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::jsonrpc::RpcProcess;
use crate::modes::Mode;
use crate::provider::StopReason;
pub use bridge::{AcpBridge, map_update};

pub const PROTOCOL_VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcpAgentConfig {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// Claude Code, Codex and Gemini CLI.
pub fn builtin_agents() -> Vec<AcpAgentConfig> {
    let npx = |id: &str, name: &str, package: &str| AcpAgentConfig {
        id: id.into(),
        name: name.into(),
        command: "npx".into(),
        args: vec!["-y".into(), package.into()],
        env: BTreeMap::new(),
    };
    vec![
        npx(
            "claude-code",
            "Claude Code",
            "@agentclientprotocol/claude-agent-acp",
        ),
        npx("codex", "Codex", "@zed-industries/codex-acp"),
        AcpAgentConfig {
            id: "gemini".into(),
            name: "Gemini CLI".into(),
            command: "gemini".into(),
            args: vec!["--experimental-acp".into()],
            env: BTreeMap::new(),
        },
    ]
}

#[derive(Debug, Deserialize)]
struct AgentEntry {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Option<Vec<String>>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    disabled: bool,
}

/// Built-ins merged with `agents.json` (`{"agent_servers": {id: {command,
/// args, env, name, disabled}}}`, Zed's shape; a bare map works too): an
/// entry with a built-in's id changes it, `disabled` hides it.
pub fn load_agents(file: &Path) -> Vec<AcpAgentConfig> {
    let mut agents = builtin_agents();
    let Ok(text) = std::fs::read_to_string(file) else {
        return agents;
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        tracing::warn!("{}: not valid JSON", file.display());
        return agents;
    };
    let map = value.get("agent_servers").cloned().unwrap_or(value);
    let Ok(entries) = serde_json::from_value::<BTreeMap<String, AgentEntry>>(map) else {
        return agents;
    };
    for (id, entry) in entries {
        if entry.disabled {
            agents.retain(|a| a.id != id);
            continue;
        }
        match agents.iter_mut().find(|a| a.id == id) {
            Some(agent) => {
                if let Some(command) = entry.command {
                    agent.command = command;
                }
                if let Some(args) = entry.args {
                    agent.args = args;
                }
                if let Some(name) = entry.name {
                    agent.name = name;
                }
                agent.env.extend(entry.env);
            }
            None => {
                if let Some(command) = entry.command {
                    agents.push(AcpAgentConfig {
                        name: entry.name.unwrap_or_else(|| id.clone()),
                        id,
                        command,
                        args: entry.args.unwrap_or_default(),
                        env: entry.env,
                    });
                }
            }
        }
    }
    agents
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcpMode {
    pub id: String,
    pub name: String,
}

pub struct AcpClient {
    pub agent: AcpAgentConfig,
    process: RpcProcess,
    pub session_id: String,
    pub agent_info: Value,
    pub load_session: bool,
    pub modes: Vec<AcpMode>,
    current_mode: Mutex<Option<String>>,
}

impl AcpClient {
    /// Starts the agent, initializes it and opens a session (loading
    /// `resume` when the agent supports it). `path_env` replaces `PATH`
    /// when given (GUI apps rarely have the login shell's).
    pub async fn start(
        agent: &AcpAgentConfig,
        cwd: &Path,
        bridge: Arc<AcpBridge>,
        mcp_servers: impl Fn(bool) -> Vec<Value>,
        resume: Option<&str>,
        path_env: Option<&str>,
    ) -> Result<Self, String> {
        let mut env = agent.env.clone();
        if let Some(path) = path_env {
            env.entry("PATH".into()).or_insert_with(|| path.into());
        }
        let command = match path_env {
            Some(path) => crate::acp::find_program(&agent.command, path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| agent.command.clone()),
            None => agent.command.clone(),
        };
        let process = RpcProcess::spawn(&command, &agent.args, &env, cwd, bridge.clone())?;
        let failed = |e: crate::jsonrpc::RpcError, process: &RpcProcess| {
            let tail = process.stderr_tail();
            if tail.is_empty() {
                format!("{}: {e}", agent.name)
            } else {
                format!("{}: {e}\n{tail}", agent.name)
            }
        };
        let init = process
            .connection
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "clientCapabilities": {
                        "fs": {"readTextFile": true, "writeTextFile": true},
                        "terminal": true,
                    },
                    "clientInfo": {"name": "ferrum-chat", "title": "FerrumChat", "version": env!("CARGO_PKG_VERSION")},
                }),
            )
            .await
            .map_err(|e| failed(e, &process))?;
        let caps = &init["agentCapabilities"];
        let load_session = caps["loadSession"].as_bool().unwrap_or(false);
        let http = caps["mcpCapabilities"]["http"].as_bool().unwrap_or(false);
        let cwd_text = cwd.to_string_lossy().to_string();
        let servers = mcp_servers(http);
        let (session_id, session) = match resume.filter(|_| load_session) {
            Some(id) => {
                let result = process
                    .connection
                    .request(
                        "session/load",
                        json!({"sessionId": id, "cwd": cwd_text, "mcpServers": servers}),
                    )
                    .await
                    .map_err(|e| failed(e, &process))?;
                (id.to_string(), result)
            }
            None => {
                let result = process
                    .connection
                    .request(
                        "session/new",
                        json!({"cwd": cwd_text, "mcpServers": servers}),
                    )
                    .await
                    .map_err(|e| failed(e, &process))?;
                let id = result["sessionId"]
                    .as_str()
                    .ok_or_else(|| format!("{}: session/new returned no sessionId", agent.name))?
                    .to_string();
                (id, result)
            }
        };
        bridge.set_session(&session_id);
        let modes = session["modes"]["availableModes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| {
                Some(AcpMode {
                    id: m["id"].as_str()?.into(),
                    name: m["name"].as_str().unwrap_or_default().into(),
                })
            })
            .collect();
        let current = session["modes"]["currentModeId"]
            .as_str()
            .map(str::to_string);
        Ok(Self {
            agent: agent.clone(),
            process,
            session_id,
            agent_info: init["agentInfo"].clone(),
            load_session,
            modes,
            current_mode: Mutex::new(current),
        })
    }

    pub fn is_alive(&self) -> bool {
        !self.process.connection.is_closed()
    }

    /// Sends a prompt (ACP content blocks) and waits for the turn to end.
    pub async fn prompt(&self, blocks: Vec<Value>) -> Result<StopReason, String> {
        let result = self
            .process
            .connection
            .request(
                "session/prompt",
                json!({"sessionId": self.session_id, "prompt": blocks}),
            )
            .await
            .map_err(|e| {
                let tail = self.process.stderr_tail();
                if tail.is_empty() {
                    e.to_string()
                } else {
                    format!("{e}\n{tail}")
                }
            })?;
        Ok(match result["stopReason"].as_str().unwrap_or("end_turn") {
            "max_tokens" | "max_turn_requests" => StopReason::MaxTokens,
            "refusal" => StopReason::Refusal,
            "cancelled" => StopReason::Cancelled,
            _ => StopReason::EndTurn,
        })
    }

    pub async fn cancel(&self) {
        let _ = self
            .process
            .connection
            .notify("session/cancel", json!({"sessionId": self.session_id}))
            .await;
    }

    pub fn current_mode(&self) -> Option<String> {
        self.current_mode
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The agent's mode for one of ours, when it has a matching one.
    pub fn mode_for(&self, mode: Mode) -> Option<&AcpMode> {
        let wanted: &[&str] = match mode {
            Mode::Plan => &["plan"],
            Mode::Ask => &["ask", "read-only", "readonly", "plan"],
            Mode::Agent => &["default", "code", "agent", "auto"],
        };
        wanted
            .iter()
            .find_map(|w| self.modes.iter().find(|m| m.id.eq_ignore_ascii_case(w)))
    }

    pub async fn set_mode(&self, mode_id: &str) -> Result<(), String> {
        self.process
            .connection
            .request(
                "session/set_mode",
                json!({"sessionId": self.session_id, "modeId": mode_id}),
            )
            .await
            .map_err(|e| e.to_string())?;
        *self
            .current_mode
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(mode_id.into());
        Ok(())
    }

    pub async fn shutdown(&self) {
        self.process.kill().await;
    }
}

/// `program` on `path_env` (as given when it has a separator).
pub fn find_program(program: &str, path_env: &str) -> Option<std::path::PathBuf> {
    let candidate = Path::new(program);
    if candidate.components().count() > 1 {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    std::env::split_paths(path_env)
        .map(|dir| dir.join(program))
        .find(|p| p.is_file())
}

/// Prompt content blocks: the text, then each attachment as an embedded
/// resource (or text when the agent lacks `embeddedContext`).
pub fn prompt_blocks(text: &str, attachments: &[crate::thread::Attachment]) -> Vec<Value> {
    let mut blocks = vec![json!({"type": "text", "text": text})];
    for attachment in attachments {
        blocks.push(json!({
            "type": "text",
            "text": format!("<context label=\"{}\">\n{}\n</context>", attachment.label, attachment.text),
        }));
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_merge_with_agents_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("agents.json");
        assert_eq!(load_agents(&file).len(), 3);
        std::fs::write(
            &file,
            r#"{"agent_servers": {
                "gemini": {"disabled": true},
                "claude-code": {"command": "claude-code-acp", "args": [], "env": {"X": "1"}},
                "opencode": {"name": "OpenCode", "command": "opencode", "args": ["acp"]}
            }}"#,
        )
        .expect("write");
        let agents = load_agents(&file);
        let ids: Vec<&str> = agents.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["claude-code", "codex", "opencode"]);
        assert_eq!(agents[0].command, "claude-code-acp");
        assert!(agents[0].args.is_empty());
        assert_eq!(agents[0].env["X"], "1");
        assert_eq!(agents[2].name, "OpenCode");
    }
}
