//! AI coding agents run in the integrated terminal: the built-in presets
//! (Claude Code, Codex, Gemini CLI) and custom CLIs from `[[agents]]`,
//! finding their programs, the command line a session starts with, and its
//! status. [`snapshot`] records the working tree when a session starts and
//! [`review`] turns what changed since into hunks to accept or reject.
//!
//! API keys in `env` go to the agent's process only: they are never
//! logged, and [`masked`] is all a UI may show of them.

pub mod review;
pub mod snapshot;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::AgentConfig;
use crate::lsp::servers::find_program;

/// How a session's starting prompt reaches the CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptStyle {
    /// The CLI takes no prompt on its command line.
    None,
    /// `agent "prompt"`.
    Positional,
    /// `agent <flag> "prompt"`, e.g. `gemini -i "prompt"`.
    Flag(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDef {
    /// `claude`, `codex`, `gemini` or the `[[agents]]` name.
    pub id: String,
    /// Shown in the panel and tabs.
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    /// Extra environment (API keys). See [`masked`].
    pub env: BTreeMap<String, String>,
    pub prompt: PromptStyle,
    /// What to run when the program is missing.
    pub install_hint: Option<String>,
    /// npm package for a one-click install.
    pub npm_package: Option<String>,
    pub builtin: bool,
}

impl AgentDef {
    fn preset(id: &str, name: &str, prompt: PromptStyle, package: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            command: id.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            prompt,
            install_hint: Some(format!("npm i -g {package}")),
            npm_package: Some(package.into()),
            builtin: true,
        }
    }

    /// Program and arguments for a session, with `prompt` (the current
    /// file or selection) passed the way the CLI takes it.
    pub fn command_line(&self, prompt: Option<&str>) -> Vec<String> {
        let mut line = vec![self.command.clone()];
        line.extend(self.args.iter().cloned());
        match (prompt.filter(|p| !p.trim().is_empty()), &self.prompt) {
            (Some(prompt), PromptStyle::Positional) => line.push(prompt.to_string()),
            (Some(prompt), PromptStyle::Flag(flag)) => {
                line.push(flag.clone());
                line.push(prompt.to_string());
            }
            _ => {}
        }
        line
    }

    pub fn takes_prompt(&self) -> bool {
        self.prompt != PromptStyle::None
    }
}

/// The built-in presets.
pub fn builtin_agents() -> Vec<AgentDef> {
    vec![
        AgentDef::preset(
            "claude",
            "Claude Code",
            PromptStyle::Positional,
            "@anthropic-ai/claude-code",
        ),
        AgentDef::preset("codex", "Codex", PromptStyle::Positional, "@openai/codex"),
        AgentDef::preset(
            "gemini",
            "Gemini CLI",
            PromptStyle::Flag("-i".into()),
            "@google/gemini-cli",
        ),
    ]
}

fn prompt_style(value: Option<&str>) -> Option<PromptStyle> {
    let value = value?.trim();
    Some(match value {
        "" | "none" => PromptStyle::None,
        "positional" => PromptStyle::Positional,
        flag => PromptStyle::Flag(flag.to_string()),
    })
}

/// Built-ins merged with `[[agents]]`: an entry named like a built-in
/// changes it, `enabled = false` hides it, any other name adds an agent.
pub fn all_agents(config: &[AgentConfig]) -> Vec<AgentDef> {
    let mut agents = builtin_agents();
    for entry in config {
        let id = entry.name.trim();
        if id.is_empty() {
            continue;
        }
        if entry.enabled == Some(false) {
            agents.retain(|a| a.id != id);
            continue;
        }
        let at = match agents.iter().position(|a| a.id == id) {
            Some(at) => at,
            None => {
                agents.push(AgentDef {
                    id: id.to_string(),
                    name: id.to_string(),
                    command: id.to_string(),
                    args: Vec::new(),
                    env: BTreeMap::new(),
                    prompt: PromptStyle::None,
                    install_hint: None,
                    npm_package: None,
                    builtin: false,
                });
                agents.len() - 1
            }
        };
        let agent = &mut agents[at];
        if let Some(command) = entry.command.as_deref().map(str::trim)
            && !command.is_empty()
        {
            agent.command = command.to_string();
        }
        if let Some(args) = &entry.args {
            agent.args = args.clone();
        }
        agent.env.extend(entry.env.0.clone());
        if let Some(style) = prompt_style(entry.prompt.as_deref()) {
            agent.prompt = style;
        }
        if let Some(hint) = &entry.install {
            agent.install_hint = Some(hint.clone());
        }
    }
    agents
}

/// What a settings UI may show for a secret value.
pub fn masked(value: &str) -> &'static str {
    if value.is_empty() {
        "(empty)"
    } else {
        "••••••••"
    }
}

/// `PATH` for finding agents: `base` (the login shell's, see
/// [`crate::lsp::servers::search_path`]) plus `~/.ferrum/bin`,
/// `~/.local/bin` and npm's global bin, then agents the editor installed
/// into `servers` (one-click installs).
pub fn search_path(
    base: &str,
    home: Option<&Path>,
    npm_global_bin: Option<&Path>,
    servers: Option<&Path>,
) -> String {
    let mut dirs: Vec<PathBuf> = std::env::split_paths(base).collect();
    if let Some(home) = home {
        dirs.push(home.join(".ferrum").join("bin"));
        dirs.push(home.join(".local").join("bin"));
    }
    dirs.extend(npm_global_bin.map(Path::to_path_buf));
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| !d.as_os_str().is_empty() && seen.insert(d.clone()));
    let joined = std::env::join_paths(dirs)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| base.to_string());
    match servers {
        Some(servers) => crate::lsp::install::with_managed(&joined, servers),
        None => joined,
    }
}

/// npm's global `bin` folder (`npm prefix -g`), asked with a timeout.
pub fn npm_global_bin(path_env: &str) -> Option<PathBuf> {
    let npm = find_program("npm", path_env)?;
    let mut child = std::process::Command::new(npm)
        .args(["prefix", "-g"])
        .env("PATH", path_env)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(4);
    while child.try_wait().ok()?.is_none() {
        if std::time::Instant::now() > deadline {
            child.kill().ok();
            child.wait().ok();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut out = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut out).ok()?;
    let prefix = PathBuf::from(out.trim());
    (!out.trim().is_empty()).then(|| {
        if cfg!(windows) {
            prefix
        } else {
            prefix.join("bin")
        }
    })
}

/// The agent's program on `path_env`, if installed.
pub fn locate(agent: &AgentDef, path_env: &str) -> Option<PathBuf> {
    find_program(&agent.command, path_env)
}

/// A shell command line for `words` (each quoted when needed).
pub fn shell_line(words: &[String]) -> String {
    words
        .iter()
        .map(|w| shell_quote(w))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn shell_quote(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:@%+,".contains(c))
    {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// The starting prompt for "with context": the file, and the selection
/// when there is one (capped).
pub fn context_prompt(relative_path: &str, selection: Option<(usize, usize, &str)>) -> String {
    const LIMIT: usize = 8 * 1024;
    match selection {
        Some((first, last, text)) if !text.trim().is_empty() => {
            let mut cut = text.len().min(LIMIT);
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            let more = if cut < text.len() { "\n…" } else { "" };
            format!(
                "I'm looking at {relative_path}, lines {first}-{last}:\n```\n{}{more}\n```\n",
                &text[..cut]
            )
        }
        _ => format!("I'm looking at {relative_path} in my editor."),
    }
}

/// What a session is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    /// Taking the snapshot, starting the process.
    Starting,
    Running,
    /// Running, but no output for a while: probably waiting for you.
    Waiting,
    Exited(i32),
    /// Stopped from the editor.
    Stopped,
}

impl AgentStatus {
    /// Running or exited, from the process and its last output.
    pub fn of(exit_code: Option<i32>, stopped: bool, idle: Duration, threshold: Duration) -> Self {
        match (stopped, exit_code) {
            (true, _) => AgentStatus::Stopped,
            (false, Some(code)) => AgentStatus::Exited(code),
            (false, None) if idle >= threshold => AgentStatus::Waiting,
            (false, None) => AgentStatus::Running,
        }
    }

    pub fn label(self) -> String {
        match self {
            AgentStatus::Starting => "starting".into(),
            AgentStatus::Running => "running".into(),
            AgentStatus::Waiting => "waiting for input".into(),
            AgentStatus::Exited(code) => format!("exited ({code})"),
            AgentStatus::Stopped => "stopped".into(),
        }
    }

    pub fn is_live(self) -> bool {
        matches!(
            self,
            AgentStatus::Starting | AgentStatus::Running | AgentStatus::Waiting
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SecretEnv;

    #[test]
    fn presets_and_config_merge() {
        let mut env = BTreeMap::new();
        env.insert("ANTHROPIC_API_KEY".to_string(), "sk-ant-x".to_string());
        let config = vec![
            AgentConfig {
                name: "claude".into(),
                env: SecretEnv(env),
                ..Default::default()
            },
            AgentConfig {
                name: "aider".into(),
                args: Some(vec!["--no-auto-commits".into()]),
                prompt: Some("--message".into()),
                ..Default::default()
            },
            AgentConfig {
                name: "codex".into(),
                enabled: Some(false),
                ..Default::default()
            },
        ];
        let agents = all_agents(&config);
        let ids: Vec<&str> = agents.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["claude", "gemini", "aider"]);
        assert_eq!(agents[0].env["ANTHROPIC_API_KEY"], "sk-ant-x");
        assert_eq!(
            agents[0].install_hint.as_deref(),
            Some("npm i -g @anthropic-ai/claude-code")
        );
        assert_eq!(
            agents[2].command_line(Some("fix it")),
            ["aider", "--no-auto-commits", "--message", "fix it"]
        );
        assert_eq!(agents[1].command_line(Some("hi")), ["gemini", "-i", "hi"]);
        assert_eq!(agents[0].command_line(None), ["claude"]);
        assert!(!agents[2].builtin && agents[0].builtin);
        let plain = all_agents(&[AgentConfig {
            name: "x".into(),
            ..Default::default()
        }]);
        assert_eq!(plain[3].command_line(Some("ignored")), ["x"]);
    }

    #[test]
    fn secrets_are_masked_and_lines_quoted() {
        assert_eq!(masked("sk-123"), "••••••••");
        assert_eq!(masked(""), "(empty)");
        assert_eq!(
            shell_line(&["claude".into(), "it's here".into(), "-x".into()]),
            r"claude 'it'\''s here' -x"
        );
    }

    #[test]
    fn search_path_adds_user_bins_and_installed_agents() {
        let servers = tempfile::tempdir().expect("tempdir");
        let bin = servers.path().join("claude/node_modules/.bin");
        std::fs::create_dir_all(&bin).expect("mkdir");
        let path = search_path(
            "/usr/bin",
            Some(Path::new("/home/me")),
            Some(Path::new("/opt/npm/bin")),
            Some(servers.path()),
        );
        let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
        assert_eq!(dirs[0], PathBuf::from("/usr/bin"));
        assert!(dirs.contains(&PathBuf::from("/home/me/.ferrum/bin")));
        assert!(dirs.contains(&PathBuf::from("/home/me/.local/bin")));
        assert!(dirs.contains(&PathBuf::from("/opt/npm/bin")));
        assert!(dirs.contains(&bin));
    }

    #[test]
    fn status_from_process_and_idle_time() {
        let t = Duration::from_secs(5);
        let s = |exit, stopped, idle| AgentStatus::of(exit, stopped, Duration::from_secs(idle), t);
        assert_eq!(s(None, false, 1), AgentStatus::Running);
        assert_eq!(s(None, false, 6), AgentStatus::Waiting);
        assert_eq!(s(Some(2), false, 0), AgentStatus::Exited(2));
        assert_eq!(s(Some(0), true, 0), AgentStatus::Stopped);
        assert_eq!(AgentStatus::Exited(1).label(), "exited (1)");
        assert!(AgentStatus::Waiting.is_live() && !AgentStatus::Stopped.is_live());
    }

    #[test]
    fn context_prompts() {
        assert_eq!(
            context_prompt("src/a.rs", None),
            "I'm looking at src/a.rs in my editor."
        );
        let p = context_prompt("src/a.rs", Some((3, 4, "fn a() {}\n")));
        assert!(p.starts_with("I'm looking at src/a.rs, lines 3-4:\n```\nfn a() {}"));
    }
}
