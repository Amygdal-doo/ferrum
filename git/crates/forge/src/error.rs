//! Forge errors, worded for the UI. Tokens never appear in them.

use ferrumgit_domain::ForgeKind;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ForgeError {
    #[error(
        "No GitHub or GitLab project found for this repository. Add a remote, or set the forge type and API URL in Repository settings."
    )]
    NotDetected,
    #[error("No access token for {host}. {hint}")]
    NoToken { host: String, hint: String },
    #[error("{host} rejected the access token (401). {hint}")]
    Unauthorized { host: String, hint: String },
    #[error("Access denied (403): {0}")]
    Forbidden(String),
    #[error("Not found (404). The project may not exist, or the token cannot see it.{0}")]
    NotFound(String),
    #[error("{0}")]
    Rejected(String),
    #[error("API rate limit reached; try again {}.", reset_text(*.reset_at))]
    RateLimited {
        /// Unix seconds when the limit resets, if the server said.
        reset_at: Option<i64>,
    },
    #[error("Network error talking to {host}: {message}")]
    Network { host: String, message: String },
    #[error("{host} answered HTTP {status}: {message}")]
    Http {
        host: String,
        status: u16,
        message: String,
    },
    #[error("Unexpected response from the API: {0}")]
    Parse(String),
    #[error("{0}")]
    Invalid(String),
}

pub type ForgeResult<T> = Result<T, ForgeError>;

fn reset_text(reset_at: Option<i64>) -> String {
    let Some(at) = reset_at else {
        return "in a few minutes".to_owned();
    };
    let mins = ((at - chrono::Utc::now().timestamp()).max(0) + 59) / 60;
    match mins {
        0 | 1 => "in about a minute".to_owned(),
        m => format!("in about {m} minutes"),
    }
}

/// Which scopes a token needs, in one sentence for error messages and onboarding.
#[must_use]
pub fn scope_hint(kind: ForgeKind) -> &'static str {
    match kind {
        ForgeKind::GitHub => {
            "Create a personal access token with the `repo` scope (classic), or a fine-grained token with Pull requests, Contents and Commit statuses access."
        }
        ForgeKind::GitLab => "Create a personal access token with the `api` scope.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_message_mentions_wait() {
        let at = chrono::Utc::now().timestamp() + 600;
        let e = ForgeError::RateLimited { reset_at: Some(at) };
        assert!(e.to_string().contains("10 minutes"), "{e}");
        let e = ForgeError::RateLimited { reset_at: None };
        assert!(e.to_string().contains("few minutes"));
    }
}
