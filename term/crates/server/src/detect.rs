//! Agent status without the agent's help. Agents that report their status
//! (OSC 7501, `fterm status`, the Claude Code hooks) are believed; for the
//! others the server watches every block whose foreground program is a
//! known coding agent and classifies it once a second from:
//!
//! - the screen: a permission prompt or question near the bottom
//!   (`❯ 1. Yes`, `Allow …?`, `(y/n)`) → blocked; an "esc to interrupt"
//!   status line → working;
//! - output timing: output in the last [`BUSY_WINDOW_MS`] that was not the
//!   echo of typing or a resize → working;
//! - the title: a braille spinner glyph in front → working.
//!
//! Working turns into done after [`DONE_AFTER_MS`] without any sign of work,
//! so short pauses between tool calls don't count as finishing.
//!
//! Everything here except [`process_command`] is pure and unit-tested.

use ferrumterm_protocol::ProgramState;

/// Output this recent (and not an echo) means the agent is working.
pub const BUSY_WINDOW_MS: u64 = 2_000;
/// Output this soon after a keystroke or resize is its echo / redraw.
pub const ECHO_MS: u64 = 400;
/// Quiet this long after working means the turn is done.
pub const DONE_AFTER_MS: u64 = 3_000;
/// Lines from the bottom of the screen searched for prompts.
const BOTTOM_LINES: usize = 24;

/// What one look at an agent's block says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// Waiting for an answer: the question line and `permission` / `question`.
    Question {
        line: String,
        kind: &'static str,
    },
    Busy,
    Quiet,
}

/// The visible screen, title and output timing of a block.
#[derive(Debug, Clone, Copy)]
pub struct Observation<'a> {
    pub screen: &'a str,
    pub title: &'a str,
    /// Milliseconds since the last output that was not an echo; `None`: none yet.
    pub quiet_ms: Option<u64>,
}

/// Strips frame drawing and padding: `│ Do you want to proceed? │` → the text.
fn clean(line: &str) -> &str {
    line.trim_matches(|c: char| {
        c.is_whitespace() || ('\u{2500}'..='\u{257f}').contains(&c) || c == '|'
    })
}

/// The non-empty lines at the bottom of the screen, top to bottom.
fn bottom_lines(screen: &str) -> Vec<&str> {
    let lines: Vec<&str> = screen
        .lines()
        .map(clean)
        .filter(|l| !l.is_empty())
        .collect();
    let start = lines.len().saturating_sub(BOTTOM_LINES);
    lines[start..].to_vec()
}

/// `❯ 1. Yes`, `› 1. Yes, proceed`, `● 1. Allow once`, `1) Approve`.
fn is_first_choice(line: &str) -> bool {
    let rest = line.trim_start_matches(|c: char| {
        c.is_whitespace() || matches!(c, '❯' | '›' | '>' | '●' | '○' | '▶' | '→' | '*' | '-')
    });
    let Some(rest) = rest.strip_prefix("1.").or_else(|| rest.strip_prefix("1)")) else {
        return false;
    };
    let word = rest.trim_start().to_ascii_lowercase();
    ["yes", "allow", "approve", "proceed", "accept", "run"]
        .iter()
        .any(|w| word.starts_with(w))
}

/// `(y/n)`, `[Y/n]`, `(yes/no)`, aider's `(Y)es/(N)o`.
fn is_yes_no(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    ["(y/n)", "[y/n]", "(yes/no)", "[yes/no]", "(y)es/(n)o"]
        .iter()
        .any(|p| l.contains(p))
}

fn is_permission_word(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.starts_with("allow ")
        || l.starts_with("approve")
        || l.contains("permission")
        || l.contains("do you want to proceed")
        || l.contains("do you want to make this edit")
        || l.contains("do you want to run")
}

/// A question or permission prompt near the bottom of `screen`.
#[must_use]
pub fn find_question(screen: &str) -> Option<(String, &'static str)> {
    let lines = bottom_lines(screen);
    // Line-based prompts (aider, scripts) wait on the last line.
    if let Some(last) = lines.last()
        && is_yes_no(last)
    {
        return Some(((*last).to_owned(), "question"));
    }
    // TUI dialogs: a numbered choice list, its question above it.
    if let Some(i) = lines.iter().position(|l| is_first_choice(l)) {
        let question = lines[..i]
            .iter()
            .rev()
            .take(6)
            .find(|l| l.ends_with('?'))
            .copied()
            .unwrap_or(lines[i]);
        return Some((question.to_owned(), "permission"));
    }
    // `Allow command?` / `Do you want to proceed?` close to the bottom.
    let tail = &lines[lines.len().saturating_sub(8)..];
    tail.iter()
        .find(|l| l.ends_with('?') && is_permission_word(l))
        .map(|l| ((*l).to_owned(), "permission"))
}

/// An "esc to interrupt" style status line (Claude Code, Codex, Gemini).
fn shows_progress(screen: &str) -> bool {
    let lines = bottom_lines(screen);
    lines[lines.len().saturating_sub(12)..].iter().any(|l| {
        let l = l.to_ascii_lowercase();
        l.contains("esc to interrupt") || l.contains("esc to cancel")
    })
}

/// A braille spinner frame in front of the title (`⠋ Fixing tests`).
fn title_spinner(title: &str) -> bool {
    title
        .trim_start()
        .chars()
        .next()
        .is_some_and(|c| ('\u{2801}'..='\u{28ff}').contains(&c))
}

/// Classifies one observation.
#[must_use]
pub fn observe(o: &Observation<'_>) -> Signal {
    if let Some((line, kind)) = find_question(o.screen) {
        return Signal::Question { line, kind };
    }
    let recent_output = o.quiet_ms.is_some_and(|q| q < BUSY_WINDOW_MS);
    if shows_progress(o.screen) || title_spinner(o.title) || recent_output {
        Signal::Busy
    } else {
        Signal::Quiet
    }
}

/// The next state of an agent: `previous` is what detection said last
/// time, `since_busy_ms` how long ago it last looked busy.
#[must_use]
pub fn step(
    previous: Option<ProgramState>,
    signal: &Signal,
    since_busy_ms: Option<u64>,
) -> ProgramState {
    match signal {
        Signal::Question { .. } => ProgramState::Blocked,
        Signal::Busy => ProgramState::Working,
        Signal::Quiet => match previous {
            Some(ProgramState::Working) => {
                if since_busy_ms.is_none_or(|ms| ms >= DONE_AFTER_MS) {
                    ProgramState::Done
                } else {
                    ProgramState::Working
                }
            }
            Some(ProgramState::Done) => ProgramState::Done,
            Some(ProgramState::Error) => ProgramState::Error,
            Some(ProgramState::Blocked | ProgramState::Idle) | None => ProgramState::Idle,
        },
    }
}

/// The command line of process `pid` (`ps -o command=`), for telling which
/// agent is in the foreground. Called only when the foreground changes.
#[must_use]
pub fn process_command(pid: i32) -> Option<String> {
    let out = std::process::Command::new("/bin/ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (out.status.success() && !text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE_PERMISSION: &str = "\
> fix the tests

● Bash(rm -rf target)

╭──────────────────────────────────────────────╮
│ Bash command                                 │
│                                              │
│   rm -rf target                              │
│   Remove build output                        │
│                                              │
│ Do you want to proceed?                      │
│ ❯ 1. Yes                                     │
│   2. Yes, and don't ask again for rm         │
│   3. No, and tell Claude what to do (esc)    │
╰──────────────────────────────────────────────╯
";

    const CLAUDE_WORKING: &str = "\
> fix the tests

✻ Pondering… (12s · ↑ 1.2k tokens · esc to interrupt)

╭──────────────────────────────────────────────╮
│ >                                            │
╰──────────────────────────────────────────────╯
  ? for shortcuts
";

    const CLAUDE_IDLE: &str = "\
● All 42 tests pass now.

╭──────────────────────────────────────────────╮
│ >                                            │
╰──────────────────────────────────────────────╯
  ? for shortcuts
";

    fn look(screen: &str, title: &str, quiet_ms: Option<u64>) -> Signal {
        observe(&Observation {
            screen,
            title,
            quiet_ms,
        })
    }

    #[test]
    fn permission_dialogs_are_questions() {
        assert_eq!(
            find_question(CLAUDE_PERMISSION),
            Some(("Do you want to proceed?".to_owned(), "permission"))
        );
        let codex = "Allow command?\n\n  $ npm install\n\n› 1. Yes, proceed (y)\n  2. No (esc)\n";
        assert_eq!(
            find_question(codex).map(|(l, _)| l),
            Some("Allow command?".to_owned())
        );
        let gemini = "Allow execution of: 'ls -la'?\n● 1. Yes, allow once\n○ 2. No\n";
        assert_eq!(find_question(gemini).map(|(_, k)| k), Some("permission"));
        let aider = "Add src/main.rs to the chat? (Y)es/(N)o/(D)on't ask again [Yes]: ";
        assert_eq!(
            find_question(aider),
            Some((aider.trim().to_owned(), "question"))
        );
        assert_eq!(
            find_question("Overwrite? [y/N]").map(|(_, k)| k),
            Some("question")
        );
        assert_eq!(
            find_question("Continue? (y/n)").map(|(_, k)| k),
            Some("question")
        );
    }

    #[test]
    fn ordinary_output_is_not_a_question() {
        assert_eq!(find_question(CLAUDE_IDLE), None);
        assert_eq!(find_question(CLAUDE_WORKING), None);
        assert_eq!(find_question("1. Install deps\n2. Run tests\n"), None);
        // An answered y/n prompt scrolled up is no longer waiting.
        assert_eq!(find_question("Continue? (y/n) y\nDone.\n$ "), None);
        assert_eq!(
            find_question("Why does the build fail?\nBecause of a typo.\n"),
            None
        );
        assert_eq!(find_question(""), None);
    }

    #[test]
    fn observations() {
        assert!(matches!(
            look(CLAUDE_PERMISSION, "", Some(10)),
            Signal::Question { .. }
        ));
        assert_eq!(look(CLAUDE_WORKING, "", None), Signal::Busy);
        assert_eq!(look(CLAUDE_IDLE, "✳ Fix tests", None), Signal::Quiet);
        assert_eq!(look(CLAUDE_IDLE, "⠋ Fix tests", None), Signal::Busy);
        assert_eq!(look(CLAUDE_IDLE, "", Some(500)), Signal::Busy);
        assert_eq!(look(CLAUDE_IDLE, "", Some(BUSY_WINDOW_MS)), Signal::Quiet);
    }

    #[test]
    fn state_machine() {
        use ProgramState::*;
        let q = Signal::Question {
            line: "?".into(),
            kind: "permission",
        };
        assert_eq!(step(None, &Signal::Quiet, None), Idle);
        assert_eq!(step(Some(Idle), &Signal::Busy, Some(0)), Working);
        // Short pauses between tool calls stay working.
        assert_eq!(step(Some(Working), &Signal::Quiet, Some(1_000)), Working);
        assert_eq!(
            step(Some(Working), &Signal::Quiet, Some(DONE_AFTER_MS)),
            Done
        );
        assert_eq!(step(Some(Done), &Signal::Quiet, Some(60_000)), Done);
        assert_eq!(step(Some(Working), &q, Some(0)), Blocked);
        assert_eq!(step(Some(Blocked), &q, Some(0)), Blocked);
        // Answered (or dismissed) without new work: back to idle.
        assert_eq!(step(Some(Blocked), &Signal::Quiet, Some(9_000)), Idle);
        assert_eq!(step(Some(Done), &Signal::Busy, Some(0)), Working);
    }
}
