//! Tools the agent can call: built-ins scoped to the project root (see
//! [`sandbox`]) and tools from MCP servers. Writes go through the
//! session's [`ChangeSet`].

pub mod builtin;
pub mod runner;
pub mod sandbox;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::changeset::ChangeSet;
use crate::mcp::McpClient;
use crate::modes::Mode;
use crate::provider::ToolSpec;
pub use runner::{CommandOutput, CommandRunner, ProcessRunner};
pub use sandbox::{Sandbox, SandboxError};

/// What a tool does, for modes and permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Read,
    Write,
    Execute,
    /// An MCP server's tool: unknown effects.
    External,
    /// Plan mode's `write_plan`.
    Plan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolDef {
    pub spec: ToolSpec,
    pub access: Access,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
    /// Files written (relative), for the changes list.
    #[serde(default)]
    pub changed: Vec<PathBuf>,
}

impl ToolOutput {
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: true,
            changed: Vec::new(),
        }
    }
}

/// What tools run against.
#[derive(Clone)]
pub struct ToolContext {
    pub sandbox: Sandbox,
    pub changes: Arc<Mutex<ChangeSet>>,
    pub runner: Arc<dyn CommandRunner>,
}

struct McpTool {
    def: ToolDef,
    client: Arc<McpClient>,
    remote_name: String,
}

/// Built-ins plus MCP tools (named `mcp__<server>__<tool>`).
#[derive(Default)]
pub struct ToolRegistry {
    mcp: Vec<McpTool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a connected MCP server's tools.
    pub fn add_mcp(&mut self, server: &str, client: Arc<McpClient>, tools: Vec<ToolSpec>) {
        for tool in tools {
            let name = format!("mcp__{}__{}", sanitize(server), sanitize(&tool.name));
            self.mcp.push(McpTool {
                def: ToolDef {
                    spec: ToolSpec {
                        name,
                        description: tool.description,
                        input_schema: tool.input_schema,
                    },
                    access: Access::External,
                },
                client: client.clone(),
                remote_name: tool.name,
            });
        }
    }

    /// Every tool, regardless of mode.
    pub fn all(&self) -> Vec<ToolDef> {
        builtin::defs()
            .into_iter()
            .chain(self.mcp.iter().map(|t| t.def.clone()))
            .collect()
    }

    /// The tools `mode` offers the model.
    pub fn for_mode(&self, mode: Mode) -> Vec<ToolDef> {
        self.all()
            .into_iter()
            .filter(|t| mode.allows(t.access))
            .collect()
    }

    pub fn access(&self, name: &str) -> Option<Access> {
        self.all()
            .into_iter()
            .find(|t| t.spec.name == name)
            .map(|t| t.access)
    }

    pub async fn call(&self, ctx: &ToolContext, name: &str, input: Value) -> ToolOutput {
        if let Some(tool) = self.mcp.iter().find(|t| t.def.spec.name == name) {
            return match tool.client.call_tool(&tool.remote_name, input).await {
                Ok(output) => output,
                Err(e) => ToolOutput::error(e.to_string()),
            };
        }
        builtin::run(ctx, name, input).await
    }
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// A short card title for a tool call: "Read src/main.rs", "Run `ls`".
pub fn title(name: &str, input: &Value) -> String {
    let arg = |key: &str| input[key].as_str().unwrap_or_default().to_string();
    match name {
        "read_file" => format!("Read {}", arg("path")),
        "list_dir" => format!("List {}", input["path"].as_str().unwrap_or(".")),
        "grep" => format!("Search “{}”", arg("pattern")),
        "edit_file" => format!("Edit {}", arg("path")),
        "write_file" => format!("Write {}", arg("path")),
        "delete_file" => format!("Delete {}", arg("path")),
        "run_command" => format!("Run `{}`", arg("command")),
        "git_status" => "Git status".into(),
        "git_diff" => "Git diff".into(),
        "write_plan" => "Write plan".into(),
        other => match other.strip_prefix("mcp__") {
            Some(rest) => format!("MCP {}", rest.replacen("__", ": ", 1)),
            None => other.to_string(),
        },
    }
}
