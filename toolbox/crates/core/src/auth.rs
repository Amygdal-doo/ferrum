//! GitHub token lookup. The token never touches `state.json` or the logs.
//!
//! Order: OS keychain (service `FerrumToolbox`), `GITHUB_TOKEN`, then
//! `gh auth token`. Keychain and `gh` calls block, so run them off the UI thread.

use crate::{Result, ToolboxError};
use std::fmt;

const KEYCHAIN_SERVICE: &str = "FerrumToolbox";
const KEYCHAIN_ACCOUNT: &str = "github-token";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
    Keychain,
    Environment,
    GhCli,
}

impl fmt::Display for TokenSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TokenSource::Keychain => "keychain",
            TokenSource::Environment => "GITHUB_TOKEN",
            TokenSource::GhCli => "gh CLI",
        })
    }
}

/// A GitHub token whose `Debug` output is redacted.
#[derive(Clone, PartialEq, Eq)]
pub struct GitHubToken {
    value: String,
    pub source: TokenSource,
}

impl GitHubToken {
    #[must_use]
    pub fn new(value: impl Into<String>, source: TokenSource) -> Self {
        Self {
            value: value.into(),
            source,
        }
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for GitHubToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GitHubToken(<redacted>, {})", self.source)
    }
}

fn keychain_entry() -> Result<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|e| ToolboxError::Credentials(e.to_string()))
}

/// Finds a token; `None` means anonymous access (public repos only).
#[must_use]
pub fn resolve_token() -> Option<GitHubToken> {
    keychain_token().or_else(env_token).or_else(gh_cli_token)
}

fn keychain_token() -> Option<GitHubToken> {
    let entry = keychain_entry().ok()?;
    match entry.get_password() {
        Ok(token) if !token.trim().is_empty() => {
            Some(GitHubToken::new(token.trim(), TokenSource::Keychain))
        }
        Ok(_) | Err(keyring::Error::NoEntry) => None,
        Err(e) => {
            tracing::warn!(error = %e, "could not read the GitHub token from the keychain");
            None
        }
    }
}

fn env_token() -> Option<GitHubToken> {
    std::env::var("GITHUB_TOKEN")
        .ok()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
        .map(|token| GitHubToken::new(token, TokenSource::Environment))
}

fn gh_cli_token() -> Option<GitHubToken> {
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!token.is_empty()).then(|| GitHubToken::new(token, TokenSource::GhCli))
}

/// Saves `token` in the OS keychain.
pub fn save_token(token: &str) -> Result<()> {
    let token = token.trim();
    if token.is_empty() {
        return Err(ToolboxError::Invalid("the token is empty".into()));
    }
    keychain_entry()?
        .set_password(token)
        .map_err(|e| ToolboxError::Credentials(e.to_string()))
}

/// Removes the keychain token (missing is fine).
pub fn clear_token() -> Result<()> {
    match keychain_entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(ToolboxError::Credentials(e.to_string())),
    }
}

/// Whether `gh auth token` yields a token (without returning it).
#[must_use]
pub fn gh_cli_signed_in() -> bool {
    gh_cli_token().is_some()
}

/// Where a token would come from, without returning it.
#[must_use]
pub fn token_source() -> Option<TokenSource> {
    resolve_token().map(|token| token.source)
}

/// Fine-grained token page, pre-filled where GitHub supports it (unknown
/// parameters are ignored).
#[must_use]
pub fn new_token_url(owner: &str) -> String {
    format!(
        "https://github.com/settings/personal-access-tokens/new?name=FerrumToolbox&description=Read%20Ferrum%20releases&target_name={owner}&contents=read"
    )
}

/// Whether a token is stored in the keychain (without returning it).
#[must_use]
pub fn has_keychain_token() -> bool {
    keychain_token().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_is_redacted() {
        let token = GitHubToken::new("ghp_supersecret", TokenSource::Environment);
        let shown = format!("{token:?}");
        assert!(!shown.contains("supersecret"));
        assert!(shown.contains("redacted"));
    }
}
