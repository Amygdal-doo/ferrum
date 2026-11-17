//! Runs the system tools and joins their output into [`Listener`]s.
//!
//! macOS / BSD: `lsof` for sockets and working directories, `ps` for
//! command lines and start times. Linux: `ss` (falling back to `lsof`),
//! `ps`, and `/proc/<pid>/cwd`. Parsing lives in [`crate::lsof`],
//! [`crate::ss`] and [`crate::ps`]; [`assemble`] is pure apart from the
//! git-root lookup.

use std::collections::{BTreeMap, HashMap};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::{Listener, Protocol, Snapshot, Socket};
use crate::ps::{self, PsEntry};
use crate::{container, project};
use crate::{lsof, ss};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanOptions {
    /// Include bound UDP sockets.
    pub udp: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScanError {
    #[error("`{0}` was not found. FerrumPorts needs it to list ports.")]
    Missing(&'static str),
    #[error("`{program}` failed: {message}")]
    Failed {
        program: &'static str,
        message: String,
    },
    /// Listing a remote host's ports over ssh failed.
    #[error("{0}")]
    Remote(String),
}

/// Runs `program` and returns its stdout. A non-zero exit with output is
/// fine (`lsof` exits 1 when one of several pids is gone).
fn run(program: &'static str, args: &[&str]) -> Result<String, ScanError> {
    let output = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| match e.kind() {
            ErrorKind::NotFound => ScanError::Missing(program),
            _ => ScanError::Failed {
                program,
                message: e.to_string(),
            },
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() && stdout.is_empty() && !stderr.trim().is_empty() {
        return Err(ScanError::Failed {
            program,
            message: stderr.trim().to_string(),
        });
    }
    Ok(stdout)
}

fn sockets_with_lsof(options: ScanOptions) -> Result<Vec<Socket>, ScanError> {
    let mut sockets = lsof::parse_sockets(&run("lsof", lsof::TCP_ARGS)?);
    if options.udp {
        sockets.extend(lsof::parse_sockets(&run("lsof", lsof::UDP_ARGS)?));
    }
    Ok(sockets)
}

fn sockets_with_ss(options: ScanOptions) -> Result<Vec<Socket>, ScanError> {
    let mut sockets = ss::parse(&run("ss", ss::TCP_ARGS)?, Protocol::Tcp);
    if options.udp {
        sockets.extend(ss::parse(&run("ss", ss::UDP_ARGS)?, Protocol::Udp));
    }
    Ok(sockets)
}

fn pid_list(pids: &[u32]) -> String {
    pids.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn cwds(pids: &[u32]) -> HashMap<u32, PathBuf> {
    if cfg!(target_os = "linux") {
        return pids
            .iter()
            .filter_map(|pid| Some((*pid, std::fs::read_link(format!("/proc/{pid}/cwd")).ok()?)))
            .collect();
    }
    let list = pid_list(pids);
    run("lsof", &["-a", "-w", "-d", "cwd", "-p", &list, "-Fn"])
        .map(|text| lsof::parse_cwds(&text))
        .unwrap_or_default()
}

/// Lists listening sockets with their processes. Blocking: call it off
/// the UI thread.
pub fn scan(options: ScanOptions) -> Result<Snapshot, ScanError> {
    let sockets = if cfg!(target_os = "linux") {
        sockets_with_ss(options).or_else(|_| sockets_with_lsof(options))?
    } else {
        sockets_with_lsof(options)?
    };
    let mut pids: Vec<u32> = sockets.iter().filter_map(|s| s.pid).collect();
    pids.sort_unstable();
    pids.dedup();
    let mut warnings = Vec::new();
    let (processes, cwds) = if pids.is_empty() {
        (HashMap::new(), HashMap::new())
    } else {
        let mut args = ps::ARGS.to_vec();
        let list = pid_list(&pids);
        args.push(&list);
        let processes = match run("ps", &args) {
            Ok(text) => ps::parse(&text),
            Err(e) => {
                warnings.push(format!("Command lines unavailable: {e}"));
                HashMap::new()
            }
        };
        (processes, cwds(&pids))
    };
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut listeners = assemble(sockets, &processes, &cwds, home.as_deref());
    // `docker ps` only when a container runtime holds a port; without
    // Docker (or with it stopped) the rows just show no container.
    if listeners.iter().any(|l| container::is_runtime(&l.process))
        && let Ok(containers) = container::containers(home.as_deref())
    {
        container::attach(&mut listeners, &containers);
    }
    Ok(Snapshot {
        listeners,
        warnings,
    })
}

/// Merges sockets of the same process, protocol and port into one
/// listener, adds process details, and sorts by port.
#[must_use]
pub fn assemble(
    sockets: Vec<Socket>,
    processes: &HashMap<u32, PsEntry>,
    cwds: &HashMap<u32, PathBuf>,
    home: Option<&Path>,
) -> Vec<Listener> {
    let mut merged: BTreeMap<(u16, Protocol, Option<u32>), Listener> = BTreeMap::new();
    let mut projects: HashMap<PathBuf, Option<PathBuf>> = HashMap::new();
    for socket in sockets {
        let key = (socket.port, socket.protocol, socket.pid);
        if let Some(listener) = merged.get_mut(&key) {
            if !listener.addresses.contains(&socket.address) {
                listener.addresses.push(socket.address);
            }
            continue;
        }
        let entry = socket.pid.and_then(|pid| processes.get(&pid));
        let cwd = socket.pid.and_then(|pid| cwds.get(&pid)).cloned();
        let project = cwd.as_ref().and_then(|cwd| {
            projects
                .entry(cwd.clone())
                .or_insert_with(|| project::project_for(cwd, home))
                .clone()
        });
        let process = if socket.process.is_empty() {
            entry
                .and_then(|e| e.command.split_whitespace().next())
                .map(|exe| exe.rsplit('/').next().unwrap_or(exe).to_string())
                .unwrap_or_else(|| "?".to_string())
        } else {
            socket.process
        };
        merged.insert(
            key,
            Listener {
                protocol: socket.protocol,
                port: socket.port,
                addresses: vec![socket.address],
                pid: socket.pid,
                process,
                user: socket.user.or_else(|| entry.map(|e| e.user.clone())),
                command: entry.map(|e| e.command.clone()).filter(|c| !c.is_empty()),
                started: entry.and_then(|e| e.started),
                cwd,
                project,
                container: None,
            },
        );
    }
    merged.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn socket(port: u16, address: &str, pid: Option<u32>, process: &str) -> Socket {
        Socket {
            protocol: Protocol::Tcp,
            address: address.into(),
            port,
            pid,
            process: process.into(),
            user: None,
        }
    }

    #[test]
    fn merges_families_and_joins_processes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path().join("shop");
        std::fs::create_dir_all(repo.join(".git")).expect("mkdir");
        let sockets = vec![
            socket(5432, "127.0.0.1", Some(20), "postgres"),
            socket(3000, "0.0.0.0", Some(10), "node"),
            socket(3000, "::", Some(10), "node"),
            socket(22, "0.0.0.0", None, ""),
        ];
        let processes = HashMap::from([(
            10,
            PsEntry {
                user: "me".into(),
                started: None,
                command: "node server.js".into(),
            },
        )]);
        let cwds = HashMap::from([(10, repo.join("web")), (20, PathBuf::from("/"))]);
        let listeners = assemble(sockets, &processes, &cwds, Some(dir.path()));
        let ports: Vec<u16> = listeners.iter().map(|l| l.port).collect();
        assert_eq!(ports, [22, 3000, 5432]);
        assert_eq!(listeners[0].process, "?");
        let node = &listeners[1];
        assert_eq!(node.addresses, ["0.0.0.0", "::"]);
        assert_eq!(node.command.as_deref(), Some("node server.js"));
        assert_eq!(node.user.as_deref(), Some("me"));
        assert_eq!(node.project.as_deref(), Some(repo.as_path()));
        assert_eq!(listeners[2].project, None);
        assert_eq!(listeners[2].command, None);
    }

    #[test]
    fn fills_missing_names_from_ps() {
        let processes = HashMap::from([(
            7,
            PsEntry {
                user: "root".into(),
                started: None,
                command: "/usr/sbin/sshd -D".into(),
            },
        )]);
        let listeners = assemble(
            vec![socket(22, "::", Some(7), "")],
            &processes,
            &HashMap::new(),
            None,
        );
        assert_eq!(listeners[0].process, "sshd");
    }

    /// Runs the real tools; only checks that a scan works on this machine.
    #[test]
    fn scans_this_machine() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        match scan(ScanOptions::default()) {
            Ok(snapshot) => {
                let me = snapshot.listeners.iter().find(|l| l.port == port);
                // Sandboxed CI may hide sockets; when seen, it is ours.
                if let Some(me) = me {
                    assert_eq!(me.pid, Some(std::process::id()));
                }
            }
            Err(ScanError::Missing(_)) => {}
            Err(e) => panic!("scan failed: {e}"),
        }
    }
}
