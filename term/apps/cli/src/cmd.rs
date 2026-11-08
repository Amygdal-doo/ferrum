//! Session, window and block commands.

use crate::Cmd;
use crate::util::{Ctx, parse_duration, passthrough_env};
use clap::Args;
use ferrumterm_client::Client;
use ferrumterm_protocol::{
    AwakeMode, Direction, Event, Input, ProgramState, ProgramStatus, Request, Response,
    SessionInfo, Spawn,
};
use std::process::ExitCode;

#[derive(Args)]
pub struct SpawnArgs {
    /// Working directory (default: your current directory).
    #[arg(short = 'c', long)]
    pub cwd: Option<String>,
    /// Name for the block (usable as a target).
    #[arg(short = 'l', long)]
    pub label: Option<String>,
    /// Keep the block after the program exits.
    #[arg(long)]
    pub keep_open: bool,
    /// Command to run (default: your shell).
    #[arg(last = true)]
    pub command: Vec<String>,
}

impl SpawnArgs {
    fn spawn(&self) -> Spawn {
        Spawn {
            command: self.command.clone(),
            cwd: self.cwd.clone().or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|d| d.display().to_string())
            }),
            label: self.label.clone(),
            keep_open: self.keep_open,
            env: passthrough_env(),
        }
    }
}

#[derive(Args)]
pub struct NewArgs {
    /// Session name.
    pub name: Option<String>,
    /// Attach to it right away.
    #[arg(short = 'a', long)]
    pub attach: bool,
    #[command(flatten)]
    pub spawn: SpawnArgs,
}

#[derive(Args)]
pub struct RunArgs {
    /// Split this direction from the target: right, down, left, up.
    #[arg(long)]
    pub split: Option<String>,
    /// Open in a new window (tab) instead.
    #[arg(long, conflicts_with = "split")]
    pub window: bool,
    /// Start a new session instead.
    #[arg(long, conflicts_with_all = ["split", "window"])]
    pub session: bool,
    /// Size of the new block (0.1–0.9) when splitting.
    #[arg(long)]
    pub ratio: Option<f32>,
    /// Block / window / session to split from or open in.
    #[arg(short = 't', long)]
    pub target: Option<String>,
    /// Move focus to the new block.
    #[arg(long)]
    pub focus: bool,
    /// Wait for the program to exit and use its exit code.
    #[arg(short = 'w', long)]
    pub wait: bool,
    #[command(flatten)]
    pub spawn: SpawnArgs,
}

#[derive(Args)]
pub struct SplitArgs {
    #[arg(short = 'd', long, default_value = "right")]
    pub direction: String,
    #[arg(long)]
    pub ratio: Option<f32>,
    #[arg(short = 't', long)]
    pub target: Option<String>,
    #[arg(long)]
    pub no_focus: bool,
    #[command(flatten)]
    pub spawn: SpawnArgs,
}

#[derive(Args)]
pub struct WindowArgs {
    /// Session to add the window to.
    #[arg(short = 's', long)]
    pub session: Option<String>,
    #[arg(short = 'n', long)]
    pub name: Option<String>,
    #[command(flatten)]
    pub spawn: SpawnArgs,
}

#[derive(Args)]
pub struct SendArgs {
    #[arg(short = 't', long)]
    pub target: Option<String>,
    /// Press Enter after the text.
    #[arg(short = 'e', long)]
    pub enter: bool,
    /// Send as a paste (bracketed when the program supports it).
    #[arg(long)]
    pub paste: bool,
    pub text: Vec<String>,
}

#[derive(Args)]
pub struct SendKeyArgs {
    #[arg(short = 't', long)]
    pub target: Option<String>,
    #[arg(required = true)]
    pub keys: Vec<String>,
}

#[derive(Args)]
pub struct CaptureArgs {
    #[arg(short = 't', long)]
    pub target: Option<String>,
    /// Include scrollback history.
    #[arg(short = 'a', long)]
    pub all: bool,
    /// Only the last N lines.
    #[arg(short = 'n', long)]
    pub lines: Option<usize>,
}

#[derive(Args)]
pub struct WaitArgs {
    #[arg(short = 't', long)]
    pub target: Option<String>,
    /// Also stop when the block reports one of these states (comma-separated): done,error,blocked,idle,working.
    #[arg(long, value_delimiter = ',')]
    pub status: Vec<String>,
    /// Give up after this long (e.g. 30s, 10m).
    #[arg(long)]
    pub timeout: Option<String>,
}

#[derive(Args)]
pub struct StatusArgs {
    /// working | blocked | done | error | idle | clear
    pub state: String,
    #[arg(short = 't', long)]
    pub target: Option<String>,
    #[arg(short = 'm', long)]
    pub message: Option<String>,
    /// 0–100
    #[arg(short = 'p', long)]
    pub progress: Option<u8>,
    /// For blocked: permission | question | auth
    #[arg(short = 'k', long)]
    pub kind: Option<String>,
    /// Print the OSC 7501 sequence instead of telling the server.
    #[arg(long)]
    pub emit: bool,
}

fn created(ctx: &Ctx, r: &Response) -> String {
    match r {
        Response::Created { session, window, block } if ctx.json => {
            serde_json::json!({ "session": session.to_string(), "window": window.to_string(), "block": block.to_string() }).to_string()
        }
        Response::Created { session, window, block } => format!("{session} {window} {block}"),
        _ => String::new(),
    }
}

pub async fn run(cmd: Cmd, ctx: &Ctx) -> Result<ExitCode, String> {
    if let Cmd::Status(a) = &cmd
        && a.emit
    {
        let status = parse_status(a)?;
        print!(
            "{}",
            status.map_or_else(|| "\x1b]7501;state=clear\x1b\\".to_owned(), |s| s.to_osc())
        );
        return Ok(ExitCode::SUCCESS);
    }
    let (c, mut events) = ctx.connect().await?;
    let req = |r| c.request(r);
    let e = |e: ferrumterm_client::ClientError| e.to_string();
    match cmd {
        Cmd::New(a) => {
            let r = req(Request::NewSession {
                name: a.name.clone(),
                spawn: a.spawn.spawn(),
            })
            .await
            .map_err(e)?;
            println!("{}", created(ctx, &r));
            if a.attach
                && let Response::Created { session, .. } = r
            {
                return crate::attach::run(ctx, Some(session.to_string())).await;
            }
        }
        Cmd::Ls => {
            let Response::Sessions { sessions } = req(Request::List).await.map_err(e)? else {
                return Err("unexpected reply".into());
            };
            if ctx.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&sessions).map_err(|e| e.to_string())?
                );
            } else {
                print_tree(&sessions);
            }
        }
        Cmd::Run(a) => {
            let spawn = a.spawn.spawn();
            let target = ctx.target(a.target.clone());
            let r = if a.session || (target.is_none() && !a.window && a.split.is_none()) {
                req(Request::NewSession {
                    name: a.spawn.label.clone(),
                    spawn,
                })
                .await
            } else if a.window {
                req(Request::NewWindow {
                    session: target,
                    name: a.spawn.label.clone(),
                    spawn,
                })
                .await
            } else {
                let dir = a.split.as_deref().map_or(Ok(Direction::Right), parse_dir)?;
                req(Request::Split {
                    target,
                    dir,
                    ratio: a.ratio,
                    spawn,
                    focus: a.focus,
                })
                .await
            }
            .map_err(e)?;
            println!("{}", created(ctx, &r));
            if a.wait
                && let Response::Created { block, .. } = r
            {
                return wait_exit(&c, block.to_string(), Vec::new(), None).await;
            }
        }
        Cmd::Split(a) => {
            let r = req(Request::Split {
                target: ctx.target(a.target.clone()),
                dir: parse_dir(&a.direction)?,
                ratio: a.ratio,
                spawn: a.spawn.spawn(),
                focus: !a.no_focus,
            })
            .await
            .map_err(e)?;
            println!("{}", created(ctx, &r));
        }
        Cmd::Window(a) => {
            let session = a.session.or_else(|| {
                ctx.is_local()
                    .then(|| std::env::var("FTERM_SESSION").ok())
                    .flatten()
            });
            let r = req(Request::NewWindow {
                session,
                name: a.name,
                spawn: a.spawn.spawn(),
            })
            .await
            .map_err(e)?;
            println!("{}", created(ctx, &r));
        }
        Cmd::Send(a) => {
            let text = a.text.join(" ");
            let mut input = vec![if a.paste {
                Input::Paste(text)
            } else {
                Input::Text(text)
            }];
            if a.enter {
                input.push(Input::Key("Enter".into()));
            }
            req(Request::Input {
                target: ctx.target(a.target),
                input,
            })
            .await
            .map_err(e)?;
        }
        Cmd::SendKey(a) => {
            let input = a.keys.into_iter().map(Input::Key).collect();
            req(Request::Input {
                target: ctx.target(a.target),
                input,
            })
            .await
            .map_err(e)?;
        }
        Cmd::Capture(a) => {
            let Response::Text { text } = req(Request::Capture {
                target: ctx.target(a.target),
                scrollback: a.all,
            })
            .await
            .map_err(e)?
            else {
                return Err("unexpected reply".into());
            };
            match a.lines {
                Some(n) => {
                    let lines: Vec<&str> = text.lines().collect();
                    println!("{}", lines[lines.len().saturating_sub(n)..].join("\n"));
                }
                None => println!("{text}"),
            }
        }
        Cmd::Kill { targets } => {
            if targets.is_empty() {
                return Err("name what to kill: fterm kill <session|window|block>…".into());
            }
            for t in targets {
                req(Request::Close { target: t }).await.map_err(e)?;
            }
        }
        Cmd::Rename { target, name } => {
            req(Request::Rename { target, name }).await.map_err(e)?;
        }
        Cmd::Focus { target } => {
            req(Request::Focus { target }).await.map_err(e)?;
        }
        Cmd::Inspect { target } => {
            let Response::Inspected { item } = req(Request::Inspect { target }).await.map_err(e)?
            else {
                return Err("unexpected reply".into());
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&item).map_err(|e| e.to_string())?
            );
        }
        Cmd::Wait(a) => {
            let states = a
                .status
                .iter()
                .map(|s| ProgramState::parse(s.trim()).ok_or_else(|| format!("unknown status {s}")))
                .collect::<Result<Vec<_>, _>>()?;
            let target = ctx
                .target(a.target)
                .ok_or("name a block to wait for: fterm wait -t <block>")?;
            let timeout = a.timeout.as_deref().map(parse_duration).transpose()?;
            return wait_exit(&c, target, states, timeout).await;
        }
        Cmd::Status(a) => {
            let status = parse_status(&a)?;
            req(Request::SetStatus {
                target: ctx.target(a.target),
                status,
            })
            .await
            .map_err(e)?;
        }
        Cmd::Events => {
            req(Request::Watch).await.map_err(e)?;
            while let Some(ev) = events.recv().await {
                if matches!(ev, Event::Frame { .. }) {
                    continue;
                }
                if ctx.json {
                    println!("{}", serde_json::to_string(&ev).map_err(|e| e.to_string())?);
                } else {
                    println!("{}", describe(&ev));
                }
                if matches!(ev, Event::Bye) {
                    break;
                }
            }
        }
        Cmd::Awake { mode } => {
            let mode = match mode.as_str() {
                "auto" => AwakeMode::Auto,
                "always" | "on" => AwakeMode::Always,
                "never" | "off" => AwakeMode::Never,
                m => return Err(format!("unknown mode {m}: use auto, always or never")),
            };
            req(Request::SetAwake { mode }).await.map_err(e)?;
        }
        Cmd::Attach { .. } | Cmd::Server(_) | Cmd::Skill(_) | Cmd::Hosts(_) => {
            unreachable!("dispatched in main")
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn parse_dir(s: &str) -> Result<Direction, String> {
    Direction::parse(s).ok_or_else(|| format!("unknown direction {s}: use right, down, left or up"))
}

fn parse_status(a: &StatusArgs) -> Result<Option<ProgramStatus>, String> {
    if a.state == "clear" {
        return Ok(None);
    }
    let state = ProgramState::parse(&a.state).ok_or_else(|| {
        format!(
            "unknown state {}: working, blocked, done, error, idle or clear",
            a.state
        )
    })?;
    let mut s = ProgramStatus::new(state);
    s.message.clone_from(&a.message);
    s.progress = a.progress.map(|p| p.min(100));
    s.kind.clone_from(&a.kind);
    Ok(Some(s))
}

async fn wait_exit(
    c: &Client,
    target: String,
    status: Vec<ProgramState>,
    timeout: Option<std::time::Duration>,
) -> Result<ExitCode, String> {
    let timeout_ms = timeout.map(|d| d.as_millis() as u64);
    match c
        .request(Request::Wait {
            target: Some(target),
            status,
            timeout_ms,
        })
        .await
        .map_err(|e| e.to_string())?
    {
        Response::Waited {
            status: Some(s),
            exit: None,
        } => {
            println!("{}", s.state.as_str());
            Ok(ExitCode::SUCCESS)
        }
        Response::Waited { exit, .. } => {
            let code = exit.unwrap_or(0);
            Ok(ExitCode::from(u8::try_from(code).unwrap_or(1)))
        }
        _ => Err("unexpected reply".into()),
    }
}

fn describe(e: &Event) -> String {
    match e {
        Event::Changed => "changed".into(),
        Event::Status {
            block,
            status: Some(s),
        } => format!(
            "status {block} {}{}",
            s.state.as_str(),
            s.message
                .as_deref()
                .map(|m| format!(" — {m}"))
                .unwrap_or_default()
        ),
        Event::Status {
            block,
            status: None,
        } => format!("status {block} cleared"),
        Event::Exited { block, code } => format!("exited {block} {code}"),
        Event::Bell { block } => format!("bell {block}"),
        Event::Bye => "server stopped".into(),
        Event::Frame { block, .. } => format!("frame {block}"),
    }
}

fn print_tree(sessions: &[SessionInfo]) {
    if sessions.is_empty() {
        println!("No sessions. Start one with `fterm new` or open FerrumTerm.");
        return;
    }
    for s in sessions {
        let badge = s
            .summary_status()
            .map(|st| format!(" {}", st.glyph()))
            .unwrap_or_default();
        println!("{} {}{}", s.id, s.name, badge);
        for w in &s.windows {
            let active = if w.id == s.active_window { "*" } else { "" };
            println!("  {} {}{} ({}x{})", w.id, w.name, active, w.cols, w.rows);
            for b in &w.blocks {
                let focus = if b.id == w.focused { ">" } else { " " };
                let name = b
                    .label
                    .clone()
                    .filter(|l| !l.is_empty())
                    .unwrap_or_else(|| {
                        if b.title.is_empty() {
                            b.command.first().map_or_else(
                                || "shell".into(),
                                |c| c.rsplit('/').next().unwrap_or(c).to_owned(),
                            )
                        } else {
                            b.title.clone()
                        }
                    });
                let status = b
                    .status
                    .as_ref()
                    .map(|st| {
                        let msg = st
                            .message
                            .as_deref()
                            .map(|m| format!(" {m}"))
                            .unwrap_or_default();
                        format!("  {} {}{msg}", st.state.glyph(), st.state.as_str())
                    })
                    .unwrap_or_default();
                let exited = b
                    .exited
                    .map(|c| format!("  (exited {c})"))
                    .unwrap_or_default();
                println!("   {focus}{} {name}{status}{exited}", b.id);
            }
        }
    }
}
