//! Where FerrumChat keeps its files: `~/.ferrum/agent/` (overridable with
//! `FERRUM_AGENT_HOME`). Nothing secret is ever written there.

use std::path::{Path, PathBuf};

/// `~/.ferrum/agent`, or `$FERRUM_AGENT_HOME`.
pub fn agent_home() -> PathBuf {
    if let Some(dir) = std::env::var_os("FERRUM_AGENT_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_else(std::env::temp_dir)
        .join(".ferrum")
        .join("agent")
}

pub fn mcp_config(home: &Path) -> PathBuf {
    home.join("mcp.json")
}

pub fn settings_file(home: &Path) -> PathBuf {
    home.join("settings.json")
}

pub fn agents_file(home: &Path) -> PathBuf {
    home.join("agents.json")
}

pub fn permissions_file(home: &Path) -> PathBuf {
    home.join("permissions.json")
}

/// Per-project thread folder: `threads/<name>-<hash of the path>`.
pub fn threads_dir(home: &Path, project: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(project.to_string_lossy().as_bytes());
    let hash: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    let name: String = project
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    home.join("threads").join(format!("{name}-{hash}"))
}
