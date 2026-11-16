//! Errors of the account client. Messages never contain tokens.

/// Everything that can go wrong signing in, refreshing or signing out.
#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    /// No refresh token for this app (or the launcher): not signed in.
    #[error("not signed in")]
    SignedOut,
    /// The browser came back with `error=access_denied` (Cancel on the
    /// consent page).
    #[error("sign-in was cancelled in the browser")]
    Denied,
    /// The callback's `state` is not the one we sent.
    #[error("the sign-in response did not match this request (state mismatch)")]
    StateMismatch,
    /// The browser did not come back in time.
    #[error("timed out waiting for the browser")]
    TimedOut,
    /// The callback was malformed (no `code`, unknown `error`, …).
    #[error("bad sign-in callback: {0}")]
    Callback(String),
    /// The server answered with an OAuth error (`invalid_grant`, …).
    #[error("the account server refused the request ({status} {code}): {description}")]
    Server {
        status: u16,
        code: String,
        description: String,
    },
    /// Any other unexpected HTTP status.
    #[error("the account server returned HTTP {0}")]
    Http(u16),
    #[error("network error: {0}")]
    Network(String),
    #[error("keychain error: {0}")]
    Keychain(String),
    #[error("could not read or write {path}: {message}")]
    File { path: String, message: String },
    #[error("could not open the browser: {0}")]
    Browser(String),
    #[error("{0}")]
    Invalid(String),
}

impl AccountError {
    /// The server says the refresh token (or code) is no longer valid: the
    /// session ended, so the stored token should be dropped.
    #[must_use]
    pub fn session_ended(&self) -> bool {
        matches!(self, AccountError::SignedOut)
            || matches!(self, AccountError::Server { code, .. } if code == "invalid_grant" || code == "invalid_token")
    }
}

impl From<reqwest::Error> for AccountError {
    fn from(e: reqwest::Error) -> Self {
        // reqwest errors carry the URL, never the body or headers.
        AccountError::Network(e.without_url().to_string())
    }
}

pub type Result<T, E = AccountError> = std::result::Result<T, E>;
