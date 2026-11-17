//! `ssh://` engines: an OpenSSH tunnel from a private local socket to the
//! remote Docker socket (`ssh -L <local.sock>:/var/run/docker.sock`), so the
//! Engine API calls (bollard and the raw requests) talk to a Unix socket as
//! usual. The `docker` CLI is handed the `ssh://` address itself (compose,
//! exec), and connects on its own.
//!
//! The system `ssh` is used with `BatchMode=yes`: keys or the SSH agent
//! must work without a prompt, exactly as for `docker -H ssh://…`.

use crate::error::{EngineError, EngineResult};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// Where the engine listens on the remote machine.
pub const REMOTE_SOCKET: &str = "/var/run/docker.sock";
/// Connecting and authenticating must be done by then.
const OPEN_TIMEOUT: Duration = Duration::from_secs(20);

/// `[user@]host[:port]` of an `ssh://` address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTarget {
    pub user: Option<String>,
    pub host: String,
    pub port: Option<u16>,
}

impl SshTarget {
    /// Parses `[user@]host[:port]` (`[::1]:22` for IPv6). Paths, spaces and
    /// names starting with `-` (they would read as `ssh` options) are refused.
    #[must_use]
    pub fn parse(target: &str) -> Option<Self> {
        if target.is_empty() || target.contains(['/', '\\']) || target.contains(char::is_whitespace)
        {
            return None;
        }
        let (user, rest) = match target.rsplit_once('@') {
            Some((user, rest)) => (Some(user), rest),
            None => (None, target),
        };
        if user.is_some_and(|u| u.is_empty() || u.starts_with('-')) {
            return None;
        }
        let (host, port) = if let Some(bracketed) = rest.strip_prefix('[') {
            let (host, after) = bracketed.split_once(']')?;
            match after.strip_prefix(':') {
                Some(port) => (host, Some(port.parse().ok()?)),
                None if after.is_empty() => (host, None),
                None => return None,
            }
        } else {
            match rest.split_once(':') {
                Some((host, port)) => (host, Some(port.parse().ok()?)),
                None => (rest, None),
            }
        };
        if host.is_empty() || host.starts_with('-') {
            return None;
        }
        Some(Self {
            user: user.map(str::to_owned),
            host: host.to_owned(),
            port,
        })
    }

    /// `user@host` (or `host`), what `ssh` connects to.
    #[must_use]
    pub fn destination(&self) -> String {
        match &self.user {
            Some(user) => format!("{user}@{}", self.host),
            None => self.host.clone(),
        }
    }
}

/// Arguments after `ssh` for a tunnel from `local` to [`REMOTE_SOCKET`].
#[must_use]
pub fn tunnel_args(target: &SshTarget, local: &Path) -> Vec<String> {
    let mut args: Vec<String> = [
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "ExitOnForwardFailure=yes",
        "-o",
        "ServerAliveInterval=30",
        "-o",
        "StreamLocalBindUnlink=yes",
        "-nNT",
        "-L",
    ]
    .map(str::to_owned)
    .to_vec();
    args.push(format!("{}:{REMOTE_SOCKET}", local.display()));
    if let Some(port) = target.port {
        args.extend(["-p".to_owned(), port.to_string()]);
    }
    args.extend(["--".to_owned(), target.destination()]);
    args
}

/// The system `ssh`: `PATH`, then `/usr/bin/ssh`.
#[must_use]
pub fn find_ssh() -> Option<PathBuf> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|dir| dir.join("ssh"))
        .chain([PathBuf::from("/usr/bin/ssh")])
        .find(|p| p.is_file())
}

/// A fresh socket path in the temp folder (short enough for `sun_path`).
fn local_socket() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("ferrumdock-ssh-{}-{n}.sock", std::process::id()))
}

/// A running tunnel. Dropping it stops `ssh` and removes the socket.
pub struct Tunnel {
    pub socket: PathBuf,
    child: Mutex<tokio::process::Child>,
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        let child = self
            .child
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = child.start_kill();
        let _ = std::fs::remove_file(&self.socket);
    }
}

impl Tunnel {
    /// Starts `ssh` (at `ssh`) and waits until the local socket exists, which
    /// it does once the connection is authenticated. `ssh`'s own message is
    /// the error when it gives up (unknown host, key refused…).
    pub async fn open(ssh: &Path, target: &SshTarget) -> EngineResult<Self> {
        Self::open_at(ssh, target, local_socket(), OPEN_TIMEOUT).await
    }

    async fn open_at(
        ssh: &Path,
        target: &SshTarget,
        socket: PathBuf,
        timeout: Duration,
    ) -> EngineResult<Self> {
        let endpoint = format!("ssh://{}", target.destination());
        let unreachable = |message: String| EngineError::Unreachable {
            endpoint: endpoint.clone(),
            message,
        };
        let _ = std::fs::remove_file(&socket);
        tracing::info!(host = %target.host, "opening SSH tunnel");
        let mut child = tokio::process::Command::new(ssh)
            .args(tunnel_args(target, &socket))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| unreachable(format!("could not run {}: {e}", ssh.display())))?;
        let deadline = Instant::now() + timeout;
        loop {
            if socket.exists() {
                return Ok(Self {
                    socket,
                    child: Mutex::new(child),
                });
            }
            if let Some(status) = child.try_wait().map_err(|e| unreachable(e.to_string()))? {
                let mut message = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    use tokio::io::AsyncReadExt;
                    let _ = stderr.read_to_string(&mut message).await;
                }
                let message = message.trim();
                return Err(unreachable(if message.is_empty() {
                    format!("ssh exited with {status}")
                } else {
                    message.to_owned()
                }));
            }
            if Instant::now() > deadline {
                let _ = child.start_kill();
                return Err(unreachable("SSH connection timed out".to_owned()));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_targets() {
        let target = SshTarget::parse("me@box.lan:2222");
        assert_eq!(
            target,
            Some(SshTarget {
                user: Some("me".into()),
                host: "box.lan".into(),
                port: Some(2222)
            })
        );
        assert_eq!(
            SshTarget::parse("box").map(|t| t.destination()).as_deref(),
            Some("box")
        );
        assert_eq!(
            SshTarget::parse("ops@[fe80::1]:22").map(|t| (t.host, t.port)),
            Some(("fe80::1".into(), Some(22)))
        );
        for bad in [
            "",
            "me@",
            "@box",
            "box:port",
            "box/path",
            "-oProxyCommand=x",
            "me@-x",
            "a b",
            "[::1",
        ] {
            assert_eq!(SshTarget::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn builds_the_tunnel_command() {
        let target = SshTarget::parse("me@box:2222").unwrap_or_else(|| panic!("target"));
        let args = tunnel_args(&target, Path::new("/tmp/t.sock"));
        let line = args.join(" ");
        assert!(line.contains("BatchMode=yes"));
        assert!(line.contains("-L /tmp/t.sock:/var/run/docker.sock"));
        assert!(line.ends_with("-p 2222 -- me@box"), "{line}");
    }

    /// A stand-in `ssh` script in a temp folder.
    #[cfg(unix)]
    fn fake_ssh(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("ssh");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap_or_else(|e| panic!("{e}"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|e| panic!("{e}"));
        path
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn reports_what_ssh_said_when_it_fails() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let ssh = fake_ssh(
            dir.path(),
            "echo 'me@box: Permission denied (publickey).' >&2; exit 255",
        );
        let target = SshTarget::parse("me@box").unwrap_or_else(|| panic!("target"));
        let socket = dir.path().join("t.sock");
        let result = Tunnel::open_at(&ssh, &target, socket, Duration::from_secs(5)).await;
        match result {
            Err(EngineError::Unreachable { endpoint, message }) => {
                assert_eq!(endpoint, "ssh://me@box");
                assert!(message.contains("Permission denied"), "{message}");
            }
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("should fail"),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn is_ready_once_the_socket_exists_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let socket = dir.path().join("t.sock");
        // The stand-in creates the "forwarded" socket path, then stays up.
        let ssh = fake_ssh(
            dir.path(),
            &format!("touch '{}'; sleep 30", socket.display()),
        );
        let target = SshTarget::parse("box").unwrap_or_else(|| panic!("target"));
        let tunnel = Tunnel::open_at(&ssh, &target, socket.clone(), Duration::from_secs(5)).await;
        let tunnel = tunnel.unwrap_or_else(|e| panic!("open: {e}"));
        assert_eq!(tunnel.socket, socket);
        drop(tunnel);
        assert!(!socket.exists(), "the socket goes with the tunnel");

        let slow = fake_ssh(dir.path(), "sleep 30");
        let timed_out = Tunnel::open_at(
            &slow,
            &target,
            dir.path().join("never.sock"),
            Duration::from_millis(200),
        )
        .await;
        assert!(timed_out.is_err_and(|e| e.to_string().contains("timed out")));
    }
}
