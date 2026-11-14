//! Extensions: FerrumChat's MCP servers in `~/.ferrum/agent/mcp.json`.
//! Edited as plain JSON so keys this app doesn't know survive.

use std::path::Path;

use ferrumagent_ui::core::mcp::McpConfig;
use serde_json::{Map, Value, json};

/// A server as listed on the Extensions page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub name: String,
    /// The command line, or the URL of an HTTP server.
    pub detail: String,
    pub disabled: bool,
}

/// The servers in `file`, by name.
pub fn list(file: &Path) -> Result<Vec<Server>, String> {
    let config = McpConfig::load(file)?;
    Ok(config
        .servers
        .into_iter()
        .map(|(name, s)| {
            let detail = if s.is_http() {
                s.url.unwrap_or_default()
            } else {
                std::iter::once(s.command.unwrap_or_default())
                    .chain(s.args)
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            Server {
                name,
                detail,
                disabled: s.disabled,
            }
        })
        .collect())
}

fn read(file: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(file) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

/// Applies `f` to the `mcpServers` object and writes the file back.
fn edit(file: &Path, f: impl FnOnce(&mut Map<String, Value>)) -> Result<(), String> {
    let mut root = read(file)?;
    let Some(object) = root.as_object_mut() else {
        return Err(format!("{}: not a JSON object", file.display()));
    };
    let servers = object
        .entry("mcpServers")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| format!("{}: mcpServers is not an object", file.display()))?;
    f(servers);
    ferrum_ui::prefs::write(file, &root).map_err(|e| format!("{}: {e}", file.display()))
}

/// Adds `name`: an `http(s)://` address is an HTTP server, anything else a
/// command line (split on spaces).
pub fn add(file: &Path, name: &str, command: &str) -> Result<(), String> {
    let name = name.trim();
    let command = command.trim();
    if name.is_empty() || command.is_empty() {
        return Err("A name and a command or URL are needed.".into());
    }
    let entry = if command.starts_with("http://") || command.starts_with("https://") {
        json!({ "type": "http", "url": command })
    } else {
        let mut parts = command.split_whitespace().map(str::to_string);
        let program = parts.next().unwrap_or_default();
        json!({ "command": program, "args": parts.collect::<Vec<_>>() })
    };
    edit(file, |servers| {
        servers.insert(name.to_string(), entry);
    })
}

pub fn set_disabled(file: &Path, name: &str, disabled: bool) -> Result<(), String> {
    edit(file, |servers| {
        if let Some(Value::Object(server)) = servers.get_mut(name) {
            if disabled {
                server.insert("disabled".into(), Value::Bool(true));
            } else {
                server.remove("disabled");
            }
        }
    })
}

pub fn remove(file: &Path, name: &str) -> Result<(), String> {
    edit(file, |servers| {
        servers.remove(name);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servers_are_added_disabled_and_removed_keeping_other_keys() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("mcp.json");
        assert!(list(&file).expect("empty").is_empty());
        std::fs::write(
            &file,
            r#"{"mcpServers":{"fs":{"command":"npx","args":["-y","fs"],"env":{"A":"1"}}},"other":1}"#,
        )
        .expect("write");
        add(&file, "docs", "https://mcp.example.com/mcp").expect("add http");
        add(&file, "git", "uvx mcp-git --repo .").expect("add stdio");
        assert!(add(&file, " ", "x").is_err());
        set_disabled(&file, "fs", true).expect("disable");

        let servers = list(&file).expect("list");
        let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["docs", "fs", "git"]);
        assert_eq!(servers[0].detail, "https://mcp.example.com/mcp");
        assert_eq!(servers[2].detail, "uvx mcp-git --repo .");
        assert!(servers[1].disabled && !servers[2].disabled);

        set_disabled(&file, "fs", false).expect("enable");
        remove(&file, "docs").expect("remove");
        let root: Value =
            serde_json::from_str(&std::fs::read_to_string(&file).expect("read")).expect("json");
        assert_eq!(root["other"], 1);
        assert_eq!(root["mcpServers"]["fs"]["env"]["A"], "1");
        assert!(root["mcpServers"]["fs"].get("disabled").is_none());
        assert!(root["mcpServers"].get("docs").is_none());
    }
}
