//! Stopping a process with the `kill` command (no libc, no unsafe).

use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// Asks the process to exit (SIGTERM).
    Term,
    /// Ends it immediately (SIGKILL); no cleanup.
    Kill,
}

impl Signal {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Signal::Term => "TERM",
            Signal::Kill => "KILL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KillError {
    #[error("Refusing to signal pid {0}.")]
    Refused(u32),
    #[error("Process {0} is no longer running.")]
    NotFound(u32),
    #[error("Permission denied: process {pid} belongs to {}. Run `sudo kill -{signal} {pid}` in a terminal.", user.as_deref().unwrap_or("another user"))]
    PermissionDenied {
        pid: u32,
        user: Option<String>,
        signal: &'static str,
    },
    #[error("kill -{signal} {pid} failed: {message}")]
    Failed {
        pid: u32,
        signal: &'static str,
        message: String,
    },
}

/// Pids that must never be signalled: 0 (our process group), 1 (init /
/// launchd) and FerrumPorts itself.
fn refused(pid: u32) -> bool {
    pid <= 1 || pid == std::process::id()
}

/// Sends `signal` to `pid`. `owner` only improves the permission message.
pub fn send(pid: u32, signal: Signal, owner: Option<&str>) -> Result<(), KillError> {
    if refused(pid) {
        return Err(KillError::Refused(pid));
    }
    let output = Command::new("kill")
        .arg(format!("-{}", signal.name()))
        .arg(pid.to_string())
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| KillError::Failed {
            pid,
            signal: signal.name(),
            message: e.to_string(),
        })?;
    classify(
        pid,
        signal,
        owner,
        output.status.success(),
        &String::from_utf8_lossy(&output.stderr),
    )
}

/// Turns `kill`'s exit status and stderr into a result.
pub fn classify(
    pid: u32,
    signal: Signal,
    owner: Option<&str>,
    success: bool,
    stderr: &str,
) -> Result<(), KillError> {
    if success {
        return Ok(());
    }
    let lower = stderr.to_lowercase();
    if lower.contains("no such process") {
        Err(KillError::NotFound(pid))
    } else if lower.contains("not permitted") || lower.contains("permission") {
        Err(KillError::PermissionDenied {
            pid,
            user: owner.map(str::to_string),
            signal: signal.name(),
        })
    } else {
        Err(KillError::Failed {
            pid,
            signal: signal.name(),
            message: stderr.trim().to_string(),
        })
    }
}

/// Whether `pid` still exists (`kill -0`; a process of another user
/// counts as alive).
#[must_use]
pub fn is_alive(pid: u32) -> bool {
    let Ok(output) = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .env("LC_ALL", "C")
        .output()
    else {
        return false;
    };
    output.status.success()
        || !String::from_utf8_lossy(&output.stderr)
            .to_lowercase()
            .contains("no such process")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_kill_errors() {
        assert_eq!(classify(5, Signal::Term, None, true, ""), Ok(()));
        assert_eq!(
            classify(5, Signal::Term, None, false, "kill: 5: No such process\n"),
            Err(KillError::NotFound(5))
        );
        let denied = classify(
            1234,
            Signal::Kill,
            Some("root"),
            false,
            "kill: 1234: Operation not permitted",
        );
        let message = denied.expect_err("denied").to_string();
        assert!(message.contains("belongs to root"), "{message}");
        assert!(message.contains("sudo kill -KILL 1234"), "{message}");
        assert!(matches!(
            classify(5, Signal::Term, None, false, "weird"),
            Err(KillError::Failed { .. })
        ));
    }

    #[test]
    fn refuses_dangerous_pids() {
        assert_eq!(send(0, Signal::Term, None), Err(KillError::Refused(0)));
        assert_eq!(send(1, Signal::Kill, None), Err(KillError::Refused(1)));
        let me = std::process::id();
        assert_eq!(send(me, Signal::Term, None), Err(KillError::Refused(me)));
    }

    #[test]
    fn terminates_a_child() {
        let Ok(mut child) = Command::new("sleep").arg("30").spawn() else {
            return; // no `sleep` on this system
        };
        let pid = child.id();
        assert!(is_alive(pid));
        send(pid, Signal::Term, None).expect("term");
        child.wait().expect("wait");
        assert!(!is_alive(pid));
        assert_eq!(send(pid, Signal::Term, None), Err(KillError::NotFound(pid)));
    }
}
