use ferrumdata_domain::{DbError, DbErrorKind, Endpoint, ResolvedConnectionConfig, TlsMode};
use sqlx::mysql::{MySqlConnectOptions, MySqlSslMode};

/// SQLx options with a redacted `Debug`.
///
/// `MySqlConnectOptions` derives `Debug`, which includes the password, so the
/// raw options must never be logged or formatted; always hold them in this wrapper.
#[derive(Clone)]
pub struct MySqlOptions(pub(crate) MySqlConnectOptions);

impl std::fmt::Debug for MySqlOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MySqlOptions")
            .field("host", &self.0.get_host())
            .field("port", &self.0.get_port())
            .field("username", &self.0.get_username())
            .field("database", &self.0.get_database())
            .field("password", &"***")
            .finish()
    }
}

/// Builds SQLx options from a profile. The password is applied here and nowhere else.
pub fn connect_options(config: &ResolvedConnectionConfig) -> Result<MySqlOptions, DbError> {
    let profile = &config.profile;
    let Endpoint::Tcp { host, port } = &profile.endpoint else {
        return Err(DbError::new(
            DbErrorKind::Connection,
            "MySQL requires a host and port",
        ));
    };
    let mut opts = MySqlConnectOptions::new()
        .host(host)
        .port(*port)
        .ssl_mode(ssl_mode(profile.tls))
        .charset("utf8mb4");
    if let Some(user) = profile.username.as_deref().filter(|u| !u.is_empty()) {
        opts = opts.username(user);
    }
    if let Some(db) = profile.database.as_deref().filter(|d| !d.is_empty()) {
        opts = opts.database(db);
    }
    if let Some(pw) = config.password.as_ref().filter(|p| !p.is_empty()) {
        opts = opts.password(pw.expose());
    }
    Ok(MySqlOptions(opts))
}

const fn ssl_mode(mode: TlsMode) -> MySqlSslMode {
    match mode {
        TlsMode::Disable => MySqlSslMode::Disabled,
        TlsMode::Prefer => MySqlSslMode::Preferred,
        TlsMode::Require => MySqlSslMode::Required,
        TlsMode::VerifyCa => MySqlSslMode::VerifyCa,
        TlsMode::VerifyFull => MySqlSslMode::VerifyIdentity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::{ConnectionProfile, DatabaseEngine, Secret};

    fn config(password: Option<&str>) -> ResolvedConnectionConfig {
        let mut profile = ConnectionProfile::new(DatabaseEngine::MySql, "t");
        profile.database = Some("app".into());
        ResolvedConnectionConfig {
            profile,
            password: password.map(Secret::new),
        }
    }

    #[test]
    fn wrapper_debug_never_prints_password() {
        let opts = connect_options(&config(Some("hunter2-very-secret"))).expect("options");
        assert!(!format!("{opts:?}").contains("hunter2-very-secret"));
        // Canary: if upstream ever stops leaking, the wrapper can be revisited.
        assert!(format!("{:?}", opts.0).contains("hunter2-very-secret"));
    }

    #[test]
    fn maps_profile_fields() {
        let opts = connect_options(&config(None)).expect("options");
        assert_eq!(opts.0.get_host(), "localhost");
        assert_eq!(opts.0.get_port(), 3306);
        assert_eq!(opts.0.get_username(), "root");
        assert_eq!(opts.0.get_database(), Some("app"));
        assert!(matches!(opts.0.get_ssl_mode(), MySqlSslMode::Preferred));
    }

    #[test]
    fn verify_full_checks_identity() {
        assert!(matches!(
            ssl_mode(TlsMode::VerifyFull),
            MySqlSslMode::VerifyIdentity
        ));
        assert!(matches!(ssl_mode(TlsMode::Disable), MySqlSslMode::Disabled));
    }

    #[test]
    fn file_endpoint_is_rejected() {
        let mut c = config(None);
        c.profile.endpoint = Endpoint::File { path: "x".into() };
        assert!(connect_options(&c).is_err());
    }
}
