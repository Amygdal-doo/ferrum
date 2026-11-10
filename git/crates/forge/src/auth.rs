//! Token onboarding text and the `gh` CLI token source. Tokens are passed
//! around as [`Secret`] so they never reach `Debug` output or logs.

use crate::error::scope_hint;
use crate::remote::ForgeRepo;
use ferrumgit_domain::{ForgeKind, Secret};

/// What to show when a host has no token yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenHelp {
    /// `repo` / `api`.
    pub scopes: &'static str,
    /// Token creation page, pre-filled where the forge supports it.
    pub create_url: String,
    pub text: String,
}

/// Onboarding text for `repo`'s host.
#[must_use]
pub fn token_help(repo: &ForgeRepo) -> TokenHelp {
    let scheme = if repo.web_url.starts_with("http://") {
        "http"
    } else {
        "https"
    };
    let host = &repo.host;
    match repo.kind {
        ForgeKind::GitHub => TokenHelp {
            scopes: "repo",
            create_url: format!(
                "{scheme}://{host}/settings/tokens/new?scopes=repo&description=FerrumGIT"
            ),
            text: format!(
                "FerrumGIT talks to {host} with a personal access token. {} \
                 The token is saved in the OS keychain (never on disk or in logs). \
                 If you use the GitHub CLI, you can use its login instead (`gh auth token --hostname {host}`).",
                scope_hint(ForgeKind::GitHub)
            ),
        },
        ForgeKind::GitLab => TokenHelp {
            scopes: "api",
            create_url: format!(
                "{scheme}://{host}/-/user_settings/personal_access_tokens?name=FerrumGIT&scopes=api"
            ),
            text: format!(
                "FerrumGIT talks to {host} with a personal access token. {} \
                 The token is saved in the OS keychain (never on disk or in logs).",
                scope_hint(ForgeKind::GitLab)
            ),
        },
    }
}

/// Arguments for asking the GitHub CLI for its token on `host`.
#[must_use]
pub fn gh_token_args(host: &str) -> [String; 4] {
    [
        "auth".into(),
        "token".into(),
        "--hostname".into(),
        host.to_owned(),
    ]
}

/// `gh auth token --hostname <host>`; `None` when gh is missing or logged out.
/// Blocks: call it from a blocking thread.
#[must_use]
pub fn gh_cli_token(host: &str) -> Option<Secret> {
    let output = std::process::Command::new("gh")
        .args(gh_token_args(host))
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8(output.stdout).ok()?;
    let token = token.trim();
    (!token.is_empty()).then(|| Secret::new(token))
}

/// Trims a pasted token; rejects empty or multi-line input.
pub fn clean_token(input: &str) -> Result<String, &'static str> {
    let t = input.trim();
    if t.is_empty() {
        return Err("Paste a token first.");
    }
    if t.contains(char::is_whitespace) {
        return Err("A token cannot contain spaces or line breaks.");
    }
    Ok(t.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::resolve;
    use ferrumgit_domain::RepoForge;

    fn repo(url: &str, kind: Option<ForgeKind>) -> ForgeRepo {
        resolve(
            &[("origin".into(), url.into())],
            &RepoForge {
                kind,
                ..RepoForge::default()
            },
        )
        .expect("repo")
    }

    #[test]
    fn onboarding_lists_scopes_and_urls() {
        let h = token_help(&repo("git@github.com:o/r.git", None));
        assert_eq!(h.scopes, "repo");
        assert!(
            h.create_url
                .starts_with("https://github.com/settings/tokens/new?scopes=repo")
        );
        assert!(
            h.text.contains("`repo`") && h.text.contains("gh auth token --hostname github.com")
        );
        let h = token_help(&repo(
            "https://git.corp:8443/g/p.git",
            Some(ForgeKind::GitLab),
        ));
        assert_eq!(h.scopes, "api");
        assert!(
            h.create_url
                .starts_with("https://git.corp:8443/-/user_settings/personal_access_tokens")
        );
        assert!(h.text.contains("`api`"));
        assert_eq!(gh_token_args("ghe.corp")[3], "ghe.corp");
    }

    #[test]
    fn cleans_pasted_tokens() {
        assert_eq!(clean_token("  ghp_abc \n").as_deref(), Ok("ghp_abc"));
        assert!(clean_token("   ").is_err());
        assert!(clean_token("a b").is_err());
    }
}
