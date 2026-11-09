use ferrumdata_domain::{DbError, DbErrorKind};

/// Converts a SQLx error into a UI-safe [`DbError`]. SQLite has no secrets to scrub.
pub fn map_sqlx_error(err: &sqlx::Error) -> DbError {
    match err {
        sqlx::Error::Database(db) => {
            let code = db.code().map(|c| c.into_owned());
            // Primary result code is the low byte of an extended code.
            let primary = code
                .as_deref()
                .and_then(|c| c.parse::<i32>().ok())
                .map(|c| c & 0xff);
            let kind = match primary {
                // SQLITE_INTERRUPT
                Some(9) => DbErrorKind::Cancelled,
                // SQLITE_CANTOPEN, SQLITE_NOTADB
                Some(14 | 26) => DbErrorKind::Connection,
                // SQLITE_AUTH
                Some(23) => DbErrorKind::Authentication,
                _ => DbErrorKind::Query,
            };
            let mut e = DbError::new(kind, db.message());
            e.code = code;
            e
        }
        sqlx::Error::PoolTimedOut => DbError::new(
            DbErrorKind::Timeout,
            "Timed out waiting for a database connection",
        ),
        sqlx::Error::Io(_) | sqlx::Error::Configuration(_) | sqlx::Error::PoolClosed => {
            DbError::new(DbErrorKind::Connection, err.to_string())
        }
        other => DbError::new(DbErrorKind::Internal, other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_errors_are_connection_errors() {
        let io = std::io::Error::other("disk gone");
        assert_eq!(
            map_sqlx_error(&sqlx::Error::Io(io)).kind,
            DbErrorKind::Connection
        );
        assert_eq!(
            map_sqlx_error(&sqlx::Error::PoolTimedOut).kind,
            DbErrorKind::Timeout
        );
    }
}
