//! `fterm port <N>`: which process listens on a TCP port (PID, name, user,
//! command line), with an optional, confirmed kill; `fterm ports` lists
//! every listening TCP port. Runs on this machine, not on the server.

use crate::util::Ctx;
use ferrumterm_net::ports::{self, Listener, Signal};
use std::io::Write as _;
use std::process::ExitCode;
use std::time::{Duration, Instant};

fn print_table(listeners: &[Listener]) {
    println!("PORT\tPID\tPROCESS\tUSER\tADDRESS\tCOMMAND");
    for l in listeners {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            l.port,
            l.pid.map_or("-".into(), |p| p.to_string()),
            l.process,
            l.user.as_deref().unwrap_or("-"),
            l.address,
            l.command.as_deref().unwrap_or("")
        );
    }
}

fn confirm(question: &str) -> bool {
    print!("{question} [y/N] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).is_ok()
        && matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn wait_gone(pid: u32, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if !ports::is_alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    !ports::is_alive(pid)
}

/// Asks before SIGTERM, then (if it is still running) before SIGKILL.
fn kill_interactively(listeners: &[Listener]) -> Result<(), String> {
    let mut pids: Vec<(u32, String)> = listeners
        .iter()
        .filter_map(|l| l.pid.map(|p| (p, l.process.clone())))
        .collect();
    pids.dedup_by_key(|(p, _)| *p);
    if pids.is_empty() {
        return Err("no process to kill (it may belong to another user: try sudo)".into());
    }
    for (pid, name) in pids {
        if !confirm(&format!("Send SIGTERM to {name} (pid {pid})?")) {
            println!("Left {name} running.");
            continue;
        }
        ports::signal(pid, Signal::Term)?;
        if wait_gone(pid, Duration::from_secs(3)) {
            println!("{name} (pid {pid}) exited.");
            continue;
        }
        if confirm(&format!(
            "{name} (pid {pid}) is still running. Force it with SIGKILL?"
        )) {
            ports::signal(pid, Signal::Kill)?;
            if wait_gone(pid, Duration::from_secs(2)) {
                println!("{name} (pid {pid}) killed.");
            } else {
                println!("{name} (pid {pid}) is still running.");
            }
        }
    }
    Ok(())
}

pub fn port(port: u16, kill: bool, ctx: &Ctx) -> Result<ExitCode, String> {
    let report = ports::lookup(port)?;
    if ctx.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
    } else if !report.listeners.is_empty() {
        print_table(&report.listeners);
    } else if report.in_use_by_other {
        println!(
            "Port {port} is in use by a process you can't see (another user's?). Try: sudo lsof -nP -iTCP:{port} -sTCP:LISTEN"
        );
    } else {
        println!("Nothing is listening on TCP port {port}.");
    }
    if kill && !report.listeners.is_empty() {
        kill_interactively(&report.listeners)?;
    }
    Ok(if report.listeners.is_empty() && !report.in_use_by_other {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

pub fn list(ctx: &Ctx) -> Result<ExitCode, String> {
    let listeners = ports::listeners(None)?;
    if ctx.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&listeners).map_err(|e| e.to_string())?
        );
    } else if listeners.is_empty() {
        println!("No listening TCP ports found.");
    } else {
        print_table(&listeners);
    }
    Ok(ExitCode::SUCCESS)
}
