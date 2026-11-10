//! Engine errors, already phrased for the UI.

/// Errors from talking to the container engine.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineError {
    /// No socket found and no `DOCKER_HOST` set.
    #[error("no container engine found")]
    NoEngine,
    /// A socket exists but nothing answers (engine stopped, permissions…).
    #[error("cannot reach the engine at {endpoint}: {message}")]
    Unreachable { endpoint: String, message: String },
    /// The engine answered with an error (HTTP status and its message).
    #[error("{message}")]
    Api { status: u16, message: String },
    /// Transport or decoding failure in the middle of a call.
    #[error("{0}")]
    Transport(String),
    /// An external command (`docker compose`, the terminal) failed.
    #[error("{0}")]
    Command(String),
}

impl EngineError {
    /// HTTP 404 from the engine (container / image already gone).
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::Api { status: 404, .. })
    }

    /// HTTP 409: in use, already running/stopped, name conflict…
    #[must_use]
    pub fn is_conflict(&self) -> bool {
        matches!(self, Self::Api { status: 409, .. })
    }
}

impl From<bollard::errors::Error> for EngineError {
    fn from(error: bollard::errors::Error) -> Self {
        use bollard::errors::Error as B;
        match error {
            B::DockerResponseServerError {
                status_code,
                message,
            } => Self::Api {
                status: status_code,
                message: capitalize(message.trim()),
            },
            B::DockerStreamError { error } => Self::Api {
                status: 500,
                message: capitalize(error.trim()),
            },
            other => Self::Transport(other.to_string()),
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

pub type EngineResult<T> = Result<T, EngineError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_server_errors_with_status() {
        let error: EngineError = bollard::errors::Error::DockerResponseServerError {
            status_code: 409,
            message: "conflict: unable to remove repository reference".into(),
        }
        .into();
        assert!(error.is_conflict());
        assert!(!error.is_not_found());
        assert_eq!(
            error.to_string(),
            "Conflict: unable to remove repository reference"
        );
    }
}
