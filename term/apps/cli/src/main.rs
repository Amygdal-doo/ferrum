//! `fterm` — FerrumTerm's command line: manage the persistent server, create
//! and drive sessions, windows and blocks (for you and for AI agents), and
//! attach to them from any terminal, tmux-style.

mod attach;
mod cmd;
mod forward;
mod hosts;
mod ports;
mod server;
mod server_status;
mod skill;
mod ssh;
mod util;

use clap::{Parser, Subcommand};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "fterm", version, about = "FerrumTerm: a persistent, agent-friendly terminal multiplexer", long_about = None)]
pub struct Cli {
    /// Server: a saved host name, `tcp://host:port`, or a socket path. Default: the local server.
    #[arg(short = 'S', long, global = true, env = "FTERM_SERVER")]
    pub server: Option<String>,
    /// Token for remote (TCP) servers.
    #[arg(long, global = true, env = "FTERM_TOKEN", hide_env_values = true)]
    pub token: Option<String>,
    /// Machine-readable output.
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Create a session (with one window and one block).
    New(cmd::NewArgs),
    /// List sessions, windows and blocks with program status.
    #[command(alias = "list")]
    Ls,
    /// Run a command in a new block (split beside you when inside FerrumTerm, else a new session).
    Run(cmd::RunArgs),
    /// Split a block.
    Split(cmd::SplitArgs),
    /// Open a new window (tab) in a session.
    Window(cmd::WindowArgs),
    /// Type text into a block.
    Send(cmd::SendArgs),
    /// Press keys in a block: Enter, Tab, Escape, Up, C-c, M-f, F5…
    SendKey(cmd::SendKeyArgs),
    /// Print a block's screen (and scrollback).
    Capture(cmd::CaptureArgs),
    /// Close sessions, windows or blocks (their programs are terminated).
    Kill { targets: Vec<String> },
    /// Rename a session, window or block.
    Rename { target: String, name: String },
    /// Focus a block, window or session.
    Focus { target: String },
    /// Show details of a session, window or block (JSON).
    Inspect { target: String },
    /// Wait until a block exits or reports a status.
    Wait(cmd::WaitArgs),
    /// Report program status (working / blocked / done / error / idle) for a block.
    Status(cmd::StatusArgs),
    /// Stream server events.
    Events,
    /// Attach to a session in this terminal (tmux-style; prefix C-b, then d to detach).
    #[command(alias = "a")]
    Attach { target: Option<String> },
    /// Keep the machine awake: auto (while agents work), always, never.
    Awake { mode: String },
    /// Manage the server.
    #[command(subcommand)]
    Server(server::ServerCmd),
    /// Teach AI agents (Claude Code, Codex) to use fterm.
    #[command(subcommand)]
    Skill(skill::SkillCmd),
    /// Saved remote servers (other Macs on your tailnet).
    #[command(subcommand)]
    Hosts(hosts::HostsCmd),
    /// Open a tab running ssh to a saved or ~/.ssh/config host; `--list`; `add` / `remove`.
    Ssh(ssh::SshArgs),
    /// SSH port forwards (-L / -R / -D), run by the server: list, add, remove, start, stop.
    #[command(subcommand)]
    Forward(forward::ForwardCmd),
    /// What's listening on a TCP port: PID, process, user, command line.
    Port {
        port: u16,
        /// Offer to stop it (asks before SIGTERM, and again before SIGKILL).
        #[arg(long)]
        kill: bool,
    },
    /// List every listening TCP port and its process.
    Ports,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("fterm: {e}");
            return ExitCode::FAILURE;
        }
    };
    match rt.block_on(dispatch(cli)) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("fterm: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn dispatch(cli: Cli) -> Result<ExitCode, String> {
    let ctx = util::Ctx {
        server: cli.server.clone(),
        token: cli.token.clone(),
        json: cli.json,
    };
    match cli.cmd {
        Cmd::Server(c) => server::run(c, &ctx).await,
        Cmd::Skill(c) => skill::run(c),
        Cmd::Hosts(c) => hosts::run(c, &ctx),
        Cmd::Ssh(a) => ssh::run(a, &ctx).await,
        Cmd::Forward(c) => forward::run(c, &ctx).await,
        Cmd::Port { port, kill } => ports::port(port, kill, &ctx),
        Cmd::Ports => ports::list(&ctx),
        Cmd::Attach { target } => attach::run(&ctx, target).await,
        other => cmd::run(other, &ctx).await,
    }
}
