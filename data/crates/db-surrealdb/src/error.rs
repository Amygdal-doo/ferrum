use ferrumdata_domain::{DbError, DbErrorKind, Secret};

/// Converts a SurrealDB SDK error into a UI-safe [`DbError`], scrubbing the
/// password. `during_signin` turns permission errors into authentication ones.
pub fn map_surreal_error(err: &surrealdb::Error, secret: &Secret, during_signin: bool) -> DbError {
    let kind = if err.is_not_allowed() {
        if during_signin {
            DbErrorKind::Authentication
        } else {
            DbErrorKind::Query
        }
    } else if err.is_connection() {
        DbErrorKind::Connection
    } else if err.is_internal() || err.is_serialization() {
        DbErrorKind::Internal
    } else {
        // Validation (parse errors), query, thrown, not-found, already-exists…
        DbErrorKind::Query
    };
    let mut e = DbError::new(kind, secret.scrub(err.message()));
    e.code = Some(err.kind_str().to_owned());
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_errors_are_classified_and_scrubbed() {
        let err = surrealdb::Error::connection("refused pw=topsecret".into(), None);
        let e = map_surreal_error(&err, &Secret::new("topsecret"), false);
        assert_eq!(e.kind, DbErrorKind::Connection);
        assert!(!e.message.contains("topsecret"));
        assert_eq!(e.code.as_deref(), Some("Connection"));
    }

    #[test]
    fn internal_errors_stay_internal() {
        let err = surrealdb::Error::internal("boom".into());
        assert_eq!(
            map_surreal_error(&err, &Secret::new(""), true).kind,
            DbErrorKind::Internal
        );
    }
}
