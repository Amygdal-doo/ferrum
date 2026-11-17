//! Listening ports of another machine, over the system `ssh`.
//!
//! Hosts are FerrumTerm's kind (`ferrumterm-net`): aliases from
//! `~/.ssh/config` and hosts typed as `user@host[:port]`, kept in
//! `~/.ferrum/ports/ssh-hosts.json`. One `ssh` run executes a small POSIX
//! script on the host that prints `ss` output (Linux) or `lsof` output
//! (macOS, BSD) and then `ps`, each after a marker line; the existing
//! parsers read them. `BatchMode` means keys must be in the agent or the
//! keychain: nothing can prompt for a password.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use ferrumterm_net::ssh_config::ConfigHost;
use ferrumterm_net::{Host, HostSource, HostStore, args};

use crate::kill::{self, KillError, Signal};
use crate::model::{Protocol, Snapshot};
use crate::scan::assemble;
use crate::{lsof, ps, ss};

/// Seconds ssh waits for the connection.
const CONNECT_TIMEOUT: &str = "10";
const MARK_SS: &str = "#ferrumports:ss";
const MARK_SS_UDP: &str = "#ferrumports:ss-udp";
const MARK_LSOF: &str = "#ferrumports:lsof";
const MARK_LSOF_UDP: &str = "#ferrumports:lsof-udp";
const MARK_PS: &str = "#ferrumports:ps";

/// `~/.ferrum/ports/ssh-hosts.json`: the hosts added in FerrumPorts.
#[must_use]
pub fn store_path(home: &Path) -> PathBuf {
    ferrumterm_net::hosts::store_path(&home.join(".ferrum").join("ports"))
}

/// The hosts to offer: the added ones (saved first, then `~/.ssh/config`
/// aliases), as FerrumTerm lists its machines.
#[must_use]
pub fn hosts(store: &HostStore, config: &[ConfigHost]) -> Vec<Host> {
    store.machines(config)
}

/// The shell script run on the host (POSIX `sh`).
#[must_use]
pub fn script(udp: bool) -> String {
    let join = |args: &[&str]| args.join(" ");
    let mut ss_part = format!("echo '{MARK_SS}'; ss {}", join(ss::TCP_ARGS));
    let mut lsof_part = format!("echo '{MARK_LSOF}'; lsof {}", join(lsof::TCP_ARGS));
    if udp {
        ss_part.push_str(&format!(
            "; echo '{MARK_SS_UDP}'; ss {}",
            join(ss::UDP_ARGS)
        ));
        lsof_part.push_str(&format!(
            "; echo '{MARK_LSOF_UDP}'; lsof {}",
            join(lsof::UDP_ARGS)
        ));
    }
    // `ps -A` instead of a pid list: one round trip.
    format!(
        "LC_ALL=C; export LC_ALL; PATH=\"$PATH:/usr/sbin:/sbin:/usr/bin\"; \
         if command -v ss >/dev/null 2>&1; then {ss_part}; \
         elif command -v lsof >/dev/null 2>&1; then {lsof_part}; \
         else echo 'neither ss nor lsof is installed' >&2; exit 3; fi; \
         echo '{MARK_PS}'; ps -A -ww -o pid=,user=,lstart=,command= 2>/dev/null; exit 0"
    )
}

/// Splits the script's output at the marker lines.
fn sections(text: &str) -> HashMap<&str, String> {
    let mut out: HashMap<&str, String> = HashMap::new();
    let mut current: Option<&str> = None;
    for line in text.lines() {
        if let Some(mark) = [MARK_SS, MARK_SS_UDP, MARK_LSOF, MARK_LSOF_UDP, MARK_PS]
            .into_iter()
            .find(|m| line.trim() == *m)
        {
            current = Some(mark);
            out.entry(mark).or_default();
            continue;
        }
        if let Some(mark) = current {
            let section = out.entry(mark).or_default();
            section.push_str(line);
            section.push('\n');
        }
    }
    out
}

/// Listeners from the script's output. Remote processes have no working
/// directory or project here.
#[must_use]
pub fn parse(text: &str) -> Snapshot {
    let sections = sections(text);
    let get = |mark: &str| sections.get(mark).map_or("", String::as_str);
    let mut sockets = ss::parse(get(MARK_SS), Protocol::Tcp);
    sockets.extend(ss::parse(get(MARK_SS_UDP), Protocol::Udp));
    sockets.extend(lsof::parse_sockets(get(MARK_LSOF)));
    sockets.extend(lsof::parse_sockets(get(MARK_LSOF_UDP)));
    let processes = ps::parse(get(MARK_PS));
    let mut warnings = Vec::new();
    if sections.contains_key(MARK_SS) && sockets.iter().any(|s| s.pid.is_none()) {
        warnings.push(
            "Some sockets have no process: `ss` shows other users' processes only to root."
                .to_string(),
        );
    }
    Snapshot {
        listeners: assemble(sockets, &processes, &HashMap::new(), None),
        warnings,
    }
}

/// `ssh [options] <host> <command>` without a terminal, failing instead of
/// prompting.
pub fn ssh_command(host: &Host, command: &str) -> Result<Vec<String>, String> {
    let mut argv = args::interactive(&args::ssh_binary(), host)?;
    // `interactive` asks for a terminal and appends a saved host's startup
    // command; neither belongs in a one-off command.
    if let HostSource::Saved(saved) = &host.source
        && saved
            .startup_command
            .as_deref()
            .is_some_and(|c| !c.trim().is_empty())
    {
        argv.pop();
    }
    argv.retain(|arg| arg != "-t");
    let options = [
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        &format!("ConnectTimeout={CONNECT_TIMEOUT}"),
    ];
    argv.splice(1..1, options.iter().map(|s| (*s).to_string()));
    argv.push(command.to_string());
    Ok(argv)
}

/// Runs `command` on `host`; `Err` holds ssh's message when it could not
/// connect (exit 255) or the command failed with no output.
fn run(host: &Host, command: &str) -> Result<(bool, String, String), String> {
    let argv = ssh_command(host, command)?;
    let output = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Could not run ssh: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.code() == Some(255) {
        return Err(if stderr.is_empty() {
            format!("ssh could not connect to {}", host.name)
        } else {
            stderr
        });
    }
    Ok((output.status.success(), stdout, stderr))
}

/// Lists the listening ports of `host`. Blocking: call it off the UI thread.
pub fn scan(host: &Host, udp: bool) -> Result<Snapshot, String> {
    let (success, stdout, stderr) = run(host, &script(udp))?;
    if !success && !stdout.contains(MARK_PS) {
        return Err(if stderr.is_empty() {
            format!("Listing ports on {} failed", host.name)
        } else {
            format!("{}: {stderr}", host.name)
        });
    }
    Ok(parse(&stdout))
}

/// `kill -<signal> <pid>` on `host`. Pids 0 and 1 are refused.
pub fn send(host: &Host, pid: u32, signal: Signal, owner: Option<&str>) -> Result<(), KillError> {
    if pid <= 1 {
        return Err(KillError::Refused(pid));
    }
    let command = format!("LC_ALL=C kill -{} {pid}", signal.name());
    let (success, _, stderr) = run(host, &command).map_err(|message| KillError::Failed {
        pid,
        signal: signal.name(),
        message,
    })?;
    kill::classify(pid, signal, owner, success, &stderr)
}

/// Whether `pid` still exists on `host` (also when ssh fails, so the UI
/// does not report a stop that did not happen).
#[must_use]
pub fn is_alive(host: &Host, pid: u32) -> bool {
    match run(host, &format!("kill -0 {pid} 2>&1")) {
        Ok((true, _, _)) => true,
        Ok((false, stdout, stderr)) => {
            let text = format!("{stdout}{stderr}").to_lowercase();
            !text.contains("no such process")
        }
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumterm_net::SavedHost;

    fn config_host(alias: &str) -> Host {
        Host {
            name: alias.into(),
            source: HostSource::SshConfig(ConfigHost {
                alias: alias.into(),
                ..ConfigHost::default()
            }),
        }
    }

    #[test]
    fn builds_a_non_interactive_ssh_command() {
        let argv = ssh_command(&config_host("prod"), "uptime").expect("argv");
        assert!(argv[0].ends_with("ssh"));
        assert_eq!(
            argv[1..6],
            ["-T", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10"]
        );
        assert_eq!(argv[argv.len() - 3..], ["--", "prod", "uptime"]);

        let saved = Host {
            name: "box".into(),
            source: HostSource::Saved(SavedHost {
                name: "box".into(),
                user: Some("me".into()),
                host: "10.0.0.5".into(),
                port: Some(2222),
                startup_command: Some("tmux attach".into()),
                ..SavedHost::default()
            }),
        };
        let argv = ssh_command(&saved, "uptime").expect("argv");
        assert!(!argv.contains(&"-t".to_string()));
        assert!(!argv.contains(&"tmux attach".to_string()));
        assert!(argv.windows(2).any(|w| w == ["-p", "2222"]));
        assert_eq!(argv[argv.len() - 3..], ["--", "me@10.0.0.5", "uptime"]);
        assert!(ssh_command(&config_host("-oProxyCommand=x"), "uptime").is_err());
    }

    #[test]
    fn script_tries_ss_then_lsof() {
        let tcp = script(false);
        assert!(tcp.contains("ss -ltnpH") && tcp.contains("lsof -nP"));
        assert!(!tcp.contains("-lunpH"));
        assert!(script(true).contains("ss -lunpH"));
    }

    #[test]
    fn parses_linux_output() {
        let text = format!(
            "{MARK_SS}\n\
             LISTEN 0 4096 127.0.0.1:5432 0.0.0.0:* users:((\"postgres\",pid=812,fd=6))\n\
             LISTEN 0 511 [::]:3000 [::]:* users:((\"node\",pid=4242,fd=21))\n\
             LISTEN 0 128 0.0.0.0:22 0.0.0.0:*\n\
             {MARK_PS}\n\
             \x20 812 postgres Mon Nov 16 09:00:00 2026 /usr/lib/postgresql/16/bin/postgres -D /var/lib/pg\n\
             4242 deploy   Tue Nov 17 10:00:00 2026 node /srv/app/server.js\n"
        );
        let snapshot = parse(&text);
        let ports: Vec<u16> = snapshot.listeners.iter().map(|l| l.port).collect();
        assert_eq!(ports, [22, 3000, 5432]);
        let node = &snapshot.listeners[1];
        assert_eq!(node.pid, Some(4242));
        assert_eq!(node.user.as_deref(), Some("deploy"));
        assert_eq!(node.command.as_deref(), Some("node /srv/app/server.js"));
        assert_eq!(node.project, None);
        assert_eq!(snapshot.listeners[0].process, "?");
        assert_eq!(snapshot.warnings.len(), 1);
    }

    #[test]
    fn parses_macos_output() {
        let text = format!(
            "{MARK_LSOF}\np1053\ncnode\nu501\nLme\nf21\ntIPv4\nPTCP\nn127.0.0.1:8080\nTST=LISTEN\n{MARK_PS}\n"
        );
        let snapshot = parse(&text);
        assert_eq!(snapshot.listeners.len(), 1);
        assert_eq!(snapshot.listeners[0].port, 8080);
        assert_eq!(snapshot.listeners[0].process, "node");
        assert!(snapshot.warnings.is_empty());
    }

    /// Runs the remote script with this machine's `sh` (what ssh would
    /// run on the host) and parses the result.
    #[test]
    fn script_runs_and_parses_on_this_machine() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let Ok(output) = Command::new("sh").arg("-c").arg(script(true)).output() else {
            return;
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        if output.status.code() == Some(3) {
            return; // neither ss nor lsof here
        }
        assert!(stdout.contains(MARK_PS), "{stdout}");
        let snapshot = parse(&stdout);
        // Sandboxed CI may hide sockets; when seen, it is ours, with ps data.
        if let Some(me) = snapshot.listeners.iter().find(|l| l.port == port) {
            assert_eq!(me.pid, Some(std::process::id()));
            assert!(me.command.is_some());
        }
    }

    #[test]
    fn keeps_added_hosts_in_their_own_file() {
        let path = store_path(Path::new("/home/me"));
        assert_eq!(path, Path::new("/home/me/.ferrum/ports/ssh-hosts.json"));
        let mut store = HostStore::default();
        let config = vec![ConfigHost {
            alias: "prod".into(),
            ..ConfigHost::default()
        }];
        assert!(hosts(&store, &config).is_empty());
        store.add_config_host("prod");
        let name = store.add_typed("me@10.0.0.9:2200").expect("host");
        let names: Vec<String> = hosts(&store, &config).into_iter().map(|h| h.name).collect();
        assert_eq!(names, [name, "prod".to_string()]);
    }
}
