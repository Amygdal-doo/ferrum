//! Finding a GitHub token without ever showing it: FerrumGIT's keychain
//! entry first, then the GitHub CLI, then `GH_TOKEN` / `GITHUB_TOKEN`.

use ferrumgit_credentials::CredentialStore;
use ferrumgit_domain::Secret;
use std::sync::Arc;

/// Where the token in use came from (shown in the UI instead of the token).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenOrigin {
    Keychain,
    GhCli,
    Env(&'static str),
}

impl TokenOrigin {
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Keychain => "Keychain (shared with FerrumGIT)".to_owned(),
            Self::GhCli => "GitHub CLI".to_owned(),
            Self::Env(name) => format!("${name}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FoundToken {
    pub token: Secret,
    pub origin: TokenOrigin,
}

/// Token lookup and storage. Blocking: call from a background thread.
pub trait TokenProvider: Send + Sync {
    fn find(&self, host: &str) -> Option<FoundToken>;
    /// Saves a pasted token for `host` (same keychain entry as FerrumGIT).
    fn save(&self, host: &str, token: &str) -> Result<(), String>;
}

type GhLookup = Arc<dyn Fn(&str) -> Option<Secret> + Send + Sync>;
type EnvLookup = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// The real sources, each replaceable for tests.
pub struct SystemTokens {
    store: Arc<dyn CredentialStore>,
    gh: GhLookup,
    env: EnvLookup,
}

impl SystemTokens {
    /// FerrumGIT's keychain service, `gh auth token`, and the process environment.
    #[must_use]
    pub fn new() -> Self {
        Self::with(
            ferrumgit_credentials::default_token_store(),
            Arc::new(ferrumgit_forge::auth::gh_cli_token),
            Arc::new(|name| std::env::var(name).ok()),
        )
    }

    #[must_use]
    pub fn with(store: Arc<dyn CredentialStore>, gh: GhLookup, env: EnvLookup) -> Self {
        Self { store, gh, env }
    }
}

impl Default for SystemTokens {
    fn default() -> Self {
        Self::new()
    }
}

/// Variables the GitHub CLI reads for `host`.
#[must_use]
pub fn env_names(host: &str) -> &'static [&'static str] {
    if host == "github.com" {
        &["GH_TOKEN", "GITHUB_TOKEN"]
    } else {
        &["GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN"]
    }
}

impl TokenProvider for SystemTokens {
    fn find(&self, host: &str) -> Option<FoundToken> {
        match self.store.get(host) {
            Ok(Some(t)) if !t.trim().is_empty() => {
                return Some(FoundToken {
                    token: Secret::new(t.trim()),
                    origin: TokenOrigin::Keychain,
                });
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "keychain lookup failed"),
        }
        if let Some(token) = (self.gh)(host) {
            return Some(FoundToken {
                token,
                origin: TokenOrigin::GhCli,
            });
        }
        env_names(host).iter().find_map(|name| {
            let value = (self.env)(name)?;
            let value = value.trim();
            (!value.is_empty()).then(|| FoundToken {
                token: Secret::new(value),
                origin: TokenOrigin::Env(name),
            })
        })
    }

    fn save(&self, host: &str, token: &str) -> Result<(), String> {
        let token = ferrumgit_forge::auth::clean_token(token)?;
        self.store
            .set(host, &token)
            .map_err(|e| format!("Could not save the token: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumgit_credentials::SessionStore;

    fn tokens(gh: Option<&'static str>, env: Option<(&'static str, &'static str)>) -> SystemTokens {
        SystemTokens::with(
            Arc::new(SessionStore::default()),
            Arc::new(move |_| gh.map(Secret::new)),
            Arc::new(move |name| env.filter(|(n, _)| *n == name).map(|(_, v)| v.to_owned())),
        )
    }

    #[test]
    fn keychain_then_gh_then_env() {
        let t = tokens(Some("gho_cli"), Some(("GITHUB_TOKEN", "ghp_env")));
        assert_eq!(
            t.find("github.com").map(|f| f.origin),
            Some(TokenOrigin::GhCli)
        );
        t.save("github.com", "  ghp_saved \n").expect("save");
        let found = t.find("github.com").expect("found");
        assert_eq!(found.origin, TokenOrigin::Keychain);
        assert_eq!(found.token.expose(), "ghp_saved");

        let t = tokens(None, Some(("GITHUB_TOKEN", "ghp_env")));
        let found = t.find("github.com").expect("env");
        assert_eq!(found.origin, TokenOrigin::Env("GITHUB_TOKEN"));
        assert_eq!(found.origin.label(), "$GITHUB_TOKEN");
        // github.com variables are not sent to Enterprise hosts.
        assert!(t.find("ghe.corp").is_none());
        assert!(tokens(None, None).find("github.com").is_none());
    }

    #[test]
    fn rejects_bad_pasted_tokens() {
        let t = tokens(None, None);
        assert!(t.save("github.com", "   ").is_err());
        assert!(t.save("github.com", "two words").is_err());
        assert!(t.find("github.com").is_none());
    }
}
