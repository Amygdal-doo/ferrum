//! Local port forwards to a remote machine: one `ssh -N -L` child per
//! forward, owned by the app and stopped with it.
//!
//! The command line is FerrumTerm's (`ferrumterm_net::args::forward`, with
//! `ExitOnForwardFailure` and `BatchMode`); FerrumTerm supervises its
//! forwards inside its server, which is not a library, so the small
//! process handling here is FerrumPorts' own.

use std::io::Read;
use std::process::{Child, Command, Stdio};

use ferrumterm_net::{Host, args};
use ferrumterm_protocol::{ForwardKind, ForwardSpec};

use crate::free;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardStatus {
    /// ssh is running but the local port is not bound yet (connecting,
    /// authenticating).
    Connecting,
    /// The local port is bound: connections reach the remote port.
    Running,
    /// ssh exited by itself, with its message.
    Failed(String),
    /// Stopped by the user.
    Stopped,
}

impl ForwardStatus {
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            ForwardStatus::Connecting => "Connecting…",
            ForwardStatus::Running => "Running",
            ForwardStatus::Failed(_) => "Failed",
            ForwardStatus::Stopped => "Stopped",
        }
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self, ForwardStatus::Connecting | ForwardStatus::Running)
    }
}

/// `localhost:<local_port>` → `<host>`'s `<remote_host>:<remote_port>`.
#[derive(Debug)]
pub struct Forward {
    pub host: Host,
    pub local_port: u16,
    /// As seen from the remote machine (`localhost` for its own ports).
    pub remote_host: String,
    pub remote_port: u16,
    /// What listens there, for the label (`postgres`).
    pub process: Option<String>,
    pub status: ForwardStatus,
    child: Option<Child>,
}

/// `remote_port` itself when it is free here, else the nearest free port
/// above it (not one of `taken`).
#[must_use]
pub fn local_port_for(remote_port: u16, taken: &[u16]) -> Option<u16> {
    if !taken.contains(&remote_port) && free::is_port_free(remote_port) {
        return Some(remote_port);
    }
    free::find_free_port_near(remote_port, 200, taken)
}

/// The `ssh -N -L 127.0.0.1:<local>:<remote_host>:<remote_port> <host>`
/// command line.
pub fn command(
    host: &Host,
    local_port: u16,
    remote_host: &str,
    remote_port: u16,
) -> Result<Vec<String>, String> {
    let spec = ForwardSpec {
        kind: ForwardKind::Local,
        bind: Some("127.0.0.1".into()),
        port: local_port,
        target_host: Some(remote_host.into()),
        target_port: Some(remote_port),
    };
    args::forward(&args::ssh_binary(), host, &spec)
}

impl Forward {
    /// Starts `ssh` (returns at once; see [`Self::poll`]).
    pub fn start(
        host: Host,
        local_port: u16,
        remote_host: &str,
        remote_port: u16,
        process: Option<String>,
    ) -> Result<Self, String> {
        let argv = command(&host, local_port, remote_host, remote_port)?;
        let child = Command::new(&argv[0])
            .args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Could not run ssh: {e}"))?;
        Ok(Self {
            host,
            local_port,
            remote_host: remote_host.into(),
            remote_port,
            process,
            status: ForwardStatus::Connecting,
            child: Some(child),
        })
    }

    /// `localhost:5432 → prod:5432`.
    #[must_use]
    pub fn label(&self) -> String {
        let target = if matches!(self.remote_host.as_str(), "localhost" | "127.0.0.1") {
            self.host.name.clone()
        } else {
            format!("{} via {}", self.remote_host, self.host.name)
        };
        format!(
            "localhost:{} → {target}:{}",
            self.local_port, self.remote_port
        )
    }

    #[must_use]
    pub fn url(&self) -> String {
        format!("http://localhost:{}", self.local_port)
    }

    /// Updates the status (ssh exited? local port bound?). Returns whether
    /// it changed. Cheap: no waiting.
    pub fn poll(&mut self) -> bool {
        let Some(child) = self.child.as_mut() else {
            return false;
        };
        let next = match child.try_wait() {
            Ok(Some(status)) => {
                let mut message = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    stderr.read_to_string(&mut message).ok();
                }
                let message = message.trim();
                self.child = None;
                ForwardStatus::Failed(if message.is_empty() {
                    format!("ssh exited ({status})")
                } else {
                    message.to_string()
                })
            }
            Ok(None) if !free::is_port_free(self.local_port) => ForwardStatus::Running,
            Ok(None) => ForwardStatus::Connecting,
            Err(e) => ForwardStatus::Failed(e.to_string()),
        };
        let changed = next != self.status;
        self.status = next;
        changed
    }

    /// Ends ssh (and with it the forward).
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            child.kill().ok();
            child.wait().ok();
        }
        self.status = ForwardStatus::Stopped;
    }
}

impl Drop for Forward {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumterm_net::HostSource;
    use ferrumterm_net::ssh_config::ConfigHost;

    fn host() -> Host {
        Host {
            name: "prod".into(),
            source: HostSource::SshConfig(ConfigHost {
                alias: "prod".into(),
                ..ConfigHost::default()
            }),
        }
    }

    #[test]
    fn builds_the_ssh_command() {
        let argv = command(&host(), 15432, "localhost", 5432).expect("argv");
        assert!(argv.contains(&"-N".to_string()));
        assert!(argv.contains(&"ExitOnForwardFailure=yes".to_string()));
        assert!(argv.contains(&"BatchMode=yes".to_string()));
        assert!(
            argv.windows(2)
                .any(|w| w == ["-L", "127.0.0.1:15432:localhost:5432"])
        );
        assert_eq!(argv[argv.len() - 2..], ["--", "prod"]);
    }

    #[test]
    fn picks_the_same_port_or_a_free_one_near_it() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let busy = listener.local_addr().expect("addr").port();
        let local = local_port_for(busy, &[]).expect("port");
        assert_ne!(local, busy);
        assert!(local > busy);
        let taken = [local];
        assert_ne!(local_port_for(busy, &taken), Some(local));
    }

    #[test]
    fn labels() {
        let forward = Forward {
            host: host(),
            local_port: 15432,
            remote_host: "localhost".into(),
            remote_port: 5432,
            process: None,
            status: ForwardStatus::Stopped,
            child: None,
        };
        assert_eq!(forward.label(), "localhost:15432 → prod:5432");
        assert_eq!(forward.url(), "http://localhost:15432");
        assert!(!forward.status.is_active());
    }

    /// A child that exits at once is reported as failed with its message,
    /// and stop() ends a running one.
    #[test]
    fn tracks_the_child_process() {
        let spawn = |script: &str| {
            Command::new("sh")
                .args(["-c", script])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
        };
        let (Ok(failing), Ok(running)) = (
            spawn("echo 'Permission denied' >&2; exit 255"),
            spawn("sleep 30"),
        ) else {
            return;
        };
        let mut forward = Forward {
            host: host(),
            local_port: 1,
            remote_host: "localhost".into(),
            remote_port: 1,
            process: None,
            status: ForwardStatus::Connecting,
            child: Some(failing),
        };
        for _ in 0..100 {
            forward.poll();
            if matches!(forward.status, ForwardStatus::Failed(_)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(
            forward.status,
            ForwardStatus::Failed("Permission denied".into())
        );
        forward.child = Some(running);
        forward.status = ForwardStatus::Connecting;
        forward.stop();
        assert_eq!(forward.status, ForwardStatus::Stopped);
        assert!(forward.child.is_none());
    }
}
