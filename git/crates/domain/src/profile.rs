//! Profiles: a commit identity plus the SSH key used for network operations.
//! Each repository is assigned one profile (or none = system git config).

use crate::ids::ProfileId;
use serde::{Deserialize, Serialize};

/// How SSH authenticates for a profile.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SshAuth {
    /// Whatever `ssh` does by default: `~/.ssh/config`, ssh-agent, default keys.
    #[default]
    System,
    /// A specific private key, forced with `-i <path> -o IdentitiesOnly=yes`.
    KeyFile {
        path: String,
        /// A passphrase is stored in the OS keychain for this profile.
        #[serde(default)]
        has_passphrase: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub id: ProfileId,
    pub name: String,
    /// Author/committer name for commits made with this profile (empty = git config).
    pub author_name: String,
    /// Author/committer email (empty = git config).
    pub author_email: String,
    pub ssh: SshAuth,
}

impl Profile {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: ProfileId::new(),
            name: name.into(),
            ..Self::default()
        }
    }

    /// `Some((name, email))` when both are set, so commits override git config.
    #[must_use]
    pub fn identity(&self) -> Option<(&str, &str)> {
        let (n, e) = (self.author_name.trim(), self.author_email.trim());
        (!n.is_empty() && !e.is_empty()).then_some((n, e))
    }

    #[must_use]
    pub fn key_path(&self) -> Option<&str> {
        match &self.ssh {
            SshAuth::KeyFile { path, .. } if !path.trim().is_empty() => Some(path.trim()),
            _ => None,
        }
    }

    /// Short description of the SSH setting for lists.
    #[must_use]
    pub fn ssh_label(&self) -> String {
        match self.key_path() {
            Some(p) => p.rsplit(['/', '\\']).next().unwrap_or(p).to_owned(),
            None => "System SSH (agent / ~/.ssh/config)".to_owned(),
        }
    }
}
