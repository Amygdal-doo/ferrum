use ferrumdata_domain::{DbError, DbErrorKind, Secret};
use mongodb::error::{ErrorKind, WriteFailure};

/// Converts a driver error into a UI-safe [`DbError`], scrubbing the password.
/// `code` carries the server error code (e.g. `11000` for a duplicate key).
pub fn map_mongo_error(err: &mongodb::error::Error, secret: &Secret) -> DbError {
    let (kind, code) = match err.kind.as_ref() {
        ErrorKind::Authentication { .. } => (DbErrorKind::Authentication, None),
        ErrorKind::ServerSelection { .. }
        | ErrorKind::Io(_)
        | ErrorKind::DnsResolve { .. }
        | ErrorKind::ConnectionPoolCleared { .. }
        | ErrorKind::InvalidTlsConfig { .. }
        | ErrorKind::Shutdown => (DbErrorKind::Connection, None),
        ErrorKind::Command(c) => (kind_for_code(c.code), Some(c.code)),
        ErrorKind::Write(WriteFailure::WriteError(w)) => (kind_for_code(w.code), Some(w.code)),
        ErrorKind::Write(WriteFailure::WriteConcernError(w)) => (DbErrorKind::Query, Some(w.code)),
        ErrorKind::InvalidArgument { .. } => (DbErrorKind::Query, None),
        _ => (DbErrorKind::Internal, None),
    };
    let mut e = DbError::new(kind, secret.scrub(&err.kind.to_string()));
    e.code = code.map(|c| c.to_string());
    e
}

fn kind_for_code(code: i32) -> DbErrorKind {
    match code {
        // Unauthorized, AuthenticationFailed
        13 | 18 => DbErrorKind::Authentication,
        // MaxTimeMSExpired
        50 => DbErrorKind::Timeout,
        // Interrupted, CursorKilled
        11601 | 237 => DbErrorKind::Cancelled,
        _ => DbErrorKind::Query,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_server_codes() {
        assert_eq!(kind_for_code(18), DbErrorKind::Authentication);
        assert_eq!(kind_for_code(50), DbErrorKind::Timeout);
        assert_eq!(kind_for_code(11000), DbErrorKind::Query);
    }

    #[test]
    fn io_errors_are_connection_errors_and_scrubbed() {
        let io = std::io::Error::other("refused pw=topsecret");
        let err = mongodb::error::Error::from(io);
        let e = map_mongo_error(&err, &Secret::new("topsecret"));
        assert_eq!(e.kind, DbErrorKind::Connection);
        assert!(!e.message.contains("topsecret"));
    }
}
