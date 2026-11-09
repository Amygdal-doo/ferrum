use ferrumdata_domain::{DbError, DbErrorKind, Endpoint, ResolvedConnectionConfig};
use sqlx::sqlite::SqliteConnectOptions;
use std::path::Path;
use std::time::Duration;

/// How long a statement waits for another connection's write lock.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Builds SQLx options from a profile. SQLite has no credentials; the file
/// must already exist (FerrumData never creates databases by accident).
pub fn connect_options(config: &ResolvedConnectionConfig) -> Result<SqliteConnectOptions, DbError> {
    let profile = &config.profile;
    let Endpoint::File { path } = &profile.endpoint else {
        return Err(DbError::new(
            DbErrorKind::Connection,
            "SQLite requires a database file path",
        ));
    };
    let path = expand_home(path.trim());
    if !Path::new(&path).is_file() {
        return Err(DbError::new(
            DbErrorKind::Connection,
            format!("Database file not found: {path}"),
        ));
    }
    Ok(SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(false)
        // Enforced by SQLite itself (SQLITE_OPEN_READONLY), unlike the
        // session defaults of server engines.
        .read_only(profile.read_only)
        .busy_timeout(BUSY_TIMEOUT)
        .foreign_keys(true))
}

/// `~/x.db` → `$HOME/x.db`; other paths are returned unchanged.
fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => Path::new(&home).join(rest).to_string_lossy().into_owned(),
        _ => path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::{ConnectionProfile, DatabaseEngine};

    fn config(path: &str, read_only: bool) -> ResolvedConnectionConfig {
        let mut profile = ConnectionProfile::new(DatabaseEngine::Sqlite, "t");
        profile.endpoint = Endpoint::File { path: path.into() };
        profile.read_only = read_only;
        ResolvedConnectionConfig {
            profile,
            password: None,
        }
    }

    #[test]
    fn missing_file_is_a_connection_error() {
        let e = connect_options(&config("/definitely/not/here.db", false)).expect_err("missing");
        assert_eq!(e.kind, DbErrorKind::Connection);
        assert!(e.message.contains("not found"));
    }

    #[test]
    fn existing_file_builds_options() {
        let file = tempfile::NamedTempFile::new().expect("tmp");
        let path = file.path().to_string_lossy().into_owned();
        let opts = connect_options(&config(&path, true)).expect("options");
        assert_eq!(opts.get_filename(), Path::new(&path));
    }

    #[test]
    fn tcp_endpoint_is_rejected() {
        let mut c = config("x", false);
        c.profile.endpoint = Endpoint::Tcp {
            host: "h".into(),
            port: 1,
        };
        assert!(connect_options(&c).is_err());
    }

    #[test]
    fn expands_home_prefix_only() {
        assert_eq!(expand_home("/abs/x.db"), "/abs/x.db");
        assert_eq!(expand_home("rel~/x.db"), "rel~/x.db");
        if let Some(home) = std::env::var_os("HOME") {
            assert!(expand_home("~/x.db").starts_with(&*home.to_string_lossy()));
        }
    }
}
