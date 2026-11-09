use ferrumdata_domain::{DbError, DbErrorKind, Secret};
use sqlx::mysql::MySqlDatabaseError;

/// Converts a SQLx error into a UI-safe [`DbError`], scrubbing the password
/// from every text field in case a driver ever echoes it back.
///
/// `code` carries the MySQL error number (e.g. `1062`), which is more specific
/// than the SQLSTATE; the SQLSTATE goes into `detail`.
pub fn map_sqlx_error(err: &sqlx::Error, secret: &Secret) -> DbError {
    let mut out = match err {
        sqlx::Error::Database(db) => {
            let (number, sqlstate) = db
                .try_downcast_ref::<MySqlDatabaseError>()
                .map_or((None, None), |my| {
                    (Some(my.number()), my.code().map(str::to_owned))
                });
            let mut e = DbError::new(kind_for(number), db.message());
            e.code = number.map(|n| n.to_string());
            e.detail = sqlstate.map(|s| format!("SQLSTATE {s}"));
            e
        }
        sqlx::Error::PoolTimedOut => DbError::new(
            DbErrorKind::Timeout,
            "Timed out waiting for a database connection",
        ),
        sqlx::Error::Io(_)
        | sqlx::Error::Tls(_)
        | sqlx::Error::Configuration(_)
        | sqlx::Error::PoolClosed
        | sqlx::Error::Protocol(_) => DbError::new(DbErrorKind::Connection, err.to_string()),
        other => DbError::new(DbErrorKind::Internal, other.to_string()),
    };
    out.message = secret.scrub(&out.message);
    out.detail = out.detail.map(|d| secret.scrub(&d));
    out
}

fn kind_for(number: Option<u16>) -> DbErrorKind {
    match number {
        // ER_DBACCESS_DENIED_ERROR, ER_ACCESS_DENIED_ERROR, ER_ACCESS_DENIED_NO_PASSWORD_ERROR
        Some(1044 | 1045 | 1698) => DbErrorKind::Authentication,
        // ER_QUERY_INTERRUPTED
        Some(1317) => DbErrorKind::Cancelled,
        // ER_QUERY_TIMEOUT (max_execution_time)
        Some(3024) => DbErrorKind::Timeout,
        // ER_SERVER_SHUTDOWN, ER_CON_COUNT_ERROR, ER_TOO_MANY_USER_CONNECTIONS
        Some(1053 | 1040 | 1203) => DbErrorKind::Connection,
        _ => DbErrorKind::Query,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_errors_are_connection_errors_and_scrubbed() {
        let io = std::io::Error::other("refused for pw=topsecret");
        let e = map_sqlx_error(&sqlx::Error::Io(io), &Secret::new("topsecret"));
        assert_eq!(e.kind, DbErrorKind::Connection);
        assert!(!e.message.contains("topsecret"));
    }

    #[test]
    fn classifies_error_numbers() {
        assert_eq!(kind_for(Some(1045)), DbErrorKind::Authentication);
        assert_eq!(kind_for(Some(1317)), DbErrorKind::Cancelled);
        assert_eq!(kind_for(Some(1062)), DbErrorKind::Query);
        assert_eq!(kind_for(None), DbErrorKind::Query);
    }
}
