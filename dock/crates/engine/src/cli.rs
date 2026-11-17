//! Locating and running the `docker` CLI (needed for `compose` and `exec -it`).
//! Apps started from Finder get a minimal `PATH`, so well-known install
//! locations are searched as well.

use crate::endpoint::Endpoint;
use crate::error::{EngineError, EngineResult};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerCli {
    pub path: PathBuf,
}

/// Install locations checked after `PATH`, relative to `$HOME` when not absolute.
const FALLBACKS: [&str; 7] = [
    "/usr/local/bin/docker",
    "/opt/homebrew/bin/docker",
    ".orbstack/bin/docker",
    ".docker/bin/docker",
    ".rd/bin/docker",
    "/Applications/Docker.app/Contents/Resources/bin/docker",
    "/Applications/OrbStack.app/Contents/MacOS/xbin/docker",
];

/// Extra directories appended to the child's `PATH` (credential helpers live there).
const EXTRA_PATH: [&str; 2] = ["/usr/local/bin", "/opt/homebrew/bin"];

/// Finds `docker` on `path_env`, then in the fallback locations.
#[must_use]
pub fn find_docker(
    path_env: Option<&std::ffi::OsStr>,
    home: Option<&Path>,
    exists: impl Fn(&Path) -> bool,
) -> Option<DockerCli> {
    let from_path = path_env
        .into_iter()
        .flat_map(std::env::split_paths)
        .map(|dir| dir.join("docker"));
    let fallbacks = FALLBACKS.iter().filter_map(|f| {
        if f.starts_with('/') {
            Some(PathBuf::from(f))
        } else {
            home.map(|h| h.join(f))
        }
    });
    from_path
        .chain(fallbacks)
        .find(|p| exists(p))
        .map(|path| DockerCli { path })
}

impl DockerCli {
    /// From the real environment.
    #[must_use]
    pub fn detect() -> Option<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        find_docker(
            std::env::var_os("PATH").as_deref(),
            home.as_deref(),
            Path::is_file,
        )
    }

    /// `PATH` for children: ours, plus the CLI's directory and common bin dirs.
    #[must_use]
    pub fn child_path(&self) -> OsString {
        let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        let extra = self
            .path
            .parent()
            .map(Path::to_path_buf)
            .into_iter()
            .chain(EXTRA_PATH.iter().map(PathBuf::from));
        for dir in extra {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
        std::env::join_paths(dirs).unwrap_or_default()
    }
}

/// Runs `docker <args>` against `endpoint` and returns stdout + stderr;
/// a non-zero exit becomes [`EngineError::Command`] with the output.
pub async fn run_capture(
    cli: &DockerCli,
    endpoint: &Endpoint,
    args: &[String],
) -> EngineResult<String> {
    run(cli, Some(endpoint), args, None).await
}

/// [`run_capture`] with `input` on stdin (`docker compose -f - config`).
pub async fn run_input(
    cli: &DockerCli,
    endpoint: &Endpoint,
    args: &[String],
    input: &str,
) -> EngineResult<String> {
    run(cli, Some(endpoint), args, Some(input)).await
}

/// Runs `docker <args>` against `endpoint`, calling `on_line` with every
/// line of stdout and stderr as it arrives (ANSI stripped; for a `\r`
/// progress line, its last state). A non-zero exit becomes
/// [`EngineError::Command`] with the last lines.
pub async fn run_lines(
    cli: &DockerCli,
    endpoint: &Endpoint,
    args: &[String],
    mut on_line: impl FnMut(String) + Send,
) -> EngineResult<()> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let mut child = command(cli, Some(endpoint), args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| EngineError::Command(format!("could not run {}: {e}", cli.path.display())))?;
    let mut out = child.stdout.take().map(|s| BufReader::new(s).lines());
    let mut err = child.stderr.take().map(|s| BufReader::new(s).lines());
    let mut tail: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    let mut emit = |line: String| {
        let line = crate::ansi::strip(line.rsplit('\r').find(|l| !l.is_empty()).unwrap_or(""));
        if tail.len() == 5 {
            tail.pop_front();
        }
        tail.push_back(line.clone());
        on_line(line);
    };
    while out.is_some() || err.is_some() {
        tokio::select! {
            line = async { out.as_mut()?.next_line().await.ok().flatten() }, if out.is_some() => {
                match line {
                    Some(line) => emit(line),
                    None => out = None,
                }
            }
            line = async { err.as_mut()?.next_line().await.ok().flatten() }, if err.is_some() => {
                match line {
                    Some(line) => emit(line),
                    None => err = None,
                }
            }
        }
    }
    let status = child
        .wait()
        .await
        .map_err(|e| EngineError::Command(e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        let last: Vec<String> = tail.into_iter().filter(|l| !l.trim().is_empty()).collect();
        Err(EngineError::Command(if last.is_empty() {
            format!("docker exited with {status}")
        } else {
            last.join("\n")
        }))
    }
}

/// Runs `docker <args>` on the CLI's own settings (`docker context …`):
/// without `DOCKER_HOST` and `DOCKER_CONTEXT`, which would override them.
pub async fn run_settings(cli: &DockerCli, args: &[String]) -> EngineResult<String> {
    run(cli, None, args, None).await
}

/// `docker <args>` with `DOCKER_HOST` set to `endpoint` (removed, with
/// `DOCKER_CONTEXT`, for `None`) and no input.
fn command(
    cli: &DockerCli,
    endpoint: Option<&Endpoint>,
    args: &[String],
) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(&cli.path);
    command
        .args(args)
        .env("PATH", cli.child_path())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    match endpoint {
        Some(endpoint) => command.env("DOCKER_HOST", endpoint.docker_host()),
        None => command
            .env_remove("DOCKER_HOST")
            .env_remove("DOCKER_CONTEXT"),
    };
    command
}

async fn run(
    cli: &DockerCli,
    endpoint: Option<&Endpoint>,
    args: &[String],
    input: Option<&str>,
) -> EngineResult<String> {
    let mut command = command(cli, endpoint, args);
    let spawn_error = |e: std::io::Error| {
        EngineError::Command(format!("could not run {}: {e}", cli.path.display()))
    };
    let output = match input {
        None => command.output().await.map_err(spawn_error)?,
        Some(input) => {
            use tokio::io::AsyncWriteExt;
            let mut child = command
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(spawn_error)?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(input.as_bytes())
                    .await
                    .map_err(|e| EngineError::Command(e.to_string()))?;
            }
            child
                .wait_with_output()
                .await
                .map_err(|e| EngineError::Command(e.to_string()))?
        }
    };
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    let text = strip_lines(&text);
    if output.status.success() {
        Ok(text)
    } else {
        Err(EngineError::Command(if text.is_empty() {
            format!("docker exited with {}", output.status)
        } else {
            text
        }))
    }
}

/// Strips ANSI escapes line by line: [`crate::ansi::strip`] also drops
/// control characters, newlines included, which would glue lines together
/// (e.g. the JSON lines of `compose ps`).
fn strip_lines(text: &str) -> String {
    text.trim()
        .lines()
        .map(crate::ansi::strip)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_lines_apart_when_stripping() {
        assert_eq!(
            strip_lines("\u{1b}[32m{\"a\":1}\u{1b}[0m\n{\"b\":2}\r\n"),
            "{\"a\":1}\n{\"b\":2}"
        );
    }

    #[test]
    fn prefers_path_then_fallbacks() {
        let path = std::env::join_paths(["/nope", "/tools"]).unwrap_or_default();
        let found = find_docker(Some(&path), None, |p| {
            p == Path::new("/tools/docker") || p == Path::new("/usr/local/bin/docker")
        });
        assert_eq!(found.map(|c| c.path), Some(PathBuf::from("/tools/docker")));

        let found = find_docker(Some(&path), Some(Path::new("/Users/me")), |p| {
            p == Path::new("/Users/me/.orbstack/bin/docker")
        });
        assert_eq!(
            found.map(|c| c.path),
            Some(PathBuf::from("/Users/me/.orbstack/bin/docker"))
        );
        assert_eq!(find_docker(None, None, |_| false), None);
    }

    #[test]
    fn child_path_includes_cli_directory() {
        let cli = DockerCli {
            path: PathBuf::from("/opt/odd/bin/docker"),
        };
        let path = cli.child_path();
        let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
        assert!(dirs.contains(&PathBuf::from("/opt/odd/bin")));
        assert!(dirs.contains(&PathBuf::from("/usr/local/bin")));
    }
}
