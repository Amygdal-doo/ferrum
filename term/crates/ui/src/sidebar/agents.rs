//! Agents: which blocks count as one, and how the sidebar starts a new one
//! (the same request as `fterm run --window -l <name> -- <command>`).

use ferrumterm_protocol::{BlockInfo, SessionInfo};

/// Menu entries of the "New agent" button: (title, command line).
pub const PRESETS: [(&str, &str); 2] = [("Claude Code", "claude"), ("Codex", "codex")];

/// Programs shown in the Agents section even before they report a status.
const KNOWN: [&str; 8] = [
    "claude",
    "codex",
    "aider",
    "gemini",
    "opencode",
    "amp",
    "goose",
    "cursor-agent",
];

const SHELLS: [&str; 5] = ["sh", "bash", "zsh", "fish", "dash"];

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The program a block runs: `claude` for `["claude"]`, and also for
/// `["/bin/zsh", "-lic", "exec claude --resume"]` (how the GUI starts agents).
#[must_use]
pub fn program_name(command: &[String]) -> Option<&str> {
    let (prog, args) = command.split_first()?;
    let prog = basename(prog);
    if SHELLS.contains(&prog)
        && let Some(i) = args
            .iter()
            .position(|a| a.starts_with('-') && !a.starts_with("--") && a.contains('c'))
        && let Some(script) = args.get(i + 1)
    {
        let script = script.trim_start();
        let script = script.strip_prefix("exec ").unwrap_or(script);
        if let Some(first) = script.split_whitespace().next() {
            return Some(basename(first));
        }
    }
    Some(prog)
}

/// Known agent program, an `agent…` label, or a block reporting a program
/// status (Claude Code hooks, Codex, anything speaking OSC 7501).
#[must_use]
pub fn is_agent(b: &BlockInfo) -> bool {
    b.status.is_some()
        || b.label
            .as_deref()
            .is_some_and(|l| l.to_ascii_lowercase().starts_with("agent"))
        || program_name(&b.command).is_some_and(|p| KNOWN.contains(&p))
}

/// `base`, or `base-2`, `base-3`… if a block already has that label.
#[must_use]
pub fn unique_label(base: &str, sessions: &[SessionInfo]) -> String {
    let taken = |l: &str| {
        sessions
            .iter()
            .flat_map(|s| &s.windows)
            .flat_map(|w| &w.blocks)
            .any(|b| b.label.as_deref() == Some(l))
    };
    if !taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|l| !taken(l))
        .unwrap_or_else(|| base.to_owned())
}

/// Label for a command line: its program name (`aider --model x` → `aider`).
#[must_use]
pub fn label_for(command_line: &str) -> String {
    command_line
        .split_whitespace()
        .next()
        .map_or("agent", basename)
        .to_owned()
}

/// Runs `command_line` through the user's login + interactive shell, so the
/// PATH set in `.zprofile` / `.zshrc` (npm, ~/.local/bin…) applies even when
/// FerrumTerm was opened from the Finder with a minimal environment.
#[must_use]
pub fn shell_command(command_line: &str) -> Vec<String> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/zsh".into());
    vec![shell, "-lic".into(), format!("exec {command_line}")]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumterm_protocol::{BlockId, ProgramState, ProgramStatus};

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn program_names_see_through_shell_wrappers() {
        assert_eq!(program_name(&[]), None);
        assert_eq!(
            program_name(&argv(&["/usr/local/bin/claude"])),
            Some("claude")
        );
        assert_eq!(program_name(&argv(&["npm", "run", "dev"])), Some("npm"));
        assert_eq!(
            program_name(&argv(&["/bin/zsh", "-lic", "exec codex --full-auto"])),
            Some("codex")
        );
        assert_eq!(program_name(&argv(&["bash", "-c", "aider"])), Some("aider"));
        assert_eq!(program_name(&argv(&["zsh", "--login"])), Some("zsh"));
        assert_eq!(program_name(&shell_command("claude")), Some("claude"));
    }

    #[test]
    fn agent_detection() {
        let mut b = BlockInfo {
            id: BlockId(1),
            label: None,
            title: String::new(),
            command: argv(&["npm", "test"]),
            cwd: None,
            pid: None,
            cols: 80,
            rows: 24,
            status: None,
            exited: None,
        };
        assert!(!is_agent(&b));
        b.label = Some("Agent-review".into());
        assert!(is_agent(&b));
        b.label = None;
        b.status = Some(ProgramStatus::new(ProgramState::Working));
        assert!(is_agent(&b), "anything reporting a status is listed");
        b.status = None;
        b.command = shell_command("gemini");
        assert!(is_agent(&b));
    }

    #[test]
    fn labels_are_unique_and_named_after_the_program() {
        assert_eq!(label_for("aider --model sonnet"), "aider");
        assert_eq!(label_for("  "), "agent");
        assert_eq!(unique_label("claude", &[]), "claude");
    }
}
