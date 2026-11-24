//! Agents working together: prompt another terminal's agent and read its
//! answer, read a terminal as clean text, hand one terminal's work to
//! another, and a context board each session's agents share.

use crate::util::{Ctx, parse_duration};
use clap::{Args, Subcommand};
use ferrumterm_client::Client;
use ferrumterm_protocol::{Input, Inspected, ProgramState, Request, Response};
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// Server revision that knows `Context` and `Capture::unwrapped`.
const CONTEXT_REVISION: u32 = 4;
/// How long a prompted agent gets to start working before fterm falls back
/// to waiting for its output to settle.
const START_WAIT: Duration = Duration::from_secs(20);
/// Output unchanged this long counts as finished (programs without a status).
const QUIET: Duration = Duration::from_secs(3);
/// Pause between a pasted prompt and its Enter, so TUIs see them apart.
const PASTE_SETTLE: Duration = Duration::from_millis(150);

#[derive(Args)]
pub struct PromptArgs {
    /// The block whose agent gets the prompt (id, label or name).
    pub target: String,
    /// The prompt.
    #[arg(required = true)]
    pub text: Vec<String>,
    /// Wait until the agent finishes (or asks something), then print its answer.
    #[arg(short = 'w', long)]
    pub wait: bool,
    /// Give up waiting after this long (e.g. 10m).
    #[arg(long, default_value = "10m")]
    pub timeout: String,
    /// Print at most this many lines of the answer.
    #[arg(short = 'n', long, default_value_t = 80)]
    pub lines: usize,
}

#[derive(Args)]
pub struct ReadArgs {
    #[arg(short = 't', long)]
    pub target: Option<String>,
    /// Only the last N lines.
    #[arg(short = 'n', long, default_value_t = 80)]
    pub lines: usize,
    /// The last finished shell command and its output (shell integration).
    #[arg(long)]
    pub command: bool,
}

#[derive(Args)]
pub struct HandoffArgs {
    /// The block whose work is handed over.
    pub from: String,
    /// The block whose agent continues it.
    pub to: String,
    /// What the receiving agent should do with it.
    pub message: Vec<String>,
    /// Lines of the sender's output to include.
    #[arg(short = 'n', long, default_value_t = 80)]
    pub lines: usize,
    /// Send the last finished shell command and its output instead.
    #[arg(long)]
    pub command: bool,
    /// Wait for the receiving agent and print its answer.
    #[arg(short = 'w', long)]
    pub wait: bool,
    #[arg(long, default_value = "10m")]
    pub timeout: String,
}

#[derive(Subcommand)]
pub enum ContextCmd {
    /// Print the session's notes (the default).
    Show,
    /// Add a note, signed with your block's name.
    Add { text: Vec<String> },
    /// Remove every note.
    Clear,
}

#[derive(Args)]
pub struct ContextArgs {
    /// Any target in the session (default: your own session).
    #[arg(short = 's', long)]
    pub session: Option<String>,
    #[command(subcommand)]
    pub cmd: Option<ContextCmd>,
}

fn err(e: ferrumterm_client::ClientError) -> String {
    e.to_string()
}

pub async fn prompt(a: PromptArgs, ctx: &Ctx) -> Result<ExitCode, String> {
    let (c, _events) = ctx.connect().await?;
    let timeout = a.wait.then(|| parse_duration(&a.timeout)).transpose()?;
    send_prompt(&c, ctx, &a.target, &a.text.join(" "), timeout, a.lines).await
}

pub async fn read(a: ReadArgs, ctx: &Ctx) -> Result<ExitCode, String> {
    let (c, _events) = ctx.connect().await?;
    let target = ctx.target(a.target);
    let text = if a.command {
        last_command(&c, target).await?
    } else {
        tail(&screen(&c, target).await?, a.lines)
    };
    println!("{text}");
    Ok(ExitCode::SUCCESS)
}

pub async fn handoff(a: HandoffArgs, ctx: &Ctx) -> Result<ExitCode, String> {
    let (c, _events) = ctx.connect().await?;
    let work = if a.command {
        last_command(&c, Some(a.from.clone())).await?
    } else {
        tail(&screen(&c, Some(a.from.clone())).await?, a.lines)
    };
    let name = block_name(&c, &a.from).await;
    let message = a.message.join(" ");
    let text = handoff_text(&name, &work, &message);
    let timeout = a.wait.then(|| parse_duration(&a.timeout)).transpose()?;
    send_prompt(&c, ctx, &a.to, &text, timeout, a.lines).await
}

pub async fn context(a: ContextArgs, ctx: &Ctx) -> Result<ExitCode, String> {
    let (c, _events) = ctx.connect().await?;
    if c.server_revision() < CONTEXT_REVISION {
        return Err(
            "the FerrumTerm server is older than this fterm and has no shared context: run `fterm server restart`"
                .into(),
        );
    }
    let session = a.session.or_else(|| {
        ctx.is_local()
            .then(|| std::env::var("FTERM_SESSION").ok())
            .flatten()
    });
    let (add, clear) = match a.cmd.unwrap_or(ContextCmd::Show) {
        ContextCmd::Show => (None, false),
        ContextCmd::Clear => (None, true),
        ContextCmd::Add { text } => {
            let text = text.join(" ");
            if text.trim().is_empty() {
                return Err("nothing to add: fterm context add <text>".into());
            }
            let author = match ctx.target(None) {
                Some(me) => block_name(&c, &me).await,
                None => "you".into(),
            };
            (Some(format!("[{author}] {}", text.trim())), false)
        }
    };
    let Response::Text { text } = c
        .request(Request::Context {
            session,
            add: add.clone(),
            clear,
        })
        .await
        .map_err(err)?
    else {
        return Err("unexpected reply".into());
    };
    if add.is_none() {
        if ctx.json {
            let notes: Vec<&str> = text.lines().collect();
            println!("{}", serde_json::json!({ "notes": notes }));
        } else if text.is_empty() {
            if !clear {
                println!("No notes yet. Add one with `fterm context add <text>`.");
            }
        } else {
            println!("{text}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Pastes `text` into `target` and presses Enter; with a timeout, waits
/// for the agent to finish and prints what it wrote.
async fn send_prompt(
    c: &Client,
    ctx: &Ctx,
    target: &str,
    text: &str,
    timeout: Option<Duration>,
    lines: usize,
) -> Result<ExitCode, String> {
    if text.trim().is_empty() {
        return Err("the prompt is empty".into());
    }
    let target = target.to_owned();
    let before = match timeout {
        Some(_) => Some(screen(c, Some(target.clone())).await?),
        None => None,
    };
    let input = |input| Request::Input {
        target: Some(target.clone()),
        input,
    };
    c.request(input(vec![Input::Paste(text.to_owned())]))
        .await
        .map_err(err)?;
    tokio::time::sleep(PASTE_SETTLE).await;
    c.request(input(vec![Input::Key("Enter".into())]))
        .await
        .map_err(err)?;
    let (Some(timeout), Some(before)) = (timeout, before) else {
        return Ok(ExitCode::SUCCESS);
    };

    let deadline = Instant::now() + timeout;
    let state = wait_finished(c, &target, deadline).await?;
    let after = screen(c, Some(target.clone())).await?;
    let answer = tail(&new_output(&before, &after), lines);
    if ctx.json {
        println!(
            "{}",
            serde_json::json!({ "status": state.map(ProgramState::as_str), "answer": answer })
        );
    } else {
        if let Some(s) = state {
            eprintln!("{target}: {}", s.as_str());
        }
        println!("{answer}");
    }
    Ok(match state {
        Some(ProgramState::Blocked) => ExitCode::from(2),
        Some(ProgramState::Error) => ExitCode::from(1),
        _ => ExitCode::SUCCESS,
    })
}

/// Waits until `target` starts working and then stops (done, idle, blocked
/// or error). A program that never reports working (a plain shell, or an
/// agent FerrumTerm doesn't recognise) is finished once its output settles.
async fn wait_finished(
    c: &Client,
    target: &str,
    deadline: Instant,
) -> Result<Option<ProgramState>, String> {
    let left = || deadline.saturating_duration_since(Instant::now());
    let wait = |status: Vec<ProgramState>, limit: Duration| {
        c.request(Request::Wait {
            target: Some(target.to_owned()),
            status,
            timeout_ms: Some(limit.as_millis() as u64),
        })
    };
    let started = match wait(vec![ProgramState::Working], left().min(START_WAIT)).await {
        Ok(Response::Waited {
            status: Some(_), ..
        }) => true,
        // It exited: nothing more will come.
        Ok(Response::Waited { exit: Some(_), .. }) => return Ok(None),
        Ok(_) | Err(_) => false,
    };
    if started {
        let finished = vec![
            ProgramState::Done,
            ProgramState::Idle,
            ProgramState::Blocked,
            ProgramState::Error,
        ];
        return match wait(finished, left()).await {
            Ok(Response::Waited { status, .. }) => Ok(status.map(|s| s.state)),
            Ok(_) => Err("unexpected reply".into()),
            Err(e) => Err(format!("{target} is still working: {e}")),
        };
    }
    // No status: wait for the screen to stop changing.
    let mut last = screen(c, Some(target.to_owned())).await?;
    let mut since = Instant::now();
    while since.elapsed() < QUIET {
        if Instant::now() >= deadline {
            return Err(format!("{target} kept printing until the timeout"));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        let now = screen(c, Some(target.to_owned())).await?;
        if now != last {
            last = now;
            since = Instant::now();
        }
    }
    Ok(None)
}

/// The block's screen and scrollback, soft-wrapped rows joined.
async fn screen(c: &Client, target: Option<String>) -> Result<String, String> {
    match c
        .request(Request::Capture {
            target,
            scrollback: true,
            unwrapped: true,
        })
        .await
        .map_err(err)?
    {
        Response::Text { text } => Ok(text),
        _ => Err("unexpected reply".into()),
    }
}

/// The last finished command of a shell (shell integration), with its output.
async fn last_command(c: &Client, target: Option<String>) -> Result<String, String> {
    let Response::Commands { commands } = c
        .request(Request::Commands {
            target: target.clone(),
        })
        .await
        .map_err(err)?
    else {
        return Err("unexpected reply".into());
    };
    let Some(cmd) = commands.iter().rev().find(|x| !x.running) else {
        return Err(
            "no finished command in this terminal (needs shell integration: zsh, fish or bash)"
                .into(),
        );
    };
    let Response::Text { text } = c
        .request(Request::CommandOutput {
            target,
            command: cmd.id,
        })
        .await
        .map_err(err)?
    else {
        return Err("unexpected reply".into());
    };
    Ok(format_command(&cmd.command, &text, cmd.exit))
}

fn format_command(command: &str, output: &str, exit: Option<i32>) -> String {
    let mut s = format!("$ {command}\n{}", output.trim_end());
    if let Some(code) = exit.filter(|c| *c != 0) {
        s.push_str(&format!("\n(exit code {code})"));
    }
    s
}

/// A block's label, else its id.
async fn block_name(c: &Client, target: &str) -> String {
    match c
        .request(Request::Inspect {
            target: target.to_owned(),
        })
        .await
    {
        Ok(Response::Inspected {
            item: Inspected::Block(b),
        }) => match b.label.filter(|l| !l.is_empty()) {
            Some(l) => format!("{l} ({})", b.id),
            None => b.id.to_string(),
        },
        _ => target.to_owned(),
    }
}

fn handoff_text(from: &str, work: &str, message: &str) -> String {
    let ask = if message.trim().is_empty() {
        "Continue from here."
    } else {
        message.trim()
    };
    format!(
        "Context from the terminal {from}:\n```\n{}\n```\n{ask}",
        work.trim_end()
    )
}

/// What `after` added to `before`: the lines after their common start.
/// A redrawn bottom area (an agent's input box) counts as new.
fn new_output(before: &str, after: &str) -> String {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let same = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    new[same..]
        .iter()
        .copied()
        .skip_while(|l| l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_output_is_what_follows_the_common_start() {
        let before = "$ ls\na b\n> ";
        let after = "$ ls\na b\n> review the diff\n\nLooks good.\n> ";
        assert_eq!(
            new_output(before, after),
            "> review the diff\n\nLooks good.\n> "
        );
        assert_eq!(new_output("same", "same"), "");
        assert_eq!(new_output("", "\n\nhello"), "hello");
    }

    #[test]
    fn tail_keeps_the_last_lines() {
        assert_eq!(tail("1\n2\n3", 2), "2\n3");
        assert_eq!(tail("1\n2", 5), "1\n2");
    }

    #[test]
    fn handoff_wraps_the_work_and_asks() {
        let t = handoff_text("claude (b3)", "fixed it\n", "");
        assert_eq!(
            t,
            "Context from the terminal claude (b3):\n```\nfixed it\n```\nContinue from here."
        );
        assert!(handoff_text("b3", "x", "  review it ").ends_with("\nreview it"));
    }

    #[test]
    fn commands_show_a_failing_exit_code() {
        assert_eq!(format_command("ls", "a\n", Some(0)), "$ ls\na");
        assert_eq!(
            format_command("cargo test", "boom", Some(101)),
            "$ cargo test\nboom\n(exit code 101)"
        );
    }
}
