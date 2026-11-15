//! Which programs are coding agents, shared by the server (status
//! detection) and the clients (sidebar, tabs).

/// Coding agents FerrumTerm recognises by program name.
pub const KNOWN: [&str; 10] = [
    "claude",
    "codex",
    "gemini",
    "aider",
    "opencode",
    "amp",
    "goose",
    "cursor-agent",
    "qwen",
    "crush",
];

const SHELLS: [&str; 5] = ["sh", "bash", "zsh", "fish", "dash"];

/// Programs that run an agent as a script (`node …/claude-code/cli.js`).
const RUNTIMES: [&str; 9] = [
    "node", "bun", "deno", "python", "python3", "npx", "uv", "uvx", "pipx",
];

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

/// The known agent named by one path or package component: `claude`,
/// `claude-code`, `gemini-cli`, `codex.js`, `@openai/codex` → its name.
fn agent_in_component(component: &str) -> Option<&'static str> {
    let stem = component
        .trim_start_matches('@')
        .split('.')
        .next()
        .unwrap_or(component)
        .to_ascii_lowercase();
    KNOWN.into_iter().find(|known| {
        stem == *known
            || stem
                .strip_prefix(known)
                .is_some_and(|rest| rest == "-code" || rest == "-cli")
    })
}

/// The agent a process runs, from its argv: `claude --resume`,
/// `node /opt/…/@anthropic-ai/claude-code/cli.js`, `python -m aider`…
#[must_use]
pub fn agent_from_argv<S: AsRef<str>>(argv: &[S]) -> Option<&'static str> {
    let (prog, args) = argv.split_first()?;
    let prog = basename(prog.as_ref()).trim_start_matches('-');
    if let Some(agent) = agent_in_component(prog) {
        return Some(agent);
    }
    if !RUNTIMES.contains(&prog) {
        return None;
    }
    // The script (or `-m module`) the runtime starts.
    let mut args = args.iter().map(AsRef::as_ref);
    while let Some(arg) = args.next() {
        let arg: &str = arg;
        if arg == "-m" {
            return args.next().and_then(agent_in_component);
        }
        if arg.starts_with('-') {
            continue;
        }
        return arg.split('/').rev().find_map(agent_in_component);
    }
    None
}

/// [`agent_from_argv`] for a whitespace-separated command line (what `ps`
/// prints).
#[must_use]
pub fn agent_from_command_line(line: &str) -> Option<&'static str> {
    let argv: Vec<&str> = line.split_whitespace().collect();
    agent_from_argv(&argv)
}

/// The agent a block was started as (`claude`, or `zsh -lic "exec claude"`).
#[must_use]
pub fn agent_of_command(command: &[String]) -> Option<&'static str> {
    program_name(command).and_then(agent_in_component)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            program_name(&argv(&["/bin/zsh", "-lic", "exec codex --full-auto"])),
            Some("codex")
        );
        assert_eq!(program_name(&argv(&["zsh", "--login"])), Some("zsh"));
        assert_eq!(
            agent_of_command(&argv(&["/bin/zsh", "-lic", "exec claude"])),
            Some("claude")
        );
        assert_eq!(agent_of_command(&argv(&["npm", "test"])), None);
    }

    #[test]
    fn agents_from_process_command_lines() {
        assert_eq!(agent_from_command_line("claude --resume"), Some("claude"));
        assert_eq!(
            agent_from_command_line(
                "node /opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js"
            ),
            Some("claude")
        );
        assert_eq!(
            agent_from_command_line("node /usr/local/lib/node_modules/@openai/codex/bin/codex.js"),
            Some("codex")
        );
        assert_eq!(
            agent_from_command_line("node --no-warnings /x/@google/gemini-cli/dist/index.js"),
            Some("gemini")
        );
        assert_eq!(
            agent_from_command_line("/usr/bin/python3 -m aider --model sonnet"),
            Some("aider")
        );
        assert_eq!(
            agent_from_command_line("/Users/me/.local/bin/cursor-agent"),
            Some("cursor-agent")
        );
        assert_eq!(agent_from_command_line("-zsh"), None);
        assert_eq!(agent_from_command_line("node server.js"), None);
        assert_eq!(agent_from_command_line("vim claude.md"), None);
        assert_eq!(agent_from_command_line(""), None);
        assert_eq!(agent_from_command_line("claudette"), None);
    }
}
