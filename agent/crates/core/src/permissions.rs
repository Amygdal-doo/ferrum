//! Per-tool permission policy: reading is allowed, anything that writes,
//! runs or reaches out asks first. The user can allow a tool once, for the
//! session, or always (persisted in `~/.ferrum/agent/permissions.json`),
//! or deny it. An [`Approval`] level on top lets file edits (or
//! everything) through without asking.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::Access;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Policy {
    Ask,
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    AllowOnce,
    AllowSession,
    AllowAlways,
    Deny,
}

impl Decision {
    pub fn allowed(self) -> bool {
        self != Decision::Deny
    }
}

/// How much the agent may do without asking (the composer's approval
/// menu, saved in the settings).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Approval {
    /// Writes, commands and MCP tools ask (unless allowed before).
    #[default]
    Ask,
    /// File edits go through; commands and MCP tools still ask.
    AutoEdits,
    /// Nothing asks (a tool denied "always" stays denied).
    FullAccess,
}

impl Approval {
    pub const ALL: [Approval; 3] = [Approval::Ask, Approval::AutoEdits, Approval::FullAccess];

    pub fn label(self) -> &'static str {
        match self {
            Approval::Ask => "Ask for approval",
            Approval::AutoEdits => "Auto-accept edits",
            Approval::FullAccess => "Full access",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Approval::Ask => "Edits, commands and MCP tools ask first",
            Approval::AutoEdits => "Edits are applied for review; commands still ask",
            Approval::FullAccess => "Edits and commands run without asking",
        }
    }

    /// Whether this level lets `access` through without a prompt.
    pub fn allows(self, access: Access) -> bool {
        match self {
            Approval::Ask => false,
            Approval::AutoEdits => access == Access::Write,
            Approval::FullAccess => true,
        }
    }
}

/// What the UI is asked about.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub id: u64,
    /// The thread's tool call this is about.
    pub tool_call_id: String,
    pub tool: String,
    pub title: String,
    pub input: Value,
    pub access: Access,
}

/// Answers permission requests (the UI through a session, or a fixed
/// answer in tests).
#[async_trait::async_trait]
pub trait PermissionBroker: Send + Sync {
    async fn ask(&self, request: PermissionRequest) -> Decision;
}

/// Always gives the same answer.
pub struct FixedBroker(pub Decision);

#[async_trait::async_trait]
impl PermissionBroker for FixedBroker {
    async fn ask(&self, _: PermissionRequest) -> Decision {
        self.0
    }
}

#[derive(Debug, Clone, Default)]
pub struct Permissions {
    file: Option<PathBuf>,
    always: BTreeMap<String, Policy>,
    session: BTreeSet<String>,
    approval: Approval,
}

impl Permissions {
    /// Persistent choices from `file` (missing: none).
    pub fn load(file: &Path) -> Self {
        let always = std::fs::read_to_string(file)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self {
            file: Some(file.to_path_buf()),
            always,
            session: BTreeSet::new(),
            approval: Approval::default(),
        }
    }

    pub fn approval(&self) -> Approval {
        self.approval
    }

    pub fn set_approval(&mut self, approval: Approval) {
        self.approval = approval;
    }

    /// An "always" choice for the tool wins (a denial even over full
    /// access), then the approval level, then session answers; reading is
    /// always allowed.
    pub fn policy(&self, tool: &str, access: Access) -> Policy {
        let always = self.always.get(tool).copied();
        if always == Some(Policy::Deny) {
            return Policy::Deny;
        }
        if self.approval.allows(access) {
            return Policy::Allow;
        }
        if let Some(policy) = always {
            return policy;
        }
        if self.session.contains(tool) || matches!(access, Access::Read | Access::Plan) {
            return Policy::Allow;
        }
        Policy::Ask
    }

    /// Remembers an answer (session or always).
    pub fn record(&mut self, tool: &str, decision: Decision) {
        match decision {
            Decision::AllowSession => {
                self.session.insert(tool.to_string());
            }
            Decision::AllowAlways => self.set_always(tool, Some(Policy::Allow)),
            Decision::AllowOnce | Decision::Deny => {}
        }
    }

    /// Sets (or clears) a persistent policy and saves it.
    pub fn set_always(&mut self, tool: &str, policy: Option<Policy>) {
        match policy {
            Some(policy) => self.always.insert(tool.to_string(), policy),
            None => self.always.remove(tool),
        };
        self.save();
    }

    pub fn always(&self) -> &BTreeMap<String, Policy> {
        &self.always
    }

    fn save(&self) {
        let Some(file) = &self.file else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(&self.always)
            && let Err(e) = crate::fsutil::write_atomic(file, &json)
        {
            tracing::warn!("saving permissions: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_session_and_always() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("permissions.json");
        let mut p = Permissions::load(&file);
        assert_eq!(p.policy("read_file", Access::Read), Policy::Allow);
        assert_eq!(p.policy("edit_file", Access::Write), Policy::Ask);
        assert_eq!(p.policy("run_command", Access::Execute), Policy::Ask);
        p.record("edit_file", Decision::AllowOnce);
        assert_eq!(p.policy("edit_file", Access::Write), Policy::Ask);
        p.record("edit_file", Decision::AllowSession);
        assert_eq!(p.policy("edit_file", Access::Write), Policy::Allow);
        p.record("run_command", Decision::AllowAlways);
        p.set_always("read_file", Some(Policy::Deny));
        let reloaded = Permissions::load(&file);
        assert_eq!(
            reloaded.policy("run_command", Access::Execute),
            Policy::Allow
        );
        assert_eq!(reloaded.policy("read_file", Access::Read), Policy::Deny);
        assert_eq!(
            reloaded.policy("edit_file", Access::Write),
            Policy::Ask,
            "session answers are not saved"
        );
    }

    #[test]
    fn approval_levels() {
        let mut p = Permissions::default();
        p.set_always("mcp_rm", Some(Policy::Deny));
        let cases = [
            (
                Approval::Ask,
                [Policy::Allow, Policy::Ask, Policy::Ask, Policy::Ask],
            ),
            (
                Approval::AutoEdits,
                [Policy::Allow, Policy::Allow, Policy::Ask, Policy::Ask],
            ),
            (
                Approval::FullAccess,
                [Policy::Allow, Policy::Allow, Policy::Allow, Policy::Allow],
            ),
        ];
        for (approval, expected) in cases {
            p.set_approval(approval);
            assert_eq!(p.approval(), approval);
            let got = [
                p.policy("read_file", Access::Read),
                p.policy("edit_file", Access::Write),
                p.policy("run_command", Access::Execute),
                p.policy("mcp_tool", Access::External),
            ];
            assert_eq!(got, expected, "{approval:?}");
            assert_eq!(p.policy("mcp_rm", Access::External), Policy::Deny);
        }
        assert_eq!(Approval::default(), Approval::Ask);
        assert_eq!(
            serde_json::to_string(&Approval::AutoEdits).expect("json"),
            "\"auto_edits\""
        );
    }
}
