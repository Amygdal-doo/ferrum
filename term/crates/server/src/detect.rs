//! Agent status without the agent's help. Agents that report their status
//! (OSC 7501, `fterm status`, the Claude Code / Codex hooks) are believed;
//! for the others the server watches every block whose foreground program is
//! a known coding agent and classifies it whenever the block prints or is
//! typed into (and at least once a second) from:
//!
//! - the screen: a permission prompt or question near the bottom
//!   (`❯ 1. Yes`, `Allow …?`, `(y/n)`) → blocked; an "esc to interrupt"
//!   status line → working;
//! - the title: a braille spinner glyph in front → working;
//! - input: Enter typed into the agent (a prompt submitted, a permission
//!   granted) → working right away, for [`SUBMIT_GRACE_MS`] while the agent
//!   gets going;
//! - output timing, for agents that never show such an indicator: output in
//!   the last [`BUSY_WINDOW_MS`] that was not the echo of typing or a resize
//!   → working.
//!
//! An agent that shows an indicator (Claude Code, Codex, Gemini…; any agent
//! once it has shown one) is working exactly while it does: its idle screen
//! may redraw (a blinking cursor, a clock) without that counting as work, and
//! the turn is done [`INDICATOR_DONE_MS`] after the indicator goes. Agents
//! known only by their output are done after [`DONE_AFTER_MS`] of quiet, so
//! short pauses between tool calls don't count as finishing.
//!
//! Everything here except [`process_command`] and [`Wake`] is pure and
//! unit-tested.

use ferrumterm_protocol::ProgramState;
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::Duration;

/// Output this recent (and not an echo) means an agent without an
/// indicator is working.
pub const BUSY_WINDOW_MS: u64 = 2_000;
/// Output this soon after a keystroke or resize is its echo / redraw.
pub const ECHO_MS: u64 = 400;
/// Quiet this long after working means the turn of an agent without an
/// indicator is done.
pub const DONE_AFTER_MS: u64 = 3_000;
/// An indicator gone this long means the turn is done (it can blink out
/// while the screen redraws).
pub const INDICATOR_DONE_MS: u64 = 400;
/// After Enter, the agent counts as working this long while no sign of work
/// shows up yet; without one it goes back to idle (Enter in a menu).
pub const SUBMIT_GRACE_MS: u64 = 1_500;
/// Lines from the bottom of the screen searched for prompts.
const BOTTOM_LINES: usize = 24;

/// Agents known to show a working indicator ("esc to interrupt", a title
/// spinner) for as long as a turn lasts.
const INDICATORS: [&str; 4] = ["claude", "codex", "gemini", "qwen"];

/// Whether `agent` (a [`ferrumterm_protocol::agents::KNOWN`] name) shows a
/// working indicator for as long as it works.
#[must_use]
pub fn shows_indicator(agent: &str) -> bool {
    INDICATORS.contains(&agent)
}

/// What one look at an agent's block says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// Waiting for an answer: the question line and `permission` / `question`.
    Question {
        line: String,
        kind: &'static str,
    },
    /// A working indicator on screen or in the title.
    Busy,
    /// Recent output, but no indicator.
    Output,
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

/// Option `n` of a choice list: the mark in front of it (`❯`, `›`, `●`,
/// `○`, …; `None`: none) and its text lowercased. `❯ 1. Yes` →
/// `(Some('❯'), "yes")`.
fn option(line: &str, n: u8) -> Option<(Option<char>, String)> {
    let rest = line.trim_start_matches(|c: char| {
        c.is_whitespace() || matches!(c, '❯' | '›' | '>' | '●' | '○' | '▶' | '→' | '*' | '-')
    });
    let mark = line
        .trim_start()
        .chars()
        .next()
        .filter(|c| !c.is_ascii_digit());
    let rest = rest
        .strip_prefix(&format!("{n}."))
        .or_else(|| rest.strip_prefix(&format!("{n})")))?;
    Some((mark, rest.trim_start().to_ascii_lowercase()))
}

fn starts_with_any(text: &str, words: &[&str]) -> bool {
    words.iter().any(|w| text.starts_with(w))
}

/// The first choice of a permission dialog at `lines[i]`: `❯ 1. Yes`,
/// `› 1. Yes, proceed`, `● 1. Allow once`, `1) Approve`, with a second
/// choice right below it. A plain numbered list at the end of a reply
/// (`1. Run the tests`) is not one: the choice needs a selection cursor in
/// front, or a question above it and a Yes/No style second choice.
fn is_first_choice(lines: &[&str], i: usize) -> bool {
    let Some((mark, first)) = option(lines[i], 1) else {
        return false;
    };
    if !starts_with_any(
        &first,
        &["yes", "allow", "approve", "proceed", "accept", "run"],
    ) {
        return false;
    }
    let Some((second_mark, second)) = lines[i + 1..].iter().take(3).find_map(|l| option(l, 2))
    else {
        return false;
    };
    // `●` alone also starts Claude Code's replies; as a radio button the
    // other choices carry `○`.
    let cursor = match mark {
        Some('❯' | '›' | '▶' | '→') => true,
        Some('●') => second_mark == Some('○'),
        _ => false,
    };
    if cursor {
        return true;
    }
    let asked = lines[..i].iter().rev().take(6).any(|l| l.ends_with('?'));
    asked
        && starts_with_any(
            &second,
            &["yes", "no", "don't", "deny", "reject", "cancel", "always"],
        )
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
    if let Some(i) = (0..lines.len()).find(|&i| is_first_choice(&lines, i)) {
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

/// An "esc to interrupt" style status line (Claude Code, Codex, Gemini;
/// OpenCode's `esc interrupt`).
fn shows_progress(screen: &str) -> bool {
    let lines = bottom_lines(screen);
    lines[lines.len().saturating_sub(12)..].iter().any(|l| {
        let l = l.to_ascii_lowercase();
        ["esc to interrupt", "esc to cancel", "esc interrupt"]
            .iter()
            .any(|p| l.contains(p))
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
    if shows_progress(o.screen) || title_spinner(o.title) {
        Signal::Busy
    } else if o.quiet_ms.is_some_and(|q| q < BUSY_WINDOW_MS) {
        Signal::Output
    } else {
        Signal::Quiet
    }
}

/// What the state machine knows besides the latest [`Signal`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Clock {
    /// The agent shows a working indicator, so output alone is not work.
    pub indicates: bool,
    /// Since it last looked busy; `None`: not since it took the foreground.
    pub since_busy_ms: Option<u64>,
    /// Since Enter was last typed into it; `None`: not since it took the
    /// foreground.
    pub since_submit_ms: Option<u64>,
}

impl Clock {
    /// Whether `signal` is a sign of work for this agent.
    #[must_use]
    pub fn is_busy(&self, signal: &Signal) -> bool {
        match signal {
            Signal::Busy => true,
            Signal::Output => !self.indicates,
            Signal::Question { .. } | Signal::Quiet => false,
        }
    }

    /// It looked busy after the last Enter (or nobody pressed one): the
    /// turn really started.
    fn worked_since_submit(&self) -> bool {
        match (self.since_busy_ms, self.since_submit_ms) {
            (Some(busy), Some(submit)) => busy <= submit,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }
}

/// The next state of an agent: `previous` is what detection said last time.
#[must_use]
pub fn step(previous: Option<ProgramState>, signal: &Signal, clock: &Clock) -> ProgramState {
    if let Signal::Question { .. } = signal {
        return ProgramState::Blocked;
    }
    let submitted = clock.since_submit_ms.is_some_and(|ms| ms < SUBMIT_GRACE_MS);
    if clock.is_busy(signal) || (submitted && !clock.worked_since_submit()) {
        return ProgramState::Working;
    }
    let done_after = if clock.indicates {
        INDICATOR_DONE_MS
    } else {
        DONE_AFTER_MS
    };
    match previous {
        Some(ProgramState::Working) => {
            if clock.since_busy_ms.is_some_and(|ms| ms < done_after) {
                ProgramState::Working
            } else if clock.worked_since_submit() {
                ProgramState::Done
            } else {
                // Enter without a turn starting (a menu, an empty prompt).
                ProgramState::Idle
            }
        }
        Some(ProgramState::Done) => ProgramState::Done,
        Some(ProgramState::Error) => ProgramState::Error,
        Some(ProgramState::Blocked | ProgramState::Idle) | None => ProgramState::Idle,
    }
}

/// How often detection looks with nothing happening (to see a program
/// take the foreground, or leave it).
pub const IDLE_TICK: Duration = Duration::from_secs(1);
/// How often it looks while an agent is working (to end the turn on time).
pub const WORKING_TICK: Duration = Duration::from_millis(150);
/// Output and input wait this long before a look, so a burst of output is
/// classified once, when it has landed.
pub const SETTLE: Duration = Duration::from_millis(100);

/// Wakes the detection thread when an agent's block prints or is typed
/// into, so status follows the agent instead of a once-a-second poll.
#[derive(Debug, Default)]
pub struct Wake {
    poked: Mutex<bool>,
    ready: Condvar,
}

impl Wake {
    /// Asks for a look soon. Cheap: called for every chunk of agent output.
    pub fn poke(&self) {
        let mut poked = self.poked.lock().unwrap_or_else(PoisonError::into_inner);
        if !*poked {
            *poked = true;
            self.ready.notify_one();
        }
    }

    /// Waits for a poke, at most `timeout`; true when poked.
    pub fn wait(&self, timeout: Duration) -> bool {
        let poked = self.poked.lock().unwrap_or_else(PoisonError::into_inner);
        let (mut poked, _) = self
            .ready
            .wait_timeout_while(poked, timeout, |p| !*p)
            .unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut *poked)
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
    fn a_numbered_plan_ending_a_reply_is_not_a_dialog() {
        let plan =
            "● Here is the plan:\n\n1. Run the migration\n2. Accept the new schema\n3. Deploy\n";
        assert_eq!(find_question(plan), None);
        let asked = "Want me to go ahead?\n1. Run the migration\n2. Deploy it\n";
        assert_eq!(find_question(asked), None);
        let last = "Next steps:\n1. Accept the PR\n";
        assert_eq!(find_question(last), None);
        let claude_reply = "● 1. Run cargo fmt\n  2. Run the tests\n";
        assert_eq!(find_question(claude_reply), None);
        let quoted = "> 1. Run the tests\n> 2. Ship it\n";
        assert_eq!(find_question(quoted), None);
        // Without a cursor, a question and a Yes/No choice still make one.
        let plain = "Do you want to run npm install?\n1. Yes\n2. No\n";
        assert_eq!(
            find_question(plain),
            Some(("Do you want to run npm install?".to_owned(), "permission"))
        );
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
        assert_eq!(look(CLAUDE_IDLE, "", Some(500)), Signal::Output);
        assert_eq!(look(CLAUDE_IDLE, "", Some(BUSY_WINDOW_MS)), Signal::Quiet);
        let codex = "• Working (4s • esc to interrupt)\n\n› Ask Codex to do anything\n";
        assert_eq!(look(codex, "", None), Signal::Busy);
        let opencode = "┃ Thinking…\n  esc interrupt     ctrl+p commands\n";
        assert_eq!(look(opencode, "", None), Signal::Busy);
    }

    fn clock(indicates: bool, busy: Option<u64>, submit: Option<u64>) -> Clock {
        Clock {
            indicates,
            since_busy_ms: busy,
            since_submit_ms: submit,
        }
    }

    #[test]
    fn state_machine() {
        use ProgramState::*;
        let q = Signal::Question {
            line: "?".into(),
            kind: "permission",
        };
        let at = |busy| clock(false, busy, None);
        assert_eq!(step(None, &Signal::Quiet, &at(None)), Idle);
        assert_eq!(step(Some(Idle), &Signal::Busy, &at(Some(0))), Working);
        assert_eq!(step(Some(Idle), &Signal::Output, &at(Some(0))), Working);
        // Short pauses between tool calls stay working.
        assert_eq!(
            step(Some(Working), &Signal::Quiet, &at(Some(1_000))),
            Working
        );
        assert_eq!(
            step(Some(Working), &Signal::Quiet, &at(Some(DONE_AFTER_MS))),
            Done
        );
        assert_eq!(step(Some(Done), &Signal::Quiet, &at(Some(60_000))), Done);
        assert_eq!(step(Some(Working), &q, &at(Some(0))), Blocked);
        assert_eq!(step(Some(Blocked), &q, &at(Some(0))), Blocked);
        // Answered (or dismissed) without new work: back to idle.
        assert_eq!(step(Some(Blocked), &Signal::Quiet, &at(Some(9_000))), Idle);
        assert_eq!(step(Some(Done), &Signal::Busy, &at(Some(0))), Working);
    }

    /// Claude Code lagged seconds behind: done needed 2 s without output,
    /// then 3 s more without looking busy. With an indicator, its going
    /// away is the end of the turn.
    #[test]
    fn indicator_agents_are_done_as_soon_as_the_indicator_goes() {
        use ProgramState::*;
        let at = |busy| clock(true, Some(busy), None);
        assert_eq!(step(Some(Working), &Signal::Quiet, &at(100)), Working);
        assert_eq!(
            step(Some(Working), &Signal::Quiet, &at(INDICATOR_DONE_MS)),
            Done
        );
        const { assert!(INDICATOR_DONE_MS < 1_000) };
    }

    /// Codex's idle screen redraws: for an agent that shows an indicator,
    /// output is not work, or it would never leave working.
    #[test]
    fn output_without_the_indicator_is_not_work() {
        use ProgramState::*;
        let idle = clock(true, None, None);
        assert_eq!(step(None, &Signal::Output, &idle), Idle);
        assert_eq!(step(Some(Idle), &Signal::Output, &idle), Idle);
        let after_turn = clock(true, Some(5_000), None);
        assert_eq!(step(Some(Working), &Signal::Output, &after_turn), Done);
        assert_eq!(step(Some(Done), &Signal::Output, &after_turn), Done);
        assert!(shows_indicator("codex") && shows_indicator("claude"));
        assert!(!shows_indicator("aider"));
    }

    /// Codex did not react to a submitted prompt: Enter is working at once.
    #[test]
    fn enter_marks_working_until_the_turn_starts_or_never_does() {
        use ProgramState::*;
        for indicates in [true, false] {
            let submitted = clock(indicates, None, Some(0));
            assert_eq!(step(Some(Idle), &Signal::Quiet, &submitted), Working);
            assert_eq!(step(Some(Done), &Signal::Quiet, &submitted), Working);
            assert_eq!(step(Some(Blocked), &Signal::Quiet, &submitted), Working);
            // A question still wins.
            let q = Signal::Question {
                line: "?".into(),
                kind: "permission",
            };
            assert_eq!(step(Some(Idle), &q, &submitted), Blocked);
            // Nothing happened after Enter (a menu, an empty prompt): idle,
            // not a finished turn.
            let nothing = clock(indicates, Some(60_000), Some(SUBMIT_GRACE_MS));
            assert_eq!(step(Some(Working), &Signal::Quiet, &nothing), Idle);
        }
        // The turn started: it ends as usual.
        let started = clock(true, Some(INDICATOR_DONE_MS), Some(800));
        assert_eq!(step(Some(Working), &Signal::Quiet, &started), Done);
        // Within the grace the indicator, once seen, decides.
        let started = clock(true, Some(INDICATOR_DONE_MS), Some(INDICATOR_DONE_MS + 1));
        assert_eq!(step(Some(Working), &Signal::Quiet, &started), Done);
    }
}
