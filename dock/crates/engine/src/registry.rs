//! Registries: which one an image reference lives in, the key its
//! credentials are stored under (the same keys `docker login` uses), and
//! what the CLI's `config.json` says about where secrets go
//! (`credHelpers`, `credsStore`, and old inline `auths`).

use std::collections::BTreeMap;
use std::fmt;

/// The key Docker Hub credentials are stored under (`docker login` with no server).
pub const DOCKER_HUB: &str = "https://index.docker.io/v1/";

/// Credentials for one registry. `secret` is a password or token and is
/// never printed (`Debug` redacts it).
#[derive(Clone, PartialEq, Eq)]
pub struct RegistryAuth {
    /// The store key: [`DOCKER_HUB`] or `host[:port]`.
    pub server: String,
    /// `<token>` when `secret` is an identity token (what the CLI stores
    /// when the registry hands one back on login).
    pub username: String,
    pub secret: String,
}

/// The username credential helpers use for identity tokens.
pub const TOKEN_USERNAME: &str = "<token>";

impl RegistryAuth {
    #[must_use]
    pub fn is_identity_token(&self) -> bool {
        self.username == TOKEN_USERNAME
    }
}

impl fmt::Debug for RegistryAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegistryAuth")
            .field("server", &self.server)
            .field("username", &self.username)
            .field("secret", &"<redacted>")
            .finish()
    }
}

/// What the engine answered to `POST /auth`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuthOutcome {
    /// e.g. `Login Succeeded`.
    pub status: String,
    /// Some registries return a token to store instead of the password.
    pub identity_token: Option<String>,
}

/// The registry host of an image reference, as the store key
/// ([`DOCKER_HUB`] for `nginx`, `library/nginx`, `docker.io/…`).
/// The first path component is a host when it has a `.` or a `:`, or is `localhost`.
#[must_use]
pub fn server_of(reference: &str) -> String {
    let reference = reference.trim();
    match reference.split_once('/') {
        Some((first, _)) if first.contains('.') || first.contains(':') || first == "localhost" => {
            server_key(first)
        }
        _ => DOCKER_HUB.to_owned(),
    }
}

/// The store key for a registry typed by the user or found in a
/// reference: Docker Hub's aliases become [`DOCKER_HUB`]; others lose any
/// scheme, path and trailing slash (`https://ghcr.io/` → `ghcr.io`).
/// Empty input means Docker Hub, like `docker login`.
#[must_use]
pub fn server_key(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed == DOCKER_HUB {
        return DOCKER_HUB.to_owned();
    }
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let host = without_scheme
        .split('/')
        .next()
        .unwrap_or_default()
        .to_lowercase();
    match host.as_str() {
        ""
        | "docker.io"
        | "index.docker.io"
        | "registry-1.docker.io"
        | "registry.hub.docker.com" => DOCKER_HUB.to_owned(),
        _ => host,
    }
}

/// What the UI calls a store key.
#[must_use]
pub fn display_name(server: &str) -> String {
    if server == DOCKER_HUB {
        "Docker Hub".to_owned()
    } else {
        server.to_owned()
    }
}

/// Whether `server` is a plausible registry host (`host[:port]`, no spaces).
#[must_use]
pub fn valid_server(server: &str) -> bool {
    if server == DOCKER_HUB {
        return true;
    }
    !server.is_empty()
        && server
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_' | '[' | ']'))
}

/// An inline `auths` entry of `config.json` (written by `docker login`
/// when no credential store is configured). Read, never written.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InlineAuth {
    /// base64 of `user:password`.
    pub auth: Option<String>,
    pub identity_token: Option<String>,
}

/// The parts of the CLI's `config.json` that decide where credentials live.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DockerConfig {
    /// `credsStore`: the helper for every registry (`desktop`, `osxkeychain`…).
    pub creds_store: Option<String>,
    /// `credHelpers`: per-registry helpers, by store key.
    pub cred_helpers: BTreeMap<String, String>,
    /// `auths`, by store key (empty objects when a store holds the secret).
    pub auths: BTreeMap<String, InlineAuth>,
}

impl DockerConfig {
    /// Parses `config.json`; a missing or broken file is an empty config.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
            return Self::default();
        };
        let string = |v: &serde_json::Value| {
            v.as_str()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
        };
        let creds_store = value.get("credsStore").and_then(string);
        let cred_helpers = value
            .get("credHelpers")
            .and_then(serde_json::Value::as_object)
            .map(|map| {
                map.iter()
                    .filter_map(|(server, helper)| Some((server_key(server), string(helper)?)))
                    .collect()
            })
            .unwrap_or_default();
        let auths = value
            .get("auths")
            .and_then(serde_json::Value::as_object)
            .map(|map| {
                map.iter()
                    .map(|(server, entry)| {
                        let auth = InlineAuth {
                            auth: entry.get("auth").and_then(string),
                            identity_token: entry.get("identitytoken").and_then(string),
                        };
                        (server_key(server), auth)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            creds_store,
            cred_helpers,
            auths,
        }
    }

    /// Credentials written inline by an older `docker login`, if any.
    #[must_use]
    pub fn inline_auth(&self, server: &str) -> Option<RegistryAuth> {
        let entry = self.auths.get(server)?;
        if let Some(token) = &entry.identity_token {
            return Some(RegistryAuth {
                server: server.to_owned(),
                username: TOKEN_USERNAME.to_owned(),
                secret: token.clone(),
            });
        }
        use base64::Engine as _;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(entry.auth.as_deref()?.trim())
            .ok()?;
        let text = String::from_utf8(decoded).ok()?;
        let (username, secret) = text.split_once(':')?;
        Some(RegistryAuth {
            server: server.to_owned(),
            username: username.to_owned(),
            secret: secret.to_owned(),
        })
    }
}

/// Where the secret for a registry is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretStore {
    /// `docker-credential-<name>` (`desktop`, `osxkeychain`, `pass`…), as `docker login` uses.
    Helper(String),
    /// The OS keychain directly, when no helper is installed. The `docker`
    /// CLI does not see these; FerrumContainers' pulls do.
    Keychain,
}

impl SecretStore {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Helper(name) => format!("docker-credential-{name}"),
            Self::Keychain => "OS keychain".to_owned(),
        }
    }
}

/// The helper the CLI falls back to when `config.json` names none.
#[must_use]
pub fn platform_helper() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["osxkeychain"]
    } else if cfg!(target_os = "windows") {
        &["wincred"]
    } else {
        &["secretservice", "pass"]
    }
}

/// The store for `server`, decided like the CLI: `credHelpers[server]`,
/// else `credsStore`, else the platform's helper when it is installed,
/// else the OS keychain. Never a plain file.
#[must_use]
pub fn store_for(
    config: &DockerConfig,
    server: &str,
    helper_installed: impl Fn(&str) -> bool,
) -> SecretStore {
    if let Some(helper) = config.cred_helpers.get(server) {
        return SecretStore::Helper(helper.clone());
    }
    if let Some(store) = &config.creds_store {
        return SecretStore::Helper(store.clone());
    }
    platform_helper()
        .iter()
        .find(|name| helper_installed(name))
        .map_or(SecretStore::Keychain, |name| {
            SecretStore::Helper((*name).to_owned())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_registry_of_a_reference() {
        for (reference, server) in [
            ("nginx", DOCKER_HUB),
            ("library/nginx:1.27", DOCKER_HUB),
            ("docker.io/me/app", DOCKER_HUB),
            ("ghcr.io/org/app:1.2", "ghcr.io"),
            ("localhost:5000/app", "localhost:5000"),
            ("localhost/app", "localhost"),
            (
                "123.dkr.ecr.eu-west-1.amazonaws.com/api@sha256:abc",
                "123.dkr.ecr.eu-west-1.amazonaws.com",
            ),
        ] {
            assert_eq!(server_of(reference), server, "{reference}");
        }
    }

    #[test]
    fn normalizes_typed_servers() {
        assert_eq!(server_key(""), DOCKER_HUB);
        assert_eq!(server_key("docker.io"), DOCKER_HUB);
        assert_eq!(server_key("https://index.docker.io/v1/"), DOCKER_HUB);
        assert_eq!(server_key("https://GHCR.io/"), "ghcr.io");
        assert_eq!(
            server_key(" registry.local:5000/v2 "),
            "registry.local:5000"
        );
        assert_eq!(display_name(DOCKER_HUB), "Docker Hub");
        assert_eq!(display_name("ghcr.io"), "ghcr.io");
        assert!(valid_server("ghcr.io") && valid_server(DOCKER_HUB));
        assert!(!valid_server("") && !valid_server("a b"));
    }

    #[test]
    fn redacts_secrets_in_debug_output() {
        let auth = RegistryAuth {
            server: "ghcr.io".into(),
            username: "me".into(),
            secret: "hunter2".into(),
        };
        let printed = format!("{auth:?}");
        assert!(printed.contains("ghcr.io") && !printed.contains("hunter2"));
    }

    #[test]
    fn reads_the_cli_config() {
        let config = DockerConfig::parse(
            r#"{"auths":{"https://index.docker.io/v1/":{},
                "old.example.com":{"auth":"bWU6czNjcmV0"},
                "tok.example.com":{"identitytoken":"tkn"}},
               "credsStore":"desktop",
               "credHelpers":{"123.dkr.ecr.us-east-1.amazonaws.com":"ecr-login"}}"#,
        );
        assert_eq!(config.creds_store.as_deref(), Some("desktop"));
        assert_eq!(config.auths.len(), 3);
        assert_eq!(config.inline_auth(DOCKER_HUB), None, "an empty entry");
        let old = config.inline_auth("old.example.com");
        assert_eq!(
            old.map(|a| (a.username, a.secret)),
            Some(("me".into(), "s3cret".into()))
        );
        assert!(
            config
                .inline_auth("tok.example.com")
                .is_some_and(|a| a.is_identity_token() && a.secret == "tkn")
        );
        assert_eq!(DockerConfig::parse("not json"), DockerConfig::default());
    }

    #[test]
    fn picks_the_store_like_the_cli() {
        let config = DockerConfig::parse(
            r#"{"credsStore":"desktop","credHelpers":{"ecr.example.com":"ecr-login"}}"#,
        );
        assert_eq!(
            store_for(&config, "ecr.example.com", |_| false),
            SecretStore::Helper("ecr-login".into())
        );
        assert_eq!(
            store_for(&config, "ghcr.io", |_| false),
            SecretStore::Helper("desktop".into())
        );
        let empty = DockerConfig::default();
        let helper = platform_helper()[0];
        assert_eq!(
            store_for(&empty, "ghcr.io", |name| name == helper),
            SecretStore::Helper(helper.into())
        );
        assert_eq!(
            store_for(&empty, "ghcr.io", |_| false),
            SecretStore::Keychain
        );
        assert_eq!(SecretStore::Keychain.label(), "OS keychain");
    }
}
