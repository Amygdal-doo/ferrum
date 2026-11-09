//! Profile → connection target and sign-in plan. Holds no live state.

use ferrumdata_domain::{DbError, DbErrorKind, Endpoint, ResolvedConnectionConfig, TlsMode};

/// Where and how to connect, with the password kept out of `Debug`.
#[derive(Clone)]
pub struct SurrealTarget {
    /// `host:port` (IPv6 hosts bracketed), without scheme.
    pub address: String,
    /// `wss://` instead of `ws://`.
    pub tls: bool,
    pub namespace: String,
    pub database: String,
    pub username: Option<String>,
    pub password: String,
}

impl std::fmt::Debug for SurrealTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurrealTarget")
            .field("address", &self.address)
            .field("tls", &self.tls)
            .field("namespace", &self.namespace)
            .field("database", &self.database)
            .field("username", &self.username)
            .field("password", &"***")
            .finish()
    }
}

impl SurrealTarget {
    #[must_use]
    pub fn url(&self) -> String {
        let scheme = if self.tls { "wss" } else { "ws" };
        format!("{scheme}://{}", self.address)
    }
}

/// Builds the target from a profile. Any TLS mode other than `Disable`
/// selects `wss://`, which always verifies the server certificate.
pub fn target(config: &ResolvedConnectionConfig) -> Result<SurrealTarget, DbError> {
    let profile = &config.profile;
    let Endpoint::Tcp { host, port } = &profile.endpoint else {
        return Err(DbError::new(
            DbErrorKind::Connection,
            "SurrealDB requires a host and port",
        ));
    };
    let required = |value: &Option<String>, what: &str| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                DbError::new(
                    DbErrorKind::Connection,
                    format!("SurrealDB requires a {what}"),
                )
            })
    };
    let host = host.trim();
    let address = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    Ok(SurrealTarget {
        address,
        tls: profile.tls != TlsMode::Disable,
        namespace: required(&profile.namespace, "namespace")?,
        database: required(&profile.database, "database")?,
        username: profile
            .username
            .as_deref()
            .filter(|u| !u.is_empty())
            .map(str::to_owned),
        password: config
            .password
            .as_ref()
            .map(|p| p.expose().to_owned())
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::{ConnectionProfile, DatabaseEngine, Secret};

    fn config() -> ResolvedConnectionConfig {
        ResolvedConnectionConfig {
            profile: ConnectionProfile::new(DatabaseEngine::SurrealDb, "s"),
            password: Some(Secret::new("hunter2-very-secret")),
        }
    }

    #[test]
    fn builds_ws_url_and_hides_password() {
        let t = target(&config()).expect("target");
        assert_eq!(t.url(), "ws://localhost:8000");
        assert_eq!(t.namespace, "test");
        assert_eq!(t.username.as_deref(), Some("root"));
        assert!(!format!("{t:?}").contains("hunter2-very-secret"));
    }

    #[test]
    fn tls_selects_wss_and_ipv6_is_bracketed() {
        let mut c = config();
        c.profile.tls = TlsMode::VerifyFull;
        c.profile.endpoint = Endpoint::Tcp {
            host: "::1".into(),
            port: 8000,
        };
        assert_eq!(target(&c).expect("t").url(), "wss://[::1]:8000");
    }

    #[test]
    fn namespace_and_database_are_required() {
        let mut c = config();
        c.profile.namespace = Some("  ".into());
        assert!(target(&c).is_err());
        let mut c = config();
        c.profile.database = None;
        assert!(target(&c).is_err());
    }
}
