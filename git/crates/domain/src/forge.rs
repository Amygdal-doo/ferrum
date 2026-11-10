//! Forge (GitHub / GitLab) settings persisted with repositories. Tokens are
//! never stored here: they live in the OS keychain or come from the `gh` CLI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ForgeKind {
    #[serde(rename = "github")]
    GitHub,
    #[serde(rename = "gitlab")]
    GitLab,
}

impl ForgeKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::GitLab => "GitLab",
        }
    }

    /// "Pull request" or "Merge request".
    #[must_use]
    pub const fn pr_noun(self) -> &'static str {
        match self {
            Self::GitHub => "pull request",
            Self::GitLab => "merge request",
        }
    }

    /// `#12` or `!12`.
    #[must_use]
    pub const fn number_prefix(self) -> char {
        match self {
            Self::GitHub => '#',
            Self::GitLab => '!',
        }
    }
}

/// Per-repository forge overrides. Empty values mean "detect from the remote".
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RepoForge {
    /// Remote whose URL identifies the project (empty = `origin`, else the first remote).
    pub remote: String,
    /// Forge type for hosts that cannot be recognised by name.
    pub kind: Option<ForgeKind>,
    /// API base URL, e.g. `https://ghe.example.com/api/v3` or `https://git.example.com/api/v4`.
    pub api_url: String,
}

/// Where the token for a forge host comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenSource {
    /// A personal access token saved in the OS keychain.
    #[default]
    Keychain,
    /// `gh auth token --hostname <host>` (GitHub only), read on demand and never stored.
    GhCli,
}

/// Authentication preference for one forge host (`github.com`, `gitlab.example.com`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ForgeHost {
    pub host: String,
    pub token_source: TokenSource,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_forge_defaults_when_missing() {
        let f: RepoForge = serde_json::from_str("{}").expect("parse");
        assert_eq!(f, RepoForge::default());
        let f: RepoForge =
            serde_json::from_str(r#"{"kind":"gitlab","api_url":"https://x/api/v4"}"#)
                .expect("parse");
        assert_eq!(f.kind, Some(ForgeKind::GitLab));
    }
}
