//! One FerrumIDE process at a time. The running app listens on a local
//! socket (`~/.ferrum/editor/instance.sock`); a second `ferrumeditor`
//! call hands its paths over that socket and exits, and the running app
//! opens them (see [`crate::windows::route`]).
//!
//! The protocol is one JSON line each way: `{"targets": [{"path": …,
//! "line": …, "column": …}]}`, answered with `{"ok": true}`. A socket
//! nobody answers on is stale (a crash) and is replaced.
//! `FERRUM_EDITOR_NEW_INSTANCE=1` skips all of this.

use std::path::{Path, PathBuf};

use ferrumeditor_core::cli::OpenTarget;
use serde_json::{Value, json};

/// Set to start a separate process even when one is running.
pub const NEW_INSTANCE_ENV: &str = "FERRUM_EDITOR_NEW_INSTANCE";

/// The socket in the user directory (`FERRUM_EDITOR_HOME` moves it).
pub fn socket_path() -> Option<PathBuf> {
    ferrumeditor_core::paths::UserFiles::locate().map(|files| files.dir.join("instance.sock"))
}

pub fn encode(targets: &[OpenTarget]) -> String {
    let targets: Vec<Value> = targets
        .iter()
        .map(|t| json!({ "path": t.path, "line": t.line, "column": t.column }))
        .collect();
    json!({ "targets": targets }).to_string()
}

pub fn decode(line: &str) -> Option<Vec<OpenTarget>> {
    let value: Value = serde_json::from_str(line).ok()?;
    let index = |v: &Value| v.as_u64().and_then(|n| usize::try_from(n).ok());
    value["targets"]
        .as_array()?
        .iter()
        .map(|t| {
            Some(OpenTarget {
                path: PathBuf::from(t["path"].as_str()?),
                line: index(&t["line"]),
                column: index(&t["column"]),
            })
        })
        .collect()
}

#[cfg(unix)]
mod imp {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::Path;
    use std::time::Duration;

    use ferrumeditor_core::cli::OpenTarget;
    use futures::channel::mpsc::UnboundedSender;

    /// Hands `targets` to a running instance. False when none answers.
    pub fn forward(socket: &Path, targets: &[OpenTarget]) -> bool {
        let Ok(mut stream) = UnixStream::connect(socket) else {
            return false;
        };
        let timeout = Some(Duration::from_secs(5));
        if stream.set_read_timeout(timeout).is_err() || stream.set_write_timeout(timeout).is_err() {
            return false;
        }
        if writeln!(stream, "{}", super::encode(targets)).is_err() {
            return false;
        }
        let mut reply = String::new();
        BufReader::new(&stream).read_line(&mut reply).is_ok() && reply.contains("\"ok\":true")
    }

    /// Listens on `socket` (replacing a stale one) and sends every request
    /// to `requests`. The thread ends with the process.
    pub fn listen(
        socket: &Path,
        requests: UnboundedSender<Vec<OpenTarget>>,
    ) -> std::io::Result<()> {
        if let Some(dir) = socket.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Only reached when connecting failed: whatever is there is stale.
        let _ = std::fs::remove_file(socket);
        let listener = UnixListener::bind(socket)?;
        std::thread::Builder::new()
            .name("ferrumeditor-instance".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let mut line = String::new();
                    if BufReader::new(&stream).read_line(&mut line).is_err() {
                        continue;
                    }
                    let ok = match super::decode(&line) {
                        Some(targets) => requests.unbounded_send(targets).is_ok(),
                        None => false,
                    };
                    let mut stream = stream;
                    let _ = writeln!(stream, "{}", serde_json::json!({ "ok": ok }));
                }
            })?;
        Ok(())
    }
}

#[cfg(not(unix))]
mod imp {
    use std::path::Path;

    use ferrumeditor_core::cli::OpenTarget;
    use futures::channel::mpsc::UnboundedSender;

    pub fn forward(_: &Path, _: &[OpenTarget]) -> bool {
        false
    }

    pub fn listen(_: &Path, _: UnboundedSender<Vec<OpenTarget>>) -> std::io::Result<()> {
        Ok(())
    }
}

/// Hands `targets` to a running instance; true when it took them (this
/// process should exit).
pub fn forward(socket: &Path, targets: &[OpenTarget]) -> bool {
    imp::forward(socket, targets)
}

pub use imp::listen;

/// Removes the socket when this process quits (if it is still ours).
pub fn cleanup(socket: &Path) {
    let _ = std::fs::remove_file(socket);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip() {
        let targets = vec![
            OpenTarget {
                path: PathBuf::from("/p/src/main.rs"),
                line: Some(4),
                column: None,
            },
            OpenTarget {
                path: PathBuf::from("/p"),
                line: None,
                column: None,
            },
        ];
        assert_eq!(decode(&encode(&targets)), Some(targets));
        assert_eq!(decode(&encode(&[])), Some(Vec::new()));
        assert_eq!(decode("nonsense"), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_second_call_hands_its_paths_to_the_first() {
        use futures::StreamExt as _;
        let dir = tempfile::tempdir().expect("tempdir");
        let socket = dir.path().join("instance.sock");
        let target = OpenTarget {
            path: dir.path().join("a.rs"),
            line: Some(3),
            column: Some(2),
        };
        // Nobody listens yet.
        assert!(!forward(&socket, std::slice::from_ref(&target)));
        // A stale file left by a crash is replaced.
        std::fs::write(&socket, "").expect("stale");
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        listen(&socket, tx).expect("listen");
        assert!(forward(&socket, std::slice::from_ref(&target)));
        let received = futures::executor::block_on(rx.next()).expect("request");
        assert_eq!(received, vec![target]);
        cleanup(&socket);
        assert!(!socket.exists());
    }
}
