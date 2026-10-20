//! Structured database errors safe to show in the UI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DbErrorKind {
    /// Wrong user/password or insufficient privileges to connect.
    Authentication,
    /// Network, DNS, TLS handshake or server unavailable.
    Connection,
    /// The server rejected the SQL (syntax, missing object, constraint...).
    Query,
    Timeout,
    Cancelled,
    /// Feature not supported by this engine/driver.
    Unsupported,
    Internal,
}

/// An error that has already been scrubbed of secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct DbError {
    pub kind: DbErrorKind,
    pub message: String,
    /// Engine-specific code, e.g. a PostgreSQL SQLSTATE.
    pub code: Option<String>,
    pub detail: Option<String>,
    pub hint: Option<String>,
}

impl DbError {
    #[must_use]
    pub fn new(kind: DbErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            code: None,
            detail: None,
            hint: None,
        }
    }

    #[must_use]
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    #[must_use]
    pub fn cancelled() -> Self {
        Self::new(DbErrorKind::Cancelled, "Query cancelled")
    }

    #[must_use]
    pub fn unsupported(what: impl Into<String>) -> Self {
        Self::new(DbErrorKind::Unsupported, what)
    }

    /// Short user-facing guidance for the error class.
    #[must_use]
    pub fn suggestion(&self) -> Option<&'static str> {
        match self.kind {
            DbErrorKind::Authentication => {
                Some("Check the username and password for this profile.")
            }
            DbErrorKind::Connection => {
                Some("Check host, port, network access and TLS settings, then test again.")
            }
            DbErrorKind::Timeout => Some("Increase the timeout or narrow the query."),
            _ => None,
        }
    }
}
