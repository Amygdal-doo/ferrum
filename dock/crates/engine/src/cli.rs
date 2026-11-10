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
    let output = tokio::process::Command::new(&cli.path)
        .args(args)
        .env("DOCKER_HOST", endpoint.docker_host())
        .env("PATH", cli.child_path())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| EngineError::Command(format!("could not run {}: {e}", cli.path.display())))?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    let text = crate::ansi::strip(text.trim());
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

#[cfg(test)]
mod tests {
    use super::*;

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
