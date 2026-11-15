//! Agents: which blocks count as one, and how the sidebar starts a new one
//! (the same request as `fterm run --window -l <name> -- <command>`).

use ferrumterm_protocol::{BlockInfo, SessionInfo};

/// Menu entries of the "New agent" button: (title, command line).
pub const PRESETS: [(&str, &str); 2] = [("Claude Code", "claude"), ("Codex", "codex")];

pub use ferrumterm_protocol::agents::program_name;
use ferrumterm_protocol::agents::{KNOWN, agent_of_command};

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The coding agent `b` runs: the one the server sees in its foreground,
/// else the program it was started as.
#[must_use]
pub fn agent_name(b: &BlockInfo) -> Option<&str> {
    b.agent.as_deref().or_else(|| agent_of_command(&b.command))
}

/// Known agent program (started as one, or in the foreground), an
/// `agent…` label, or a block reporting a program status (Claude Code
/// hooks, Codex, anything speaking OSC 7501).
#[must_use]
pub fn is_agent(b: &BlockInfo) -> bool {
    b.status.is_some() || runs_agent(b)
}

/// Whether `b` runs a known coding agent (or is labelled as one), not
/// just a shell reporting its status.
pub fn runs_agent(b: &BlockInfo) -> bool {
    b.label
        .as_deref()
        .is_some_and(|l| l.to_ascii_lowercase().starts_with("agent"))
        || agent_name(b).is_some()
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
    fn agents_started_through_the_login_shell() {
        assert_eq!(program_name(&shell_command("claude")), Some("claude"));
        assert_eq!(program_name(&argv(&["npm", "run", "dev"])), Some("npm"));
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
            agent: None,
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
        // Typed into a shell: the server reports the foreground agent.
        b.command = argv(&["/bin/zsh"]);
        assert!(!is_agent(&b));
        b.agent = Some("codex".into());
        assert!(is_agent(&b));
        assert_eq!(agent_name(&b), Some("codex"));
    }

    #[test]
    fn labels_are_unique_and_named_after_the_program() {
        assert_eq!(label_for("aider --model sonnet"), "aider");
        assert_eq!(label_for("  "), "agent");
        assert_eq!(unique_label("claude", &[]), "claude");
    }
}
