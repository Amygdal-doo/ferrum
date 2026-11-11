//! "Open shell": `docker exec -it <id> sh` in FerrumTerm when it is installed,
//! else in Terminal.app.
//!
//! FerrumTerm counts as installed when both its GUI bundle and the `fterm` CLI
//! exist. `fterm window --name <container> --cwd ~ -- <cmd>` opens a new window (tab) in the running FerrumTerm server, starting it on
//! demand; then `open -a FerrumTerm.app` brings the GUI forward.

use crate::cli::DockerCli;
use crate::endpoint::Endpoint;
use crate::error::{EngineError, EngineResult};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellPlan {
    FerrumTerm {
        fterm: PathBuf,
        args: Vec<String>,
        app: PathBuf,
    },
    Terminal {
        /// AppleScript for `osascript -e`.
        script: String,
    },
}

impl ShellPlan {
    #[must_use]
    pub fn target_name(&self) -> &'static str {
        match self {
            Self::FerrumTerm { .. } => "FerrumTerm",
            Self::Terminal { .. } => "Terminal",
        }
    }
}

/// `docker -H <endpoint> exec -it <id> sh`.
#[must_use]
pub fn exec_command(cli: &DockerCli, endpoint: &Endpoint, container_id: &str) -> Vec<String> {
    vec![
        cli.path.display().to_string(),
        "-H".to_owned(),
        endpoint.docker_host(),
        "exec".to_owned(),
        "-it".to_owned(),
        container_id.to_owned(),
        "sh".to_owned(),
    ]
}

/// The FerrumTerm GUI bundle and `fterm`, when both are installed.
#[must_use]
pub fn find_ferrumterm(home: &Path, exists: impl Fn(&Path) -> bool) -> Option<(PathBuf, PathBuf)> {
    let fterm = home.join(".ferrum/bin/fterm");
    let app = [
        home.join("Applications/Ferrum/FerrumTerm.app"),
        PathBuf::from("/Applications/Ferrum/FerrumTerm.app"),
        PathBuf::from("/Applications/FerrumTerm.app"),
    ]
    .into_iter()
    .find(|p| exists(p))?;
    exists(&fterm).then_some((fterm, app))
}

/// Chooses FerrumTerm (`ferrumterm` = bundle + fterm) or Terminal.app.
#[must_use]
pub fn plan(
    command: &[String],
    title: &str,
    home: &Path,
    ferrumterm: Option<(PathBuf, PathBuf)>,
) -> ShellPlan {
    match ferrumterm {
        Some((fterm, app)) => {
            let mut args = vec![
                "window".to_owned(),
                "--name".to_owned(),
                title.to_owned(),
                "--cwd".to_owned(),
                home.display().to_string(),
                "--".to_owned(),
            ];
            args.extend(command.iter().cloned());
            ShellPlan::FerrumTerm { fterm, args, app }
        }
        None => {
            let line = command
                .iter()
                .map(|a| shell_quote(a))
                .collect::<Vec<_>>()
                .join(" ");
            ShellPlan::Terminal {
                script: format!(
                    "tell application \"Terminal\"\n    activate\n    do script \"{}\"\nend tell",
                    applescript_string(&line)
                ),
            }
        }
    }
}

/// POSIX shell quoting: bare when safe, else single quotes.
#[must_use]
pub fn shell_quote(arg: &str) -> String {
    let safe = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/-_.:=@,+%".contains(c));
    if safe {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

/// Escapes text for an AppleScript string literal.
#[must_use]
pub fn applescript_string(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Runs the plan. `fterm` is awaited (it returns once the window exists).
pub async fn launch(plan: &ShellPlan) -> EngineResult<()> {
    match plan {
        ShellPlan::FerrumTerm { fterm, args, app } => {
            run(fterm, args.iter().map(String::as_str)).await?;
            run(Path::new("open"), ["-a", &app.display().to_string()]).await
        }
        ShellPlan::Terminal { script } => {
            if !cfg!(target_os = "macos") {
                return Err(EngineError::Command(
                    "no terminal found: install FerrumTerm to open a shell".into(),
                ));
            }
            run(Path::new("osascript"), ["-e", script.as_str()]).await
        }
    }
}

async fn run<'a>(program: &Path, args: impl IntoIterator<Item = &'a str>) -> EngineResult<()> {
    let output = tokio::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| EngineError::Command(format!("could not run {}: {e}", program.display())))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(EngineError::Command(if stderr.is_empty() {
            format!("{} exited with {}", program.display(), output.status)
        } else {
            stderr
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command() -> Vec<String> {
        let cli = DockerCli {
            path: PathBuf::from("/usr/local/bin/docker"),
        };
        exec_command(
            &cli,
            &Endpoint::Unix("/Users/me/.colima/default/docker.sock".into()),
            "abc123",
        )
    }

    #[test]
    fn builds_the_exec_command_for_the_connected_engine() {
        assert_eq!(
            command().join(" "),
            "/usr/local/bin/docker -H unix:///Users/me/.colima/default/docker.sock exec -it abc123 sh"
        );
    }

    #[test]
    fn uses_ferrumterm_when_bundle_and_cli_exist() {
        let home = Path::new("/Users/me");
        let installed = |p: &Path| {
            p == Path::new("/Users/me/.ferrum/bin/fterm")
                || p == Path::new("/Users/me/Applications/Ferrum/FerrumTerm.app")
        };
        let found = find_ferrumterm(home, installed);
        let plan = plan(&command(), "web-db-1", home, found);
        let ShellPlan::FerrumTerm { fterm, args, app } = plan else {
            panic!("expected FerrumTerm");
        };
        assert_eq!(fterm, Path::new("/Users/me/.ferrum/bin/fterm"));
        assert_eq!(
            app,
            Path::new("/Users/me/Applications/Ferrum/FerrumTerm.app")
        );
        assert_eq!(
            args[..6].join(" "),
            "window --name web-db-1 --cwd /Users/me --"
        );
        assert_eq!(args[6..], command()[..]);

        let cli_only = |p: &Path| p == Path::new("/Users/me/.ferrum/bin/fterm");
        assert_eq!(
            find_ferrumterm(home, cli_only),
            None,
            "fterm alone is not enough"
        );
    }

    #[test]
    fn falls_back_to_terminal_app_with_quoting() {
        let mut cmd = command();
        cmd[5] = "it's \"x\"".to_owned();
        let ShellPlan::Terminal { script } = plan(&cmd, "x", Path::new("/h"), None) else {
            panic!("expected Terminal");
        };
        assert!(script.starts_with("tell application \"Terminal\""));
        assert!(script.contains(
            r#"do script "/usr/local/bin/docker -H unix:///Users/me/.colima/default/docker.sock exec -it 'it'\\''s \"x\"' sh""#
        ));
    }

    #[test]
    fn quotes_shell_arguments() {
        assert_eq!(shell_quote("abc-1.2"), "abc-1.2");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(applescript_string(r#"a\"b"#), r#"a\\\"b"#);
    }
}
