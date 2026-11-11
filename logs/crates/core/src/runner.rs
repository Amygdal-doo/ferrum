//! Running sources on background threads: shell commands (also `docker
//! logs` / `docker compose logs`) and file tails. Output is parsed on the
//! producer threads and sent in batches over a bounded channel, so a fast
//! producer is slowed down instead of flooding the UI.

use crate::buffer::Splitter;
use crate::line::{LogLine, ParseOptions, Stream, parse};
use crate::source::SourceKind;
use crate::tail::{FileTail, TailEvent};
use futures::SinkExt;
use futures::channel::mpsc;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Identifies one start of a source; a restart gets a new one.
pub type RunId = u64;

/// Batches in flight before producers block.
pub const CHANNEL_CAPACITY: usize = 256;
/// How often files are checked for new data.
pub const TAIL_INTERVAL: Duration = Duration::from_millis(250);
/// Time between SIGTERM and SIGKILL when stopping a command.
const KILL_AFTER: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    /// The process ended on its own (exit code; None when killed by a signal).
    Exited(Option<i32>),
    /// Stopped by the user.
    Stopped,
    /// Could not start.
    Failed(String),
}

impl RunStatus {
    #[must_use]
    pub fn is_running(&self) -> bool {
        *self == RunStatus::Running
    }

    /// Ended badly: failed to start or a non-zero exit.
    #[must_use]
    pub fn is_failure(&self) -> bool {
        matches!(self, RunStatus::Failed(_) | RunStatus::Exited(None))
            || matches!(self, RunStatus::Exited(Some(code)) if *code != 0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Lines { run: RunId, lines: Vec<LogLine> },
    Status { run: RunId, status: RunStatus },
}

impl Event {
    #[must_use]
    pub fn run(&self) -> RunId {
        match self {
            Event::Lines { run, .. } | Event::Status { run, .. } => *run,
        }
    }
}

pub type EventSender = mpsc::Sender<Event>;
pub type EventReceiver = mpsc::Receiver<Event>;

#[must_use]
pub fn channel() -> (EventSender, EventReceiver) {
    mpsc::channel(CHANNEL_CAPACITY)
}

/// Everything needed to start a source.
#[derive(Debug, Clone)]
pub struct RunSpec {
    pub run: RunId,
    /// Session slot written into every line.
    pub slot: u32,
    pub kind: SourceKind,
    pub project: PathBuf,
    /// The shell for commands, e.g. `$SHELL`.
    pub shell: PathBuf,
    /// Run the shell as a login shell (`-l`), for the user's PATH.
    pub login: bool,
    pub home: Option<PathBuf>,
}

impl RunSpec {
    /// The user's shell, as a login shell.
    #[must_use]
    pub fn user_shell() -> PathBuf {
        std::env::var_os("SHELL")
            .filter(|s| !s.is_empty())
            .map_or_else(|| PathBuf::from("/bin/sh"), PathBuf::from)
    }
}

/// A running source. Dropping it stops the source.
#[derive(Debug)]
pub struct RunHandle {
    pub run: RunId,
    stop: Arc<AtomicBool>,
    /// The process group leader, for commands.
    pid: Option<u32>,
}

impl RunHandle {
    /// Asks the source to stop: SIGTERM to the whole process group, then
    /// SIGKILL if it is still around a few seconds later.
    pub fn stop(&self) {
        if self.stop.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(pid) = self.pid {
            signal_group(pid, "TERM");
        }
    }
}

impl Drop for RunHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn signal_group(pid: u32, signal: &str) {
    // `kill -SIG -PGID` reaches the shell and everything it started.
    let _ = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(format!("-{pid}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn send(tx: &mut EventSender, event: Event) -> bool {
    futures::executor::block_on(tx.send(event)).is_ok()
}

/// Starts `spec` on background threads.
#[must_use]
pub fn spawn(spec: RunSpec, tx: EventSender) -> RunHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let run = spec.run;
    let pid = match &spec.kind {
        SourceKind::File { path } => {
            let pattern = path.clone();
            spawn_tail(spec, pattern, stop.clone(), tx);
            None
        }
        kind => {
            let line = kind.command_line().unwrap_or_default();
            spawn_command(spec, &line, stop.clone(), tx)
        }
    };
    RunHandle { run, stop, pid }
}

fn spawn_command(
    spec: RunSpec,
    line: &str,
    stop: Arc<AtomicBool>,
    mut tx: EventSender,
) -> Option<u32> {
    let run = spec.run;
    let mut command = Command::new(&spec.shell);
    if spec.login {
        command.arg("-l");
    }
    command
        .arg("-c")
        .arg(line)
        .current_dir(&spec.project)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Most tools drop colours when not on a terminal; ask for them.
        .env("FORCE_COLOR", "1")
        .env("CLICOLOR_FORCE", "1")
        .env("CARGO_TERM_COLOR", "always");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            let status = RunStatus::Failed(format!("{}: {e}", spec.shell.display()));
            send(&mut tx, Event::Status { run, status });
            return None;
        }
    };
    let pid = child.id();
    send(
        &mut tx,
        Event::Status {
            run,
            status: RunStatus::Running,
        },
    );
    let options = spec.kind.parse_options();
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        readers.push(spawn_reader(
            out,
            Stream::Stdout,
            spec.slot,
            run,
            options,
            tx.clone(),
        ));
    }
    if let Some(err) = child.stderr.take() {
        readers.push(spawn_reader(
            err,
            Stream::Stderr,
            spec.slot,
            run,
            options,
            tx.clone(),
        ));
    }
    let exited = Arc::new(AtomicBool::new(false));
    // Escalate to SIGKILL when a stop request is ignored.
    {
        let stop = stop.clone();
        let exited = exited.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                if exited.load(Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let deadline = std::time::Instant::now() + KILL_AFTER;
            while std::time::Instant::now() < deadline {
                if exited.load(Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            signal_group(pid, "KILL");
        });
    }
    std::thread::spawn(move || {
        // Readers end when every process holding the pipes is gone, so
        // all output arrives before the status.
        for reader in readers {
            let _ = reader.join();
        }
        let code = child.wait().ok().and_then(|s| s.code());
        exited.store(true, Ordering::SeqCst);
        let status = if stop.load(Ordering::SeqCst) {
            RunStatus::Stopped
        } else {
            RunStatus::Exited(code)
        };
        send(&mut tx, Event::Status { run, status });
    });
    Some(pid)
}

fn spawn_reader(
    mut pipe: impl Read + Send + 'static,
    stream: Stream,
    slot: u32,
    run: RunId,
    options: ParseOptions,
    mut tx: EventSender,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut splitter = Splitter::default();
        let mut buf = vec![0u8; 64 * 1024];
        let mut connected = true;
        loop {
            let n = match pipe.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            if !connected {
                // The UI is gone: keep draining so the process isn't blocked.
                continue;
            }
            let mut lines = Vec::new();
            splitter.feed(&buf[..n], |raw| {
                lines.push(parse(slot, stream, raw, options))
            });
            if !lines.is_empty() {
                connected = send(&mut tx, Event::Lines { run, lines });
            }
        }
        let mut lines = Vec::new();
        splitter.finish(|raw| lines.push(parse(slot, stream, raw, options)));
        if !lines.is_empty() && connected {
            send(&mut tx, Event::Lines { run, lines });
        }
    })
}

fn spawn_tail(spec: RunSpec, pattern: String, stop: Arc<AtomicBool>, mut tx: EventSender) {
    let run = spec.run;
    let slot = spec.slot;
    let multi = crate::glob::has_wildcards(&pattern);
    std::thread::spawn(move || {
        let mut tail = FileTail::new(&spec.project, &pattern, spec.home.clone());
        if !send(
            &mut tx,
            Event::Status {
                run,
                status: RunStatus::Running,
            },
        ) {
            return;
        }
        while !stop.load(Ordering::SeqCst) {
            let mut lines = Vec::new();
            for event in tail.poll() {
                match event {
                    TailEvent::Lines { file, lines: raw } => {
                        let tag = multi
                            .then(|| file.file_name().map(|n| n.to_string_lossy().into_owned()))
                            .flatten();
                        for raw in raw {
                            let mut line =
                                parse(slot, Stream::Stdout, &raw, ParseOptions::default());
                            line.tag.clone_from(&tag);
                            lines.push(line);
                        }
                    }
                    TailEvent::Note(note) => lines.push(LogLine::system(slot, note, None)),
                }
            }
            if !lines.is_empty() && !send(&mut tx, Event::Lines { run, lines }) {
                return;
            }
            std::thread::sleep(TAIL_INTERVAL);
        }
        send(
            &mut tx,
            Event::Status {
                run,
                status: RunStatus::Stopped,
            },
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    fn spec(kind: SourceKind, project: PathBuf) -> RunSpec {
        RunSpec {
            run: 7,
            slot: 2,
            kind,
            project,
            shell: PathBuf::from("/bin/sh"),
            login: false,
            home: None,
        }
    }

    /// Collects events until `done` says so.
    fn collect(rx: &mut EventReceiver, done: impl Fn(&[Event]) -> bool) -> Vec<Event> {
        let mut events = Vec::new();
        while !done(&events) {
            match futures::executor::block_on(rx.next()) {
                Some(event) => events.push(event),
                None => break,
            }
        }
        events
    }

    fn texts(events: &[Event]) -> Vec<(Stream, String)> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Lines { lines, .. } => {
                    Some(lines.iter().map(|l| (l.stream, l.text.clone())))
                }
                Event::Status { .. } => None,
            })
            .flatten()
            .collect()
    }

    fn ended(events: &[Event]) -> bool {
        events
            .iter()
            .any(|e| matches!(e, Event::Status { status, .. } if !status.is_running()))
    }

    #[test]
    fn commands_capture_both_streams_and_exit_status() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("marker"), "").expect("marker");
        let (tx, mut rx) = channel();
        let kind = SourceKind::Command {
            command: "ls; echo 'WARN oops' >&2; printf tail; exit 3".into(),
        };
        let handle = spawn(spec(kind, dir.path().to_path_buf()), tx);
        let events = collect(&mut rx, ended);
        let lines = texts(&events);
        assert!(
            lines.contains(&(Stream::Stdout, "marker".into())),
            "{lines:?}"
        );
        assert!(lines.contains(&(Stream::Stderr, "WARN oops".into())));
        assert!(
            lines.contains(&(Stream::Stdout, "tail".into())),
            "partial flushed"
        );
        assert_eq!(
            events.first(),
            Some(&Event::Status {
                run: 7,
                status: RunStatus::Running
            })
        );
        assert_eq!(
            events.last(),
            Some(&Event::Status {
                run: 7,
                status: RunStatus::Exited(Some(3))
            })
        );
        assert!(RunStatus::Exited(Some(3)).is_failure());
        assert!(!RunStatus::Exited(Some(0)).is_failure());
        if let Some(Event::Lines { lines, .. }) = events.get(1) {
            assert_eq!(lines[0].source, 2);
        }
        drop(handle);
    }

    #[test]
    fn stop_kills_the_process_group() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (tx, mut rx) = channel();
        let kind = SourceKind::Command {
            command: "echo up; sleep 30 & sleep 30; wait".into(),
        };
        let handle = spawn(spec(kind, dir.path().to_path_buf()), tx);
        let events = collect(&mut rx, |e| texts(e).iter().any(|(_, t)| t == "up"));
        assert!(!ended(&events));
        let started = std::time::Instant::now();
        handle.stop();
        let events = collect(&mut rx, ended);
        assert!(matches!(
            events.last(),
            Some(Event::Status {
                status: RunStatus::Stopped,
                ..
            })
        ));
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn missing_shell_fails() {
        let (tx, mut rx) = channel();
        let mut spec = spec(
            SourceKind::Command {
                command: "true".into(),
            },
            std::env::temp_dir(),
        );
        spec.shell = PathBuf::from("/nonexistent/shell");
        let _handle = spawn(spec, tx);
        let events = collect(&mut rx, ended);
        assert!(matches!(
            &events[..],
            [Event::Status {
                status: RunStatus::Failed(_),
                ..
            }]
        ));
    }

    #[test]
    fn files_are_tailed_until_stopped() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("logs")).expect("mkdir");
        std::fs::write(dir.path().join("logs/a.log"), "ERROR first\n").expect("write");
        let (tx, mut rx) = channel();
        let kind = SourceKind::File {
            path: "logs/*.log".into(),
        };
        let handle = spawn(spec(kind, dir.path().to_path_buf()), tx);
        let events = collect(&mut rx, |e| !texts(e).is_empty());
        let first = events.iter().find_map(|e| match e {
            Event::Lines { lines, .. } => lines.first().cloned(),
            Event::Status { .. } => None,
        });
        let first = first.expect("a line");
        assert_eq!(first.text, "ERROR first");
        assert_eq!(first.tag.as_deref(), Some("a.log"));
        assert_eq!(first.level, Some(crate::Level::Error));
        handle.stop();
        let events = collect(&mut rx, ended);
        assert!(matches!(
            events.last(),
            Some(Event::Status {
                status: RunStatus::Stopped,
                ..
            })
        ));
    }
}
