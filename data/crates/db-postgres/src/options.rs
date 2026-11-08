use ferrumdata_domain::{DbError, DbErrorKind, Endpoint, ResolvedConnectionConfig, TlsMode};
use sqlx::postgres::{PgConnectOptions, PgSslMode};

/// SQLx options with a redacted `Debug`.
///
/// `PgConnectOptions`'s own `Debug` output includes the password, so the raw
/// options must never be logged or formatted; always hold them in this wrapper.
#[derive(Clone)]
pub struct PgOptions(pub(crate) PgConnectOptions);

impl std::fmt::Debug for PgOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgOptions")
            .field("host", &self.0.get_host())
            .field("port", &self.0.get_port())
            .field("username", &self.0.get_username())
            .field("database", &self.0.get_database())
            .field("password", &"***")
            .finish()
    }
}

/// Builds SQLx options from a profile. The password is applied here and nowhere else.
pub fn connect_options(config: &ResolvedConnectionConfig) -> Result<PgOptions, DbError> {
    let profile = &config.profile;
    let Endpoint::Tcp { host, port } = &profile.endpoint else {
        return Err(DbError::new(
            DbErrorKind::Connection,
            "PostgreSQL requires a host and port",
        ));
    };
    let mut opts = PgConnectOptions::new_without_pgpass()
        .host(host)
        .port(*port)
        .ssl_mode(ssl_mode(profile.tls))
        .application_name("FerrumData");
    if let Some(user) = profile.username.as_deref().filter(|u| !u.is_empty()) {
        opts = opts.username(user);
    }
    if let Some(db) = profile.database.as_deref().filter(|d| !d.is_empty()) {
        opts = opts.database(db);
    }
    if let Some(pw) = config.password.as_ref().filter(|p| !p.is_empty()) {
        opts = opts.password(pw.expose());
    }
    if profile.read_only {
        // Server-enforced default for every transaction in the session. Not a
        // security boundary (a session can override it); use read-only roles.
        opts = opts.options([("default_transaction_read_only", "on")]);
    }
    Ok(PgOptions(opts))
}

const fn ssl_mode(mode: TlsMode) -> PgSslMode {
    match mode {
        TlsMode::Disable => PgSslMode::Disable,
        TlsMode::Prefer => PgSslMode::Prefer,
        TlsMode::Require => PgSslMode::Require,
        TlsMode::VerifyCa => PgSslMode::VerifyCa,
        TlsMode::VerifyFull => PgSslMode::VerifyFull,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::{ConnectionProfile, Secret};

    #[test]
    fn wrapper_debug_never_prints_password() {
        let config = ResolvedConnectionConfig {
            profile: ConnectionProfile::new_postgres("t"),
            password: Some(Secret::new("hunter2-very-secret")),
        };
        let opts = connect_options(&config).expect("options");
        assert!(!format!("{opts:?}").contains("hunter2-very-secret"));
        // Canary: if upstream ever stops leaking, the wrapper can be revisited.
        assert!(format!("{:?}", opts.0).contains("hunter2-very-secret"));
    }

    #[test]
    fn read_only_sets_session_default() {
        let mut profile = ConnectionProfile::new_postgres("t");
        profile.read_only = true;
        let opts = connect_options(&ResolvedConnectionConfig {
            profile,
            password: None,
        })
        .expect("o");
        assert!(
            opts.0
                .get_options()
                .unwrap_or_default()
                .contains("default_transaction_read_only")
        );
    }

    #[test]
    fn file_endpoint_is_rejected() {
        let mut profile = ConnectionProfile::new_postgres("t");
        profile.endpoint = Endpoint::File { path: "x".into() };
        assert!(
            connect_options(&ResolvedConnectionConfig {
                profile,
                password: None
            })
            .is_err()
        );
    }
}
