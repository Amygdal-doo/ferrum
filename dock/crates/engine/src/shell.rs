//! "Open shell": `docker exec -it <id> <shell>` in FerrumTerm when it is
//! installed, else in Terminal.app (macOS) or the first terminal emulator
//! found on `PATH` (Linux).
//!
//! FerrumTerm counts as installed when both its GUI bundle and the `fterm` CLI
//! exist. `fterm window --name <container> --cwd ~ -- <cmd>` opens a new window (tab) in the running FerrumTerm server, starting it on
//! demand; then `open -a FerrumTerm.app` brings the GUI forward.

use crate::cli::{DockerCli, run_capture};
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
    /// A Linux terminal emulator: `program <args…>` (args end with the command).
    Linux { program: PathBuf, args: Vec<String> },
}

impl ShellPlan {
    #[must_use]
    pub fn target_name(&self) -> &'static str {
        match self {
            Self::FerrumTerm { .. } => "FerrumTerm",
            Self::Terminal { .. } => "Terminal",
            Self::Linux { .. } => "terminal",
        }
    }
}

/// Which shell to start in the container. Every choice falls back to `sh`
/// when the shell is missing, so the window never closes at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Shell {
    /// bash, else zsh, else sh.
    #[default]
    Auto,
    Bash,
    Zsh,
    Sh,
}

impl Shell {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Bash, Self::Zsh, Self::Sh];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Sh => "sh",
        }
    }

    /// What runs after `exec -it <id>`.
    #[must_use]
    pub fn argv(self) -> Vec<String> {
        let prefer = |shells: &[&str]| {
            let tries: String = shells
                .iter()
                .map(|s| format!("command -v {s} >/dev/null 2>&1 && exec {s}; "))
                .collect();
            vec!["sh".to_owned(), "-c".to_owned(), format!("{tries}exec sh")]
        };
        match self {
            Self::Auto => prefer(&["bash", "zsh"]),
            Self::Bash => prefer(&["bash"]),
            Self::Zsh => prefer(&["zsh"]),
            Self::Sh => vec!["sh".to_owned()],
        }
    }
}

/// Prints the shells a container has, one per line.
const DETECT_SCRIPT: &str =
    "for s in bash zsh sh; do command -v $s >/dev/null 2>&1 && echo $s; done";

/// Shells listed by [`DETECT_SCRIPT`]; `sh` is assumed when nothing answered.
#[must_use]
pub fn parse_detected(output: &str) -> Vec<Shell> {
    let mut shells: Vec<Shell> = output
        .lines()
        .filter_map(|l| match l.trim() {
            "bash" => Some(Shell::Bash),
            "zsh" => Some(Shell::Zsh),
            "sh" => Some(Shell::Sh),
            _ => None,
        })
        .collect();
    if !shells.contains(&Shell::Sh) {
        shells.push(Shell::Sh);
    }
    shells
}

/// Asks the running container which shells it has (`docker exec <id> sh -c …`).
pub async fn detect_shells(
    cli: &DockerCli,
    endpoint: &Endpoint,
    container_id: &str,
) -> EngineResult<Vec<Shell>> {
    let args = ["exec", container_id, "sh", "-c", DETECT_SCRIPT].map(str::to_owned);
    Ok(parse_detected(&run_capture(cli, endpoint, &args).await?))
}

/// `docker -H <endpoint> exec -it <id> <shell…>`.
#[must_use]
pub fn exec_command(
    cli: &DockerCli,
    endpoint: &Endpoint,
    container_id: &str,
    shell: Shell,
) -> Vec<String> {
    let mut command = vec![
        cli.path.display().to_string(),
        "-H".to_owned(),
        endpoint.docker_host(),
        "exec".to_owned(),
        "-it".to_owned(),
        container_id.to_owned(),
    ];
    command.extend(shell.argv());
    command
}

/// Linux terminal emulators and the flag that precedes the command, in order of preference.
const LINUX_TERMINALS: [(&str, &[&str]); 8] = [
    ("x-terminal-emulator", &["-e"]),
    ("gnome-terminal", &["--"]),
    ("konsole", &["-e"]),
    ("xfce4-terminal", &["-x"]),
    ("alacritty", &["-e"]),
    ("kitty", &[]),
    ("wezterm", &["start", "--"]),
    ("xterm", &["-e"]),
];

/// The first terminal emulator on `path_env`, with the arguments before the command.
#[must_use]
pub fn find_linux_terminal(
    path_env: Option<&std::ffi::OsStr>,
    exists: impl Fn(&Path) -> bool,
) -> Option<(PathBuf, Vec<String>)> {
    let dirs: Vec<PathBuf> = path_env
        .map(|p| std::env::split_paths(p).collect())
        .unwrap_or_default();
    LINUX_TERMINALS.iter().find_map(|(name, prefix)| {
        dirs.iter()
            .map(|d| d.join(name))
            .find(|p| exists(p))
            .map(|program| (program, prefix.iter().map(|s| (*s).to_owned()).collect()))
    })
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

/// Chooses FerrumTerm (`ferrumterm` = bundle + fterm), then a Linux terminal
/// (`linux`, from [`find_linux_terminal`]; pass `None` on macOS), else Terminal.app.
#[must_use]
pub fn plan(
    command: &[String],
    title: &str,
    home: &Path,
    ferrumterm: Option<(PathBuf, PathBuf)>,
    linux: Option<(PathBuf, Vec<String>)>,
) -> ShellPlan {
    if let (None, Some((program, mut args))) = (&ferrumterm, linux) {
        args.extend(command.iter().cloned());
        return ShellPlan::Linux { program, args };
    }
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
        ShellPlan::Linux { program, args } => {
            // The emulator runs until its window closes: don't wait for it.
            let mut child = tokio::process::Command::new(program)
                .args(args)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|e| {
                    EngineError::Command(format!("could not run {}: {e}", program.display()))
                })?;
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            Ok(())
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
            Shell::Sh,
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
        let linux = Some((PathBuf::from("/usr/bin/xterm"), vec!["-e".to_owned()]));
        let plan = plan(&command(), "web-db-1", home, found, linux);
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
        let ShellPlan::Terminal { script } = plan(&cmd, "x", Path::new("/h"), None, None) else {
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

    #[test]
    fn shells_fall_back_to_sh() {
        assert_eq!(Shell::Sh.argv(), ["sh"]);
        let bash = Shell::Bash.argv();
        assert_eq!(bash[..2], ["sh", "-c"]);
        assert_eq!(
            bash[2],
            "command -v bash >/dev/null 2>&1 && exec bash; exec sh"
        );
        assert!(Shell::Auto.argv()[2].contains("exec bash; command -v zsh"));
        let cli = DockerCli {
            path: PathBuf::from("docker"),
        };
        let cmd = exec_command(&cli, &Endpoint::Tcp("h:1".into()), "id", Shell::Zsh);
        assert_eq!(cmd[..6].join(" "), "docker -H tcp://h:1 exec -it id");
        assert_eq!(parse_detected("bash\nsh\n"), [Shell::Bash, Shell::Sh]);
        assert_eq!(parse_detected("OCI runtime exec failed"), [Shell::Sh]);
    }

    #[test]
    fn finds_a_linux_terminal_and_plans_it() {
        let path = std::env::join_paths(["/usr/local/bin", "/usr/bin"]).unwrap_or_default();
        let found = find_linux_terminal(Some(&path), |p| {
            p == Path::new("/usr/bin/konsole") || p == Path::new("/usr/bin/xterm")
        });
        assert_eq!(
            found,
            Some((PathBuf::from("/usr/bin/konsole"), vec!["-e".to_owned()]))
        );
        assert_eq!(find_linux_terminal(None, |_| true), None);
        let ShellPlan::Linux { program, args } =
            plan(&command(), "x", Path::new("/h"), None, found)
        else {
            panic!("expected a Linux terminal");
        };
        assert_eq!(program, Path::new("/usr/bin/konsole"));
        assert_eq!(args[0], "-e");
        assert_eq!(args[1..], command()[..]);
    }
}
