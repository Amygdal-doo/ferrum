//! Running shell commands for `run_command`. The default runs them as a
//! child process with a timeout and captures the output; a host can
//! supply its own runner (e.g. one that runs in its terminal).

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;

/// Output kept per command (the tail when longer).
pub const OUTPUT_LIMIT: usize = 30 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub output: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
}

#[async_trait::async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(&self, command: &str, cwd: &Path, timeout: Duration) -> CommandOutput;
}

/// `sh -c` (or `cmd /C`) in a child process.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessRunner;

#[async_trait::async_trait]
impl CommandRunner for ProcessRunner {
    async fn run(&self, command: &str, cwd: &Path, timeout: Duration) -> CommandOutput {
        let mut cmd = if cfg!(windows) {
            let mut c = tokio::process::Command::new("cmd");
            c.args(["/C", command]);
            c
        } else {
            let mut c = tokio::process::Command::new("sh");
            c.args(["-c", command]);
            c
        };
        cmd.current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("TERM", "dumb")
            .env("GIT_TERMINAL_PROMPT", "0")
            .kill_on_drop(true);
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                return CommandOutput {
                    output: format!("could not start: {e}"),
                    exit_code: None,
                    timed_out: false,
                };
            }
        };
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        // Both pipes at once: a child filling one while we wait on the
        // other would block forever.
        let read_all = async {
            let out = async {
                let mut v = Vec::new();
                if let Some(s) = stdout.as_mut() {
                    let _ = s.read_to_end(&mut v).await;
                }
                v
            };
            let err = async {
                let mut v = Vec::new();
                if let Some(s) = stderr.as_mut() {
                    let _ = s.read_to_end(&mut v).await;
                }
                v
            };
            tokio::join!(out, err)
        };
        let both = async {
            let (out, err) = read_all.await;
            let status = child.wait().await.ok();
            (out, err, status)
        };
        match tokio::time::timeout(timeout, both).await {
            Ok((out, err, status)) => {
                let mut text = String::from_utf8_lossy(&out).to_string();
                if !err.is_empty() {
                    if !text.is_empty() && !text.ends_with('\n') {
                        text.push('\n');
                    }
                    text.push_str(&String::from_utf8_lossy(&err));
                }
                CommandOutput {
                    output: tail(&text, OUTPUT_LIMIT),
                    exit_code: status.and_then(|s| s.code()),
                    timed_out: false,
                }
            }
            Err(_) => CommandOutput {
                output: format!("timed out after {}s", timeout.as_secs()),
                exit_code: None,
                timed_out: true,
            },
        }
    }
}

/// The last `limit` bytes of `text` (on a char boundary), marked.
pub fn tail(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut start = text.len() - limit;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    format!("…(output truncated)\n{}", &text[start..])
}
