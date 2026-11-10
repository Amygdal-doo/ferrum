//! SSH port forwards owned by the server: each one is an `ssh -N` child
//! process, so it keeps running when windows close, like sessions do.
//!
//! [`ForwardTable`] is the bookkeeping (running / failed, and which child
//! process an exit belongs to); [`start`] / [`stop`] drive the processes.

use crate::block::lock;
use crate::{Server, now_ms};
use ferrumterm_protocol::{Event, ForwardInfo, ForwardSpec, ForwardState, Response, forward_key};
use std::collections::{BTreeMap, VecDeque};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::oneshot;

/// How long a new forward must stay up before it counts as started
/// (ssh exits quickly on bad hosts, auth failures and refused forwards).
const STARTUP_GRACE: Duration = Duration::from_millis(1500);
/// stderr lines kept for error messages.
const STDERR_LINES: usize = 6;

struct Entry {
    info: ForwardInfo,
    /// Distinguishes runs of the same key, so a late exit of a stopped
    /// process can't mark its replacement as failed.
    serial: u64,
    stop: Option<oneshot::Sender<()>>,
}

#[derive(Default)]
pub struct ForwardTable {
    entries: BTreeMap<String, Entry>,
    next: u64,
}

impl ForwardTable {
    #[must_use]
    pub fn is_running(&self, key: &str) -> bool {
        self.entries
            .get(key)
            .is_some_and(|e| e.info.state == ForwardState::Running)
    }

    /// Records a started forward (replacing a failed entry); returns its serial.
    pub fn insert_running(&mut self, mut info: ForwardInfo, stop: oneshot::Sender<()>) -> u64 {
        self.next += 1;
        info.state = ForwardState::Running;
        info.message = None;
        self.entries.insert(
            info.key.clone(),
            Entry {
                info,
                serial: self.next,
                stop: Some(stop),
            },
        );
        self.next
    }

    /// Records a forward that could not start.
    pub fn insert_failed(&mut self, mut info: ForwardInfo, message: String) {
        info.state = ForwardState::Failed;
        info.pid = None;
        info.message = Some(message);
        self.entries.insert(
            info.key.clone(),
            Entry {
                info,
                serial: 0,
                stop: None,
            },
        );
    }

    /// The process of run `serial` exited on its own. Returns whether the
    /// table changed (false when it was stopped or replaced meanwhile).
    pub fn exited(&mut self, key: &str, serial: u64, message: String) -> bool {
        match self.entries.get_mut(key) {
            Some(e) if e.serial == serial && e.info.state == ForwardState::Running => {
                e.info.state = ForwardState::Failed;
                e.info.pid = None;
                e.info.message = Some(message);
                e.stop = None;
                true
            }
            _ => false,
        }
    }

    /// Removes a forward (running or failed); the sender stops its process.
    pub fn stop(&mut self, key: &str) -> Result<Option<oneshot::Sender<()>>, String> {
        self.entries
            .remove(key)
            .map(|e| e.stop)
            .ok_or_else(|| format!("no forward “{key}” is running"))
    }

    #[must_use]
    pub fn list(&self) -> Vec<ForwardInfo> {
        self.entries.values().map(|e| e.info.clone()).collect()
    }

    /// Stops everything (server shutdown).
    pub fn stop_all(&mut self) {
        for (_, e) in std::mem::take(&mut self.entries) {
            if let Some(s) = e.stop {
                let _ = s.send(());
            }
        }
    }
}

fn last_lines(lines: &Mutex<VecDeque<String>>) -> String {
    lock(lines)
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" · ")
}

fn exit_message(status: std::io::Result<std::process::ExitStatus>, stderr: &str) -> String {
    let code = match status {
        Ok(s) => s.code().map_or_else(
            || "was killed".to_owned(),
            |c| format!("exited with code {c}"),
        ),
        Err(e) => format!("failed: {e}"),
    };
    if stderr.is_empty() {
        format!("ssh {code}")
    } else {
        format!("ssh {code}: {stderr}")
    }
}

fn fail(server: &Server, info: ForwardInfo, message: String) -> Response {
    lock(&server.forwards).insert_failed(info, message.clone());
    server.emit(Event::Forwards);
    Response::Error { message }
}

/// Starts `command` (an `ssh -N …` argv) for `spec`. Refuses when the local
/// port is taken, naming the process that holds it.
pub async fn start(
    server: Arc<Server>,
    host: String,
    spec: ForwardSpec,
    command: Vec<String>,
    env: Vec<(String, String)>,
) -> Response {
    let key = forward_key(&host, &spec);
    if lock(&server.forwards).is_running(&key) {
        return Response::Error {
            message: format!("{key} is already running"),
        };
    }
    let Some((program, args)) = command.split_first() else {
        return Response::Error {
            message: "empty forward command".into(),
        };
    };
    let info = ForwardInfo {
        key: key.clone(),
        host,
        spec: spec.clone(),
        state: ForwardState::Running,
        pid: None,
        started_ms: now_ms(),
        message: None,
        command: command.clone(),
    };
    if let Some(port) = spec.local_port() {
        let bind = spec.bind.clone();
        let taken = tokio::task::spawn_blocking(move || {
            ferrumterm_net::ports::port_in_use(port, bind.as_deref()).then(|| {
                ferrumterm_net::ports::lookup(port)
                    .map(|r| ferrumterm_net::ports::describe_owner(&r))
                    .unwrap_or_else(|_| "another process".into())
            })
        })
        .await
        .ok()
        .flatten();
        if let Some(owner) = taken {
            return fail(
                &server,
                info,
                format!("local port {port} is already in use by {owner}"),
            );
        }
    }

    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for (k, v) in &env {
        cmd.env(k, v);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return fail(&server, info, format!("could not start {program}: {e}")),
    };
    let stderr = Arc::new(Mutex::new(VecDeque::new()));
    let reader = child.stderr.take().map(|pipe| {
        let lines_out = stderr.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(pipe).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim().to_owned();
                if line.is_empty() {
                    continue;
                }
                let mut buf = lock(&lines_out);
                buf.push_back(line);
                if buf.len() > STDERR_LINES {
                    buf.pop_front();
                }
            }
        })
    });

    tokio::select! {
        status = child.wait() => {
            if let Some(r) = reader {
                let _ = tokio::time::timeout(Duration::from_millis(300), r).await;
            }
            let message = exit_message(status, &last_lines(&stderr));
            return fail(&server, info, message);
        }
        () = tokio::time::sleep(STARTUP_GRACE) => {}
    }

    let (stop_tx, stop_rx) = oneshot::channel();
    let info = ForwardInfo {
        pid: child.id(),
        ..info
    };
    let serial = lock(&server.forwards).insert_running(info.clone(), stop_tx);
    tracing::info!(%key, pid = ?info.pid, "port forward started");
    server.emit(Event::Forwards);

    let s = server.clone();
    tokio::spawn(async move {
        tokio::select! {
            status = child.wait() => {
                if let Some(r) = reader {
                    let _ = tokio::time::timeout(Duration::from_millis(300), r).await;
                }
                let message = exit_message(status, &last_lines(&stderr));
                tracing::warn!(%key, %message, "port forward exited");
                if lock(&s.forwards).exited(&key, serial, message) {
                    s.emit(Event::Forwards);
                }
            }
            _ = stop_rx => {
                let _ = child.kill().await;
                tracing::info!(%key, "port forward stopped");
            }
        }
    });
    Response::Forwards {
        forwards: vec![info],
    }
}

pub fn stop(server: &Server, key: &str) -> Response {
    let stopped = lock(&server.forwards).stop(key);
    match stopped {
        Ok(sender) => {
            if let Some(s) = sender {
                let _ = s.send(());
            }
            server.emit(Event::Forwards);
            Response::Ok
        }
        Err(message) => Response::Error { message },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumterm_protocol::ForwardSpec;

    fn info(key: &str) -> ForwardInfo {
        let spec = ForwardSpec::parse("L 8080:localhost:80").expect("spec");
        ForwardInfo {
            key: key.into(),
            host: "h".into(),
            spec,
            state: ForwardState::Running,
            pid: Some(42),
            started_ms: 0,
            message: None,
            command: vec!["ssh".into()],
        }
    }

    #[test]
    fn running_failed_and_stopped() {
        let mut t = ForwardTable::default();
        let (tx, mut rx) = oneshot::channel();
        let serial = t.insert_running(info("h/L8080"), tx);
        assert!(t.is_running("h/L8080"));
        assert!(
            !t.exited("h/L8080", serial + 1, "stale".into()),
            "other run"
        );
        assert!(t.exited("h/L8080", serial, "ssh exited with code 255".into()));
        assert!(!t.is_running("h/L8080"));
        let listed = t.list();
        assert_eq!(listed[0].state, ForwardState::Failed);
        assert_eq!(listed[0].pid, None);
        assert_eq!(
            listed[0].message.as_deref(),
            Some("ssh exited with code 255")
        );
        // Clearing a failed forward works; there is nothing to signal.
        assert!(t.stop("h/L8080").expect("clear").is_none());
        assert!(t.stop("h/L8080").is_err());
        assert!(rx.try_recv().is_err(), "sender dropped, never fired");
    }

    #[test]
    fn stop_signals_and_late_exit_is_ignored() {
        let mut t = ForwardTable::default();
        let (tx, mut rx) = oneshot::channel();
        let serial = t.insert_running(info("h/L8080"), tx);
        let sender = t.stop("h/L8080").expect("stop").expect("sender");
        sender.send(()).expect("send");
        assert!(rx.try_recv().is_ok());
        assert!(!t.exited("h/L8080", serial, "killed".into()));
        assert!(t.list().is_empty());

        t.insert_failed(info("h/L8080"), "port in use".into());
        let (tx, _rx) = oneshot::channel();
        t.insert_running(info("h/L8080"), tx);
        assert!(t.is_running("h/L8080"), "a restart replaces the failure");
        assert_eq!(t.list()[0].message, None);
        t.stop_all();
        assert!(t.list().is_empty());
    }

    #[test]
    fn exit_messages() {
        assert_eq!(
            exit_message(
                Err(std::io::Error::other("boom")),
                "Permission denied (publickey)"
            ),
            "ssh failed: boom: Permission denied (publickey)"
        );
    }
}
