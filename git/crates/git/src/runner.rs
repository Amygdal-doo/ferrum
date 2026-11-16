//! Spawning git with a consistent, non-interactive environment.

use crate::error::{GitError, GitResult, sanitize};
use crate::ssh::NetEnv;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

/// The git binary plus environment applied to every invocation. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Git {
    binary: PathBuf,
    env: Vec<(String, OsString)>,
    askpass: Option<PathBuf>,
}

impl Default for Git {
    fn default() -> Self {
        Self::new()
    }
}

impl Git {
    /// Uses `git` from `PATH`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            binary: PathBuf::from("git"),
            env: Vec::new(),
            askpass: None,
        }
    }

    #[must_use]
    pub fn with_binary(mut self, binary: impl Into<PathBuf>) -> Self {
        self.binary = binary.into();
        self
    }

    /// Extra environment for every command (tests isolate global config this way).
    #[must_use]
    pub fn with_env(mut self, key: &str, value: impl Into<OsString>) -> Self {
        self.env.push((key.to_owned(), value.into()));
        self
    }

    /// Program ssh/git call for passphrase prompts (the FerrumGIT binary itself).
    #[must_use]
    pub fn with_askpass(mut self, program: impl Into<PathBuf>) -> Self {
        self.askpass = Some(program.into());
        self
    }

    #[must_use]
    pub fn askpass(&self) -> Option<&Path> {
        self.askpass.as_deref()
    }

    /// `git version 2.50.1` → `2.50.1`.
    pub async fn version(&self) -> GitResult<String> {
        let out = self.run(Cmd::new(["version"])).await?;
        let text = out.stdout_str();
        Ok(text.trim().trim_start_matches("git version ").to_owned())
    }

    pub async fn run(&self, cmd: Cmd) -> GitResult<Output> {
        let mut command = Command::new(&self.binary);
        command
            .args([
                "-c",
                "core.quotepath=false",
                "-c",
                "color.ui=never",
                "-c",
                "log.showSignature=false",
            ])
            .args(&cmd.args)
            .stdin(if cmd.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            // Never wait for a terminal or an editor.
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .env("GIT_SEQUENCE_EDITOR", "true")
            .env("GIT_MERGE_AUTOEDIT", "no")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("LC_ALL", "C");
        if let Some(dir) = &cmd.cwd {
            command.current_dir(dir);
        }
        for (k, v) in &self.env {
            command.env(k, v);
        }
        for (k, v) in &cmd.env {
            command.env(k, v);
        }
        if let Some(net) = &cmd.net {
            for (k, v) in net.vars() {
                command.env(k, v);
            }
        }
        let child = command.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                GitError::NotInstalled(self.binary.display().to_string())
            }
            _ => GitError::Spawn(e.to_string()),
        })?;
        let label = cmd
            .args
            .first()
            .map(|a| a.to_string_lossy().into_owned())
            .unwrap_or_default();
        let output = wait(child, cmd.stdin, cmd.cancel).await?;
        let code = output.status.code().unwrap_or(-1);
        let out = Output {
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            code,
        };
        if out.code == 0 || cmd.ok_codes.contains(&out.code) {
            Ok(out)
        } else {
            tracing::debug!(command = %label, code, "git command failed");
            Err(GitError::Failed {
                code,
                message: out.failure_message(),
            })
        }
    }
}

async fn wait(
    mut child: tokio::process::Child,
    stdin: Option<Vec<u8>>,
    cancel: Option<CancellationToken>,
) -> GitResult<std::process::Output> {
    if let (Some(bytes), Some(mut pipe)) = (stdin, child.stdin.take()) {
        // A broken pipe means git exited early; its status explains why.
        let _ = pipe.write_all(&bytes).await;
        drop(pipe);
    }
    let io = |e: std::io::Error| GitError::Spawn(e.to_string());
    match cancel {
        None => child.wait_with_output().await.map_err(io),
        Some(token) => tokio::select! {
            out = child.wait_with_output() => out.map_err(io),
            // Dropping the future kills the child (kill_on_drop).
            () = token.cancelled() => Err(GitError::Cancelled),
        },
    }
}

/// One git invocation.
#[derive(Debug, Default)]
pub struct Cmd {
    args: Vec<OsString>,
    cwd: Option<PathBuf>,
    stdin: Option<Vec<u8>>,
    ok_codes: Vec<i32>,
    cancel: Option<CancellationToken>,
    net: Option<NetEnv>,
    env: Vec<(String, String)>,
}

impl Cmd {
    pub fn new<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        Self {
            args: args.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn arg(mut self, a: impl Into<OsString>) -> Self {
        self.args.push(a.into());
        self
    }

    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Global options (`-c k=v`) must precede the subcommand.
    #[must_use]
    pub fn prepend<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let mut v: Vec<OsString> = args.into_iter().map(Into::into).collect();
        v.append(&mut self.args);
        self.args = v;
        self
    }

    /// Commit identity from the profile. Uses the author/committer environment
    /// variables, which take precedence over both config and inherited env.
    #[must_use]
    pub fn identity(mut self, identity: Option<(&str, &str)>) -> Self {
        if let Some((name, email)) = identity {
            for role in ["AUTHOR", "COMMITTER"] {
                self.env.push((format!("GIT_{role}_NAME"), name.to_owned()));
                self.env
                    .push((format!("GIT_{role}_EMAIL"), email.to_owned()));
            }
        }
        self
    }

    /// One environment variable for this command only.
    #[must_use]
    pub fn env(mut self, key: &str, value: impl Into<String>) -> Self {
        self.env.push((key.to_owned(), value.into()));
        self
    }

    #[must_use]
    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    #[must_use]
    pub fn stdin(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(bytes.into());
        self
    }

    /// Exit codes besides 0 that are not failures (e.g. `diff --no-index` → 1).
    #[must_use]
    pub fn ok_codes(mut self, codes: &[i32]) -> Self {
        self.ok_codes = codes.to_vec();
        self
    }

    #[must_use]
    pub fn cancel(mut self, token: Option<CancellationToken>) -> Self {
        self.cancel = token;
        self
    }

    #[must_use]
    pub fn net(mut self, env: Option<NetEnv>) -> Self {
        self.net = env;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Output {
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub code: i32,
}

impl Output {
    #[must_use]
    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// The most useful lines of output for an error, without `hint:` noise.
    #[must_use]
    pub fn failure_message(&self) -> String {
        let stdout = self.stdout_str();
        let mut lines: Vec<&str> = self
            .stderr
            .lines()
            .chain(
                stdout
                    .lines()
                    .filter(|l| l.starts_with("CONFLICT") || l.contains("conflict")),
            )
            .map(str::trim_end)
            .filter(|l| !l.trim().is_empty() && !l.starts_with("hint:"))
            .collect();
        if lines.is_empty() {
            lines = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
        }
        let keep = lines.len().saturating_sub(12);
        let msg = lines[keep..].join("\n");
        let msg = if msg.is_empty() {
            format!("git exited with status {}", self.code)
        } else {
            msg
        };
        sanitize(&msg)
    }

    /// Last informative line (for success notices).
    #[must_use]
    pub fn summary(&self) -> Option<String> {
        let stdout = self.stdout_str();
        stdout
            .lines()
            .chain(self.stderr.lines())
            .map(str::trim)
            .rfind(|l| !l.is_empty() && !l.starts_with("hint:"))
            .map(sanitize)
    }
}
