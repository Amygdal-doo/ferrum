use ferrumdata_domain::{DbError, DbErrorKind, Secret};
use sqlx::postgres::PgDatabaseError;

/// Converts a SQLx error into a UI-safe [`DbError`], scrubbing the password
/// from every text field in case a driver ever echoes it back.
pub fn map_sqlx_error(err: &sqlx::Error, secret: &Secret) -> DbError {
    let mut out = match err {
        sqlx::Error::Database(db) => {
            let code = db.code().map(|c| c.into_owned());
            let kind = match code.as_deref() {
                // invalid_password, invalid_authorization_specification
                Some("28P01" | "28000") => DbErrorKind::Authentication,
                Some("57014") => DbErrorKind::Cancelled,
                // admin_shutdown, crash_shutdown, cannot_connect_now
                Some("57P01" | "57P02" | "57P03") => DbErrorKind::Connection,
                _ => DbErrorKind::Query,
            };
            let mut e = DbError::new(kind, db.message());
            e.code = code;
            if let Some(pg) = db.try_downcast_ref::<PgDatabaseError>() {
                e.detail = pg.detail().map(str::to_owned);
                e.hint = pg.hint().map(str::to_owned);
            }
            e
        }
        sqlx::Error::PoolTimedOut => DbError::new(
            DbErrorKind::Timeout,
            "Timed out waiting for a database connection",
        ),
        sqlx::Error::Io(_)
        | sqlx::Error::Tls(_)
        | sqlx::Error::Configuration(_)
        | sqlx::Error::PoolClosed => DbError::new(DbErrorKind::Connection, err.to_string()),
        sqlx::Error::Protocol(_) => DbError::new(DbErrorKind::Connection, err.to_string()),
        other => DbError::new(DbErrorKind::Internal, other.to_string()),
    };
    out.message = secret.scrub(&out.message);
    out.detail = out.detail.map(|d| secret.scrub(&d));
    out.hint = out.hint.map(|h| secret.scrub(&h));
    out
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
}
