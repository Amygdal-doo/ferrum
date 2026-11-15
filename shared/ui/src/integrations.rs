//! Coding agents, the Ferrum agent skill, the command-line tools, macOS
//! permissions and agent-driven config migration: the machinery behind
//! onboarding's "Integrations & Permissions" / "Migrate" steps and
//! Settings › Integrations.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::app_info::{AppInfo, PermissionKind};
use crate::components::IconName;
use crate::prefs;

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// A coding agent CLI the user may have installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Agent {
    ClaudeCode,
    Codex,
    OpenCode,
}

impl Agent {
    pub const ALL: [Self; 3] = [Self::ClaudeCode, Self::Codex, Self::OpenCode];

    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::OpenCode => "OpenCode",
        }
    }

    pub fn command(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::ClaudeCode => IconName::Sparkle,
            Self::Codex => IconName::Code,
            Self::OpenCode => IconName::Terminal,
        }
    }

    /// `0xRRGGBB` tile colour.
    pub fn tint(self) -> u32 {
        match self {
            Self::ClaudeCode => 0xd97757,
            Self::Codex => 0x10a37f,
            Self::OpenCode => 0x8b8b90,
        }
    }

    /// Where the agent's executable is, searching `PATH` and the usual
    /// install folders (apps started from the Dock get a minimal `PATH`).
    pub fn locate(self) -> Option<PathBuf> {
        find_executable(self.command())
    }

    /// Where the Ferrum skill goes for this agent. Codex and OpenCode both
    /// read the shared `~/.agents/skills` folder.
    pub fn skill_file(self) -> Option<PathBuf> {
        let home = home()?;
        Some(match self {
            Self::ClaudeCode => home.join(".claude/skills/ferrum/SKILL.md"),
            Self::Codex | Self::OpenCode => home.join(".agents/skills/ferrum/SKILL.md"),
        })
    }
}

/// `name` on `PATH` or in a common install folder.
pub fn find_executable(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(home) = home() {
        for rel in [
            ".local/bin",
            ".claude/local",
            ".npm-global/bin",
            ".bun/bin",
            ".cargo/bin",
            ".opencode/bin",
            ".ferrum/bin",
        ] {
            dirs.push(home.join(rel));
        }
    }
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    dirs.into_iter()
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

/// Agents found on this machine.
pub fn installed_agents() -> Vec<Agent> {
    Agent::ALL
        .into_iter()
        .filter(|a| a.locate().is_some())
        .collect()
}

/// The skill teaching agents to drive the Ferrum apps from the shell.
pub const SKILL: &str = r#"---
name: ferrum
description: Open and drive the Ferrum developer tools (FerrumIDE, FerrumGIT, FerrumTerm, FerrumAPI, FerrumData, FerrumContainers, FerrumPorts, FerrumVariables, FerrumKit, FerrumEngineer, Ferrum System) from the command line. Use when the user asks to open a file or project in Ferrum, show a repo in FerrumGIT, run something in a FerrumTerm pane, or mentions any Ferrum app.
---

# Ferrum tools

The apps' command-line entry points live in `~/.ferrum/bin` (installed by Ferrum
Launcher). Call them by full path if `~/.ferrum/bin` is not on `PATH`.

## Open things
- `ferrumeditor path[:line[:column]] …` — open files or a folder in FerrumIDE
  (no arguments: restore the last session). `ferrumeditor --help` for options.
- `ferrumgit <repo>` — show a repository in FerrumGIT.
- `ferrumvibe <folder>` — chat, git and terminal side by side for a project
  (FerrumEngineer).
- `ferrumsystem <folder>` — the project's architecture diagram (Ferrum System);
  it is kept in `<folder>/.architecture/system.json`, which you may read or edit.
- `ferrumdock <folder>` — containers for a project (FerrumContainers).
- `ferrumports <folder>` — listening ports and their processes.
- `ferrumapi`, `ferrumdata`, `ferrumenv` (FerrumVariables), `ferrumkit`,
  `ferrumterm` — open the app.

Short aliases may be enabled in the launcher: `fei` (FerrumIDE), `feg` (git),
`fet` (term), `fea` (api), `fed` (data), `fec` (containers), `fep` (ports),
`fev` (FerrumVariables), `fek` (kit), `fen` (FerrumEngineer), `fes` (system).
`fei .` opens the current folder.

## Terminals
FerrumTerm terminals live in a persistent server driven by `fterm`
(`fterm --help`): `fterm run --split right -- npm run dev`, `fterm capture -t b3 -n 40`,
`fterm send -t b3 -e "cmd"`, `fterm wait -t b3`, `fterm status done -m "…"`.
If the `fterm` skill is installed, follow it for terminal work.

## Shared state
- `~/.ferrum/projects.json` — recent projects shared by every app.
- `~/.ferrum/appearance.json` — colour scheme and themes for the whole suite.
- `~/.ferrum/<app>/keymap.json` — shortcut overrides recorded in Settings.

Never edit the apps' databases directly; ask the user to use the app instead.
"#;

/// Whether the Ferrum skill is installed for every agent found.
pub fn skill_installed() -> bool {
    let agents = installed_agents();
    !agents.is_empty()
        && agents
            .iter()
            .filter_map(|a| a.skill_file())
            .all(|f| f.is_file())
}

/// Installs the skill for every agent found (or Claude Code when none is).
/// Returns the files written.
pub fn install_skill() -> std::io::Result<Vec<PathBuf>> {
    let mut agents = installed_agents();
    if agents.is_empty() {
        agents.push(Agent::ClaudeCode);
    }
    let mut files: Vec<PathBuf> = agents.iter().filter_map(|a| a.skill_file()).collect();
    files.dedup();
    for file in &files {
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file, SKILL)?;
    }
    Ok(files)
}

/// Removes the skill from every agent.
pub fn remove_skill() -> std::io::Result<()> {
    for agent in Agent::ALL {
        if let Some(dir) = agent
            .skill_file()
            .and_then(|f| f.parent().map(Path::to_path_buf))
            && dir.is_dir()
        {
            std::fs::remove_dir_all(dir)?;
        }
    }
    Ok(())
}

/// `~/.ferrum/bin/<binary>`, when Ferrum Launcher linked it.
pub fn cli_path(binary: &str) -> Option<PathBuf> {
    Some(prefs::ferrum_dir()?.join("bin").join(binary)).filter(|p| p.exists())
}

/// Whether a privacy permission is known to be granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionStatus {
    Granted,
    NotGranted,
    /// macOS gives apps no way to check; the user looks in System Settings.
    Unknown,
}

impl PermissionKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::FullDiskAccess => "Full Disk Access",
            Self::FilesAndFolders => "Files and Folders",
            Self::Notifications => "Notifications",
            Self::Automation => "Automation",
            Self::LocalNetwork => "Local Network",
            Self::Accessibility => "Accessibility",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::FullDiskAccess => IconName::Lock,
            Self::FilesAndFolders => IconName::Folder,
            Self::Notifications => IconName::Bell,
            Self::Automation => IconName::ToolHammer,
            Self::LocalNetwork => IconName::Public,
            Self::Accessibility => IconName::Person,
        }
    }

    /// The System Settings pane for it.
    pub fn settings_url(self) -> &'static str {
        match self {
            Self::FullDiskAccess => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
            }
            Self::FilesAndFolders => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_FilesAndFolders"
            }
            Self::Notifications => {
                "x-apple.systempreferences:com.apple.Notifications-Settings.extension"
            }
            Self::Automation => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation"
            }
            Self::LocalNetwork => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork"
            }
            Self::Accessibility => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
        }
    }

    pub fn status(self) -> PermissionStatus {
        if !cfg!(target_os = "macos") {
            return PermissionStatus::Granted;
        }
        match self {
            Self::Notifications => match crate::mac_notifications::granted() {
                Some(true) => PermissionStatus::Granted,
                Some(false) => PermissionStatus::NotGranted,
                None => PermissionStatus::Unknown,
            },
            // Only apps with Full Disk Access can read the TCC folder.
            Self::FullDiskAccess => match home() {
                Some(home) => {
                    let probe = home.join("Library/Application Support/com.apple.TCC");
                    if std::fs::read_dir(probe).is_ok() {
                        PermissionStatus::Granted
                    } else {
                        PermissionStatus::NotGranted
                    }
                }
                None => PermissionStatus::Unknown,
            },
            _ => PermissionStatus::Unknown,
        }
    }
}

/// Asks for `kind`. Notifications get the system prompt (which also lists
/// the app under System Settings › Notifications); everything else, and a
/// notification permission already declined, opens its System Settings pane.
pub fn grant(kind: PermissionKind, cx: &mut gpui::App) {
    let prompt = kind == PermissionKind::Notifications
        && crate::mac_notifications::available()
        && crate::mac_notifications::granted() != Some(false);
    if !prompt {
        cx.open_url(kind.settings_url());
        return;
    }
    crate::mac_notifications::request_authorization();
    // The answer arrives on another thread; redraw once it is in.
    cx.spawn(async move |cx| {
        for _ in 0..30 {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(500))
                .await;
            if crate::mac_notifications::granted().is_some() {
                cx.update(|cx| cx.refresh_windows());
                break;
            }
        }
    })
    .detach();
}

/// "None granted yet" / "2 of 3 granted" for the permissions card.
pub fn permissions_summary(info: &AppInfo) -> String {
    if info.permissions.is_empty() {
        return "None needed".into();
    }
    let granted = info
        .permissions
        .iter()
        .filter(|p| p.kind.status() == PermissionStatus::Granted)
        .count();
    let checkable = info
        .permissions
        .iter()
        .filter(|p| p.kind.status() != PermissionStatus::Unknown)
        .count();
    match (granted, checkable) {
        (_, 0) => format!("{} to review", info.permissions.len()),
        (0, _) => "None granted yet".into(),
        (g, _) if g == info.permissions.len() => "All granted".into(),
        (g, _) => format!("{g} of {} granted", info.permissions.len()),
    }
}

/// The prompt handed to a coding agent to migrate the user's config.
pub fn migration_prompt(info: &AppInfo) -> Option<String> {
    let migration = info.migration.as_ref()?;
    Some(format!(
        "Migrate my configuration from {from} to {app}.\n\n\
         {task}\n\n\
         How to work:\n\
         1. Find which of {from} are installed and where their config files are. Read them; do not change them.\n\
         2. Build a table: each setting or binding you found, with its file and line, and what it becomes in {app} \
            (or why it has no counterpart). Never drop an item silently.\n\
         3. Before writing, show the table and ask me about every conflict, e.g. a key that {app} already uses \
            for something else. Keep {app}'s default unless I choose otherwise.\n\
         4. Write only {app}'s files named above. Back up any file you change (copy it to `<file>.bak`).\n\
         5. Finish with what was migrated, what replaced a default, and what could not be migrated.\n",
        from = join_names(migration.from),
        app = info.name,
        task = migration.task,
    ))
}

fn join_names(names: &[&str]) -> String {
    match names {
        [] => "other tools".into(),
        [one] => (*one).into(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Starts `agent` with the migration prompt in a new terminal window.
/// Returns the prompt file it reads from.
pub fn run_migration(agent: Agent, info: &AppInfo) -> anyhow::Result<PathBuf> {
    let prompt = migration_prompt(info).ok_or_else(|| anyhow::anyhow!("nothing to migrate"))?;
    let dir = prefs::path(info.binary).ok_or_else(|| anyhow::anyhow!("no home folder"))?;
    std::fs::create_dir_all(&dir)?;
    let prompt_file = dir.join("migration-prompt.md");
    std::fs::write(&prompt_file, &prompt)?;
    let command = agent
        .locate()
        .map_or_else(|| agent.command().to_owned(), |p| p.display().to_string());
    let script = dir.join("migrate.command");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\ncd \"$HOME\" || exit 1\nexec {} \"$(cat {})\"\n",
            shell_quote(&command),
            shell_quote(&prompt_file.display().to_string()),
        ),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
    }
    if cfg!(target_os = "macos") {
        // `.command` files open in the default terminal app.
        Command::new("/usr/bin/open")
            .arg(&script)
            .stdin(Stdio::null())
            .spawn()?;
    } else {
        anyhow::bail!(
            "open {} in a terminal to start the migration",
            script.display()
        );
    }
    Ok(prompt_file)
}

/// `text` in single quotes for `sh`.
pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_info::Migration;

    #[test]
    fn prompt_names_every_source_and_the_task() {
        let info = AppInfo::new("FerrumTerm", "ferrumterm").migration(Migration {
            from: &["iTerm2", "Ghostty", "tmux"],
            summary: "Key bindings.",
            task: "FerrumTerm keeps overrides in ~/.ferrum/ferrumterm/keymap.json.",
        });
        let prompt = migration_prompt(&info).expect("prompt");
        assert!(prompt.contains("from iTerm2, Ghostty and tmux to FerrumTerm"));
        assert!(prompt.contains("~/.ferrum/ferrumterm/keymap.json"));
        assert!(migration_prompt(&AppInfo::new("X", "x")).is_none());
    }

    #[test]
    fn quoting() {
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(join_names(&["A", "B"]), "A and B");
    }

    #[test]
    fn skill_has_front_matter() {
        assert!(SKILL.starts_with("---\nname: ferrum\n"));
    }
}
