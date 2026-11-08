//! Errors from running git. Messages are git's own (sanitized of URL credentials).

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GitError {
    #[error("git was not found ({0}). Install Git or set its path in settings.")]
    NotInstalled(String),
    #[error("could not start git: {0}")]
    Spawn(String),
    #[error("{message}")]
    Failed { code: i32, message: String },
    #[error("cancelled")]
    Cancelled,
    #[error("not a git repository: {0}")]
    NotARepo(String),
    #[error("{0}")]
    Invalid(String),
    #[error("unexpected git output: {0}")]
    Parse(String),
}

pub type GitResult<T> = Result<T, GitError>;

impl GitError {
    /// Adds a hint for common SSH failures.
    #[must_use]
    pub fn with_ssh_hint(self) -> Self {
        let Self::Failed { code, message } = self else {
            return self;
        };
        let hint = if message.contains("Host key verification failed") {
            Some(
                "The server's host key is not trusted yet. Use “Test connection” in the profile to trust it.",
            )
        } else if message.contains("Permission denied (publickey") {
            Some(
                "The server rejected the SSH key. Check the key selected for this repository's profile and that its public key is added to your account.",
            )
        } else if message.contains("passphrase") || message.contains("incorrect passphrase") {
            Some(
                "The key needs a passphrase. Store it in the profile, or add the key to ssh-agent.",
            )
        } else {
            None
        };
        let message = match hint {
            Some(h) => format!("{message}\n\n{h}"),
            None => message,
        };
        Self::Failed { code, message }
    }
}

/// Removes `user:password@` from URLs so tokens never reach the UI or logs.
#[must_use]
pub fn sanitize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out.push_str(head);
        let end = tail
            .find(|c: char| c.is_whitespace() || c == '/' || c == '\'' || c == '"')
            .unwrap_or(tail.len());
        match tail[..end].rfind('@') {
            Some(at) => {
                out.push_str("***@");
                rest = &tail[at + 1..];
            }
            None => rest = tail,
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_credentials_from_urls() {
        assert_eq!(
            sanitize("fatal: unable to access 'https://me:ghp_x@github.com/o/r/'"),
            "fatal: unable to access 'https://***@github.com/o/r/'"
        );
        assert_eq!(sanitize("ssh://git@host/r"), "ssh://***@host/r");
        assert_eq!(sanitize("no urls here"), "no urls here");
    }

    #[test]
    fn adds_publickey_hint() {
        let e = GitError::Failed {
            code: 128,
            message: "git@github.com: Permission denied (publickey).".into(),
        }
        .with_ssh_hint();
        assert!(e.to_string().contains("rejected the SSH key"));
    }
}
