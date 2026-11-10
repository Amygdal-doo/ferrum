use ferrumdata_domain::{DbError, DbErrorKind, Secret};
use redis::{ErrorKind, RedisError};

/// Converts a client error into a UI-safe [`DbError`], scrubbing the password.
/// `code` carries the server error prefix (e.g. `WRONGTYPE`, `NOPERM`).
pub fn map_redis_error(err: &RedisError, secret: &Secret) -> DbError {
    let code = err.code().map(str::to_owned);
    let kind = if err.is_timeout() {
        DbErrorKind::Timeout
    } else if matches!(err.kind(), ErrorKind::AuthenticationFailed) {
        DbErrorKind::Authentication
    } else if let Some(code) = code.as_deref() {
        kind_for_code(code)
    } else if err.is_io_error()
        || err.is_connection_refusal()
        || err.is_connection_dropped()
        || matches!(err.kind(), ErrorKind::InvalidClientConfig)
    {
        DbErrorKind::Connection
    } else {
        match err.kind() {
            ErrorKind::Server(_) | ErrorKind::Extension => DbErrorKind::Query,
            _ => DbErrorKind::Internal,
        }
    };
    let mut e = DbError::new(kind, secret.scrub(&err.to_string()));
    e.code = code;
    e
}

fn kind_for_code(code: &str) -> DbErrorKind {
    match code {
        "NOAUTH" | "WRONGPASS" | "NOPERM" => DbErrorKind::Authentication,
        "LOADING" | "MASTERDOWN" => DbErrorKind::Connection,
        _ => DbErrorKind::Query,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_server_codes() {
        assert_eq!(kind_for_code("WRONGPASS"), DbErrorKind::Authentication);
        assert_eq!(kind_for_code("NOPERM"), DbErrorKind::Authentication);
        assert_eq!(kind_for_code("WRONGTYPE"), DbErrorKind::Query);
    }

    #[test]
    fn io_errors_are_connection_errors_and_scrubbed() {
        let io = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused topsecret");
        let e = map_redis_error(&RedisError::from(io), &Secret::new("topsecret"));
        assert_eq!(e.kind, DbErrorKind::Connection);
        assert!(!e.message.contains("topsecret"));
    }
}
