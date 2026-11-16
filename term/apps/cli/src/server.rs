//! `fterm server …`: run, start (detached), stop, restart, status, logs, token,
//! install / uninstall as a macOS LaunchAgent (starts at login, restarts if it dies).

use crate::server_status;
use crate::util::Ctx;
use clap::{Args, Subcommand};
use ferrumterm_client::{Client, Endpoint, paths};
use ferrumterm_protocol::{AwakeMode, DEFAULT_PORT, Request};
use ferrumterm_server::{ServerConfig, check_listen_addr};
use std::net::SocketAddr;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

const LAUNCH_LABEL: &str = "dev.ferrumterm.server";

#[derive(Args, Clone)]
pub struct ServeArgs {
    /// Extra TCP listener (loopback or Tailscale address), e.g. 100.101.102.103:7397. Token required.
    #[arg(long)]
    pub listen: Vec<SocketAddr>,
    /// Listen on this Mac's Tailscale address (port 7397) for remote clients.
    #[arg(long)]
    pub tailscale: bool,
    /// Keep the machine awake: auto (while agents work), always, never.
    #[arg(long, default_value = "auto")]
    pub awake: String,
    /// No desktop notifications for done / blocked / error.
    #[arg(long)]
    pub no_notify: bool,
}

impl ServeArgs {
    fn to_args(&self) -> Vec<String> {
        let mut v = vec!["--awake".to_owned(), self.awake.clone()];
        for l in &self.listen {
            v.extend(["--listen".to_owned(), l.to_string()]);
        }
        if self.tailscale {
            v.push("--tailscale".into());
        }
        if self.no_notify {
            v.push("--no-notify".into());
        }
        v
    }
}

#[derive(Subcommand)]
pub enum ServerCmd {
    /// Run the server in the foreground.
    Run(ServeArgs),
    /// Start the server in the background (no-op if it is running).
    Start(ServeArgs),
    /// Stop the server. All programs in it are terminated.
    Stop,
    /// Stop the server and start this fterm's build. All programs in it are terminated.
    Restart {
        /// Don't ask for confirmation.
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Show server status (its build, and whether it is older than this fterm).
    Status,
    /// Show the server log.
    Logs {
        #[arg(short = 'n', long, default_value_t = 50)]
        lines: usize,
        #[arg(short = 'f', long)]
        follow: bool,
    },
    /// Print the token remote clients need (`fterm hosts add … --token`).
    Token,
    /// Start the server at login and restart it if it dies (macOS LaunchAgent).
    Install(ServeArgs),
    /// Remove the LaunchAgent.
    Uninstall,
}

fn awake_mode(s: &str) -> Result<AwakeMode, String> {
    match s {
        "auto" => Ok(AwakeMode::Auto),
        "always" | "on" => Ok(AwakeMode::Always),
        "never" | "off" => Ok(AwakeMode::Never),
        m => Err(format!("unknown awake mode {m}")),
    }
}

/// This Mac's Tailscale IPv4 address via the `tailscale` CLI.
fn tailscale_ip() -> Result<SocketAddr, String> {
    let candidates = [
        "tailscale",
        "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        "/usr/local/bin/tailscale",
        "/opt/homebrew/bin/tailscale",
    ];
    for bin in candidates {
        if let Ok(out) = Command::new(bin)
            .args(["ip", "-4"])
            .stderr(Stdio::null())
            .output()
            && out.status.success()
            && let Some(ip) = String::from_utf8_lossy(&out.stdout).lines().next()
        {
            return format!("{}:{DEFAULT_PORT}", ip.trim())
                .parse()
                .map_err(|e| format!("bad tailscale address {ip}: {e}"));
        }
    }
    Err("Tailscale is not running or its CLI was not found; install Tailscale and sign in, or pass --listen <tailnet-ip>:7397".into())
}

fn config(a: &ServeArgs) -> Result<ServerConfig, String> {
    let mut tcp = a.listen.clone();
    if a.tailscale {
        tcp.push(tailscale_ip()?);
    }
    for addr in &tcp {
        check_listen_addr(addr)?;
    }
    Ok(ServerConfig {
        state_dir: paths::state_dir(),
        socket: paths::socket(),
        tcp,
        awake: awake_mode(&a.awake)?,
        notify: !a.no_notify,
    })
}

async fn local() -> Option<Client> {
    Client::connect(&Endpoint::Unix(paths::socket()), "fterm")
        .await
        .ok()
        .map(|(c, _)| c)
}

pub async fn run(cmd: ServerCmd, ctx: &Ctx) -> Result<ExitCode, String> {
    match cmd {
        ServerCmd::Run(a) => {
            let cfg = config(&a)?;
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".into()),
                )
                .with_target(false)
                .init();
            ferrumterm_server::run(cfg)
                .await
                .map_err(|e| e.to_string())?;
        }
        ServerCmd::Start(a) => {
            if local().await.is_some() {
                println!(
                    "FerrumTerm server already running ({})",
                    paths::socket().display()
                );
                return Ok(ExitCode::SUCCESS);
            }
            config(&a)?;
            let pid = start_detached(&a.to_args()).await?;
            println!("FerrumTerm server started (pid {pid})");
        }
        ServerCmd::Restart { yes } => restart(yes, ctx).await?,
        ServerCmd::Stop => {
            match ctx.connect_existing().await {
                Ok((c, _)) => {
                    let _ = c
                        .request_timeout(Request::Shutdown, Duration::from_secs(5))
                        .await;
                }
                // A server from an older build: stop it by its process.
                Err(e) => {
                    let socket = ferrumterm_client::paths::socket();
                    if !ferrumterm_client::socket_is_live(&socket).await {
                        return Err(e);
                    }
                    ferrumterm_client::force_stop(&socket).await?;
                }
            }
            println!("FerrumTerm server stopped");
        }
        ServerCmd::Status => {
            let (c, _) = ctx.connect_existing().await?;
            let info = c.info().await.map_err(|e| e.to_string())?;
            let agent = if ctx.is_local() {
                launch_agent_note()
            } else {
                None
            };
            if ctx.json {
                let mut v = server_status::status_json(&info);
                if let (Some(o), Some(note)) = (v.as_object_mut(), &agent) {
                    o.insert("launch_agent".into(), note.clone().into());
                }
                println!(
                    "{}",
                    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?
                );
            } else {
                for line in server_status::status_lines(&info, now_ms(), ctx.is_local()) {
                    println!("{line}");
                }
                if let Some(note) = agent {
                    println!("  {note}");
                }
            }
        }
        ServerCmd::Logs { lines, follow } => {
            let path = paths::log_file();
            if follow {
                let status = Command::new("tail")
                    .args(["-n", &lines.to_string(), "-f"])
                    .arg(&path)
                    .status()
                    .map_err(|e| e.to_string())?;
                return Ok(if status.success() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                });
            }
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let all: Vec<&str> = text.lines().collect();
            println!("{}", all[all.len().saturating_sub(lines)..].join("\n"));
        }
        ServerCmd::Token => {
            let token = ferrumterm_server::load_or_create_token(&paths::state_dir())
                .map_err(|e| e.to_string())?;
            println!("{token}");
        }
        ServerCmd::Install(a) => install(&a)?,
        ServerCmd::Uninstall => {
            let plist = plist_path()?;
            let _ = Command::new("launchctl")
                .args([
                    "bootout",
                    &format!("gui/{}", uid()?),
                    &plist.display().to_string(),
                ])
                .status();
            let _ = std::fs::remove_file(&plist);
            println!("Removed {}", plist.display());
        }
    }
    Ok(ExitCode::SUCCESS)
}

impl Ctx {
    /// Connects without auto-starting a local server.
    pub async fn connect_existing(
        &self,
    ) -> Result<
        (
            Client,
            tokio::sync::mpsc::UnboundedReceiver<ferrumterm_protocol::Event>,
        ),
        String,
    > {
        if self.is_local() {
            return Client::connect(&Endpoint::Unix(paths::socket()), "fterm")
                .await
                .map_err(|_| "the FerrumTerm server is not running".to_owned());
        }
        self.connect().await
    }
}

/// Starts `fterm server run <args>` from this executable in the background
/// and waits for it to answer; returns its pid.
async fn start_detached(args: &[String]) -> Result<u32, String> {
    let dir = paths::state_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths::log_file())
        .map_err(|e| e.to_string())?;
    let log2 = log.try_clone().map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let child = Command::new(exe)
        .args(["server", "run"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(log2)
        // Own process group: closing this terminal does not take the server down.
        .process_group(0)
        .spawn()
        .map_err(|e| format!("could not start the server: {e}"))?;
    for _ in 0..50 {
        if local().await.is_some() {
            return Ok(child.id());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "server did not come up; see {}",
        paths::log_file().display()
    ))
}

/// `fterm server restart`: confirm, stop the running server (even one too
/// old to talk to), start this build with the same listeners and keep-awake mode.
async fn restart(yes: bool, ctx: &Ctx) -> Result<(), String> {
    if !ctx.is_local() {
        return Err(
            "`fterm server restart` restarts this Mac's server; run it on the server's machine"
                .into(),
        );
    }
    let socket = paths::socket();
    let running = local().await;
    let info = match &running {
        Some(c) => c.info().await.ok(),
        None => None,
    };
    let live = running.is_some() || ferrumterm_client::socket_is_live(&socket).await;
    if live {
        let counts = info.as_ref().map(|i| (i.sessions, i.blocks));
        if !yes && counts != Some((0, 0)) {
            eprintln!("{}", server_status::restart_warning(counts));
            if !confirm("Restart now? [y/N] ")? {
                eprintln!("Not restarted.");
                return Ok(());
            }
        }
        drop(running);
        ferrumterm_client::force_stop(&socket).await?;
    }
    let args = info
        .as_ref()
        .map(server_status::restart_args)
        .unwrap_or_else(|| vec!["--awake".into(), "auto".into()]);
    let pid = start_detached(&args).await?;
    println!(
        "FerrumTerm server {} (revision {}) {} (pid {pid})",
        ferrumterm_protocol::build_version(),
        ferrumterm_protocol::SERVER_REVISION,
        if live { "restarted" } else { "started" }
    );
    if let Some(note) = launch_agent_note() {
        println!("  {note}");
    }
    Ok(())
}

/// Asks on the terminal; refuses without one (scripts pass `--yes`).
fn confirm(prompt: &str) -> Result<bool, String> {
    use std::io::{BufRead, IsTerminal, Write};
    if !std::io::stdin().is_terminal() {
        return Err("not a terminal; pass --yes to restart without asking".into());
    }
    eprint!("{prompt}");
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(|e| e.to_string())?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes"))
}

/// The installed LaunchAgent, when it starts a binary other than this one.
fn launch_agent_note() -> Option<String> {
    let plist = plist_path().ok()?;
    let text = std::fs::read_to_string(&plist).ok()?;
    let exe = std::env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    server_status::launch_agent_note(&plist, &text, &exe, Path::is_file)
        // Compare resolved paths (symlinked installs).
        .filter(|_| {
            server_status::plist_program(&text)
                .and_then(|p| Path::new(&p).canonicalize().ok())
                .is_none_or(|p| p != exe)
        })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn uid() -> Result<String, String> {
    let out = Command::new("id")
        .arg("-u")
        .output()
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn plist_path() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME is not set".to_owned())?;
    Ok(PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{LAUNCH_LABEL}.plist")))
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn install(a: &ServeArgs) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("install uses launchd (macOS); on Linux run `fterm server run` from a systemd user unit".into());
    }
    config(a)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut args = vec![exe.display().to_string(), "server".into(), "run".into()];
    args.extend(a.to_args());
    let path_env = std::env::var("PATH").unwrap_or_default();
    let log = paths::log_file().display().to_string();
    let program = args
        .iter()
        .map(|x| format!("    <string>{}</string>", xml(x)))
        .collect::<Vec<_>>()
        .join("\n");
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LAUNCH_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
{program}
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key><string>{}</string>
  </dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
  <key>ProcessType</key><string>Interactive</string>
  <key>StandardOutPath</key><string>{}</string>
  <key>StandardErrorPath</key><string>{}</string>
</dict>
</plist>
"#,
        xml(&path_env),
        xml(&log),
        xml(&log)
    );
    let path = plist_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(paths::state_dir()).map_err(|e| e.to_string())?;
    std::fs::write(&path, plist).map_err(|e| e.to_string())?;
    let domain = format!("gui/{}", uid()?);
    let _ = Command::new("launchctl")
        .args(["bootout", &domain, &path.display().to_string()])
        .stderr(Stdio::null())
        .status();
    let ok = Command::new("launchctl")
        .args(["bootstrap", &domain, &path.display().to_string()])
        .status()
        .map_err(|e| e.to_string())?;
    if !ok.success() {
        return Err(format!(
            "launchctl bootstrap failed; the plist is at {}",
            path.display()
        ));
    }
    println!(
        "Installed {}: the FerrumTerm server now starts at login and restarts if it stops.",
        path.display()
    );
    Ok(())
}
