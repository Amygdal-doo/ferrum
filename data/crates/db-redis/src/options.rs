//! Profile → redis-rs connection settings. The password is applied here and
//! nowhere else.

use ferrumdata_domain::{DbError, DbErrorKind, Endpoint, ResolvedConnectionConfig, TlsMode};
use redis::{ConnectionAddr, ConnectionInfo, IntoConnectionInfo};

fn config_error(message: impl Into<String>) -> DbError {
    DbError::new(DbErrorKind::Connection, message)
}

/// Builds connection settings from a host/port or `redis://` / `rediss://`
/// profile. Host/port profiles use TLS unless the TLS mode is `Disable`;
/// the unverified modes skip certificate checks, `Verify CA` and
/// `Verify CA + host` both verify the chain and the host name. For a URL,
/// the scheme decides TLS (`#insecure` skips verification).
pub fn connection_info(config: &ResolvedConnectionConfig) -> Result<ConnectionInfo, DbError> {
    let profile = &config.profile;
    let db_index = profile
        .redis_db_index()
        .ok_or_else(|| config_error("DB index must be a number from 0 to 65535"))?;
    let (info, url_db) = match &profile.endpoint {
        Endpoint::Tcp { host, port } => {
            let host = host.trim().to_owned();
            let addr = match profile.tls {
                TlsMode::Disable => ConnectionAddr::Tcp(host, *port),
                mode => ConnectionAddr::TcpTls {
                    host,
                    port: *port,
                    insecure: !mode.verifies_server(),
                    tls_params: None,
                },
            };
            let info = addr
                .into_connection_info()
                .map_err(|e| config_error(e.to_string()))?;
            (info, None)
        }
        Endpoint::Uri { uri } => {
            let info = uri
                .trim()
                .into_connection_info()
                .map_err(|_| config_error("Redis URL did not parse"))?;
            let db = info.redis_settings().db();
            (info, Some(db))
        }
        Endpoint::File { .. } => {
            return Err(config_error(
                "Redis needs a host and port or a redis:// URL",
            ));
        }
    };
    // A DB in the URL path counts unless the DB index field picks another one.
    let db = match url_db {
        Some(db) if db != 0 && db_index == 0 => db,
        _ => i64::from(db_index),
    };
    let mut redis = info.redis_settings().clone().set_db(db);
    if let Some(user) = profile
        .username
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
    {
        redis = redis.set_username(user);
    }
    if let Some(password) = config.password.as_ref().filter(|p| !p.is_empty()) {
        redis = redis.set_password(password.expose());
    }
    Ok(info.set_redis_settings(redis))
}

/// True if the connection will use TLS (a crypto provider must be installed).
#[must_use]
pub fn uses_tls(info: &ConnectionInfo) -> bool {
    matches!(info.addr(), ConnectionAddr::TcpTls { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::{ConnectionProfile, DatabaseEngine, Secret};

    fn config(endpoint: Endpoint, db: Option<&str>) -> ResolvedConnectionConfig {
        let mut profile = ConnectionProfile::new(DatabaseEngine::Redis, "r");
        profile.endpoint = endpoint;
        profile.database = db.map(str::to_owned);
        profile.username = Some("alice".into());
        ResolvedConnectionConfig {
            profile,
            password: Some(Secret::new("pw")),
        }
    }

    fn tcp(host: &str, port: u16) -> Endpoint {
        Endpoint::Tcp {
            host: host.into(),
            port,
        }
    }

    #[test]
    fn host_port_profile_applies_db_user_password() {
        let info = connection_info(&config(tcp("cache", 6380), Some("3"))).expect("info");
        assert_eq!(info.addr(), &ConnectionAddr::Tcp("cache".into(), 6380));
        assert_eq!(info.redis_settings().db(), 3);
        assert_eq!(info.redis_settings().username(), Some("alice"));
        assert_eq!(info.redis_settings().password(), Some("pw"));
        assert!(!uses_tls(&info));
    }

    #[test]
    fn tls_modes_map_to_verified_or_insecure_tls() {
        let mut cfg = config(tcp("h", 1), None);
        cfg.profile.tls = TlsMode::VerifyFull;
        let info = connection_info(&cfg).expect("info");
        assert!(matches!(
            info.addr(),
            ConnectionAddr::TcpTls {
                insecure: false,
                ..
            }
        ));
        cfg.profile.tls = TlsMode::Require;
        let info = connection_info(&cfg).expect("info");
        assert!(matches!(
            info.addr(),
            ConnectionAddr::TcpTls { insecure: true, .. }
        ));
        assert!(uses_tls(&info));
    }

    #[test]
    fn url_profiles_keep_their_db_unless_the_field_overrides_it() {
        let url = |u: &str| Endpoint::Uri { uri: u.into() };
        let info = connection_info(&config(url("rediss://h:6380/4"), Some("0"))).expect("info");
        assert_eq!(info.redis_settings().db(), 4);
        assert!(uses_tls(&info));
        let info = connection_info(&config(url("redis://h/4"), Some("2"))).expect("info");
        assert_eq!(info.redis_settings().db(), 2);
        assert!(connection_info(&config(url("nope://h"), None)).is_err());
    }

    #[test]
    fn bad_db_index_and_file_endpoint_are_rejected() {
        assert!(connection_info(&config(tcp("h", 1), Some("x"))).is_err());
        let file = Endpoint::File { path: "x".into() };
        assert!(connection_info(&config(file, None)).is_err());
    }
}
