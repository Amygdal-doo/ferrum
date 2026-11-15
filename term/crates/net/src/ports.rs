//! "What's using port N?": listening TCP sockets and the processes behind
//! them, from `lsof -nP -iTCP -sTCP:LISTEN` (macOS) or `ss -ltnp` (Linux,
//! falling back to lsof), with user and command line from `ps`.
//!
//! Without root, macOS `lsof` only shows your own processes; a port that is
//! taken by someone else's shows up as [`PortReport::in_use_by_other`].

use serde::Serialize;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener};
use std::process::{Command, Stdio};

/// A process listening on a TCP port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Listener {
    pub port: u16,
    /// `127.0.0.1`, `*`, `[::1]`…
    pub address: String,
    pub pid: Option<u32>,
    /// Short process name (`node`, `postgres`).
    pub process: String,
    pub user: Option<String>,
    /// Full command line, from `ps`.
    pub command: Option<String>,
}

/// Result of a lookup for one port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortReport {
    pub port: u16,
    pub listeners: Vec<Listener>,
    /// The port can't be bound but no visible process holds it (another
    /// user's process: try `sudo lsof -nP -iTCP:<port>`).
    pub in_use_by_other: bool,
}

/// Decodes lsof's `\xNN` escapes (`Google\x20Chrome` → `Google Chrome`).
fn unescape(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && b.get(i + 1) == Some(&b'x')
            && let Some(hex) = s.get(i + 2..i + 4)
            && let Ok(v) = u8::from_str_radix(hex, 16)
        {
            out.push(v);
            i += 4;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `127.0.0.1:5432` → (`127.0.0.1`, 5432); `[::1]:8080`, `*:80`, `*.80`.
fn split_addr(s: &str) -> Option<(String, u16)> {
    let i = s.rfind([':', '.'])?;
    let port = s[i + 1..].parse().ok()?;
    Some((s[..i].to_owned(), port))
}

fn dedupe(mut v: Vec<Listener>) -> Vec<Listener> {
    let mut seen = std::collections::HashSet::new();
    v.retain(|l| seen.insert((l.pid, l.address.clone(), l.port)));
    v.sort_by(|a, b| (a.port, a.pid, &a.address).cmp(&(b.port, b.pid, &b.address)));
    v
}

/// Parses `lsof [+c 0] -nP -iTCP[:N] -sTCP:LISTEN` table output.
#[must_use]
pub fn parse_lsof(text: &str) -> Vec<Listener> {
    let mut out = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.first() == Some(&"COMMAND") || cols.len() < 9 {
            continue;
        }
        // COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME [(STATE)]
        let (name, state) = match cols.last() {
            Some(s) if s.starts_with('(') => (cols[cols.len() - 2], Some(*s)),
            Some(s) => (*s, None),
            None => continue,
        };
        if state.is_some_and(|s| s != "(LISTEN)") || name.contains("->") {
            continue;
        }
        let Some((address, port)) = split_addr(name) else {
            continue;
        };
        let pid = cols[1].parse().ok();
        let user = cols[2];
        out.push(Listener {
            port,
            address,
            pid,
            process: unescape(cols[0]),
            // With -n, unknown users print as a uid.
            user: (!user.chars().all(|c| c.is_ascii_digit())).then(|| user.to_owned()),
            command: None,
        });
    }
    dedupe(out)
}

/// Parses `ss -ltnp` (with or without the header line).
#[must_use]
pub fn parse_ss(text: &str) -> Vec<Listener> {
    let mut out = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 || cols[0] != "LISTEN" {
            continue;
        }
        let Some((address, port)) = split_addr(cols[3]) else {
            continue;
        };
        // Drop `%lo` interface suffixes: `127.0.0.53%lo`.
        let address = address
            .split('%')
            .next()
            .unwrap_or_default()
            .to_owned()
            .replace("0.0.0.0", "*")
            .replace("[::]", "*");
        let rest = cols[5..].join(" ");
        let mut procs = Vec::new();
        // users:(("sshd",pid=812,fd=3),("sshd",pid=900,fd=3))
        for chunk in rest.split("((").skip(1).flat_map(|s| s.split("),(")) {
            let name = chunk.split('"').nth(1).unwrap_or_default().to_owned();
            let pid = chunk
                .split(',')
                .find_map(|kv| kv.strip_prefix("pid="))
                .and_then(|p| p.trim_end_matches(')').parse().ok());
            procs.push((name, pid));
        }
        if procs.is_empty() {
            procs.push((String::new(), None));
        }
        for (process, pid) in procs {
            out.push(Listener {
                port,
                address: address.clone(),
                pid,
                process,
                user: None,
                command: None,
            });
        }
    }
    dedupe(out)
}

/// Parses `ps -o pid=,user=,command= -p …` → pid → (user, command line).
#[must_use]
pub fn parse_ps(text: &str) -> HashMap<u32, (String, String)> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let (Some(pid), Some(user)) = (it.next().and_then(|p| p.parse().ok()), it.next()) else {
            continue;
        };
        out.insert(pid, (user.to_owned(), it.collect::<Vec<_>>().join(" ")));
    }
    out
}

/// Runs a command; `Ok(None)` when the program isn't installed.
fn run(program: &str, args: &[&str]) -> Result<Option<String>, String> {
    match Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        Ok(o) => Ok(Some(String::from_utf8_lossy(&o.stdout).into_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("could not run {program}: {e}")),
    }
}

fn lsof_path() -> &'static str {
    if std::path::Path::new("/usr/sbin/lsof").exists() {
        "/usr/sbin/lsof"
    } else {
        "lsof"
    }
}

fn via_lsof(port: Option<u16>) -> Result<Option<Vec<Listener>>, String> {
    let filter = port.map_or_else(|| "-iTCP".to_owned(), |p| format!("-iTCP:{p}"));
    // lsof exits 1 when nothing matches; the (empty) output is still right.
    Ok(run(lsof_path(), &["+c", "0", "-nP", &filter, "-sTCP:LISTEN"])?.map(|t| parse_lsof(&t)))
}

fn via_ss(port: Option<u16>) -> Result<Option<Vec<Listener>>, String> {
    let Some(text) = run("ss", &["-ltnpH"])? else {
        return Ok(None);
    };
    let mut v = parse_ss(&text);
    if let Some(p) = port {
        v.retain(|l| l.port == p);
    }
    Ok(Some(v))
}

/// Fills user and command line from `ps`.
fn enrich(listeners: &mut [Listener]) {
    let pids: Vec<String> = listeners
        .iter()
        .filter_map(|l| l.pid)
        .map(|p| p.to_string())
        .collect();
    if pids.is_empty() {
        return;
    }
    let Ok(Some(text)) = run("ps", &["-o", "pid=,user=,command=", "-p", &pids.join(",")]) else {
        return;
    };
    let info = parse_ps(&text);
    for l in listeners {
        if let Some((user, cmd)) = l.pid.and_then(|p| info.get(&p)) {
            l.user = Some(user.clone());
            l.command = Some(cmd.clone()).filter(|c| !c.is_empty());
            if l.process.is_empty() {
                l.process = cmd
                    .split_whitespace()
                    .next()
                    .and_then(|p| p.rsplit('/').next())
                    .unwrap_or_default()
                    .to_owned();
            }
        }
    }
}

/// Listening TCP sockets (all, or on `port`).
pub fn listeners(port: Option<u16>) -> Result<Vec<Listener>, String> {
    let found = if cfg!(target_os = "linux") {
        match via_ss(port)? {
            // ss shows no owner for sockets it can't attribute; try lsof for those.
            Some(v) if v.iter().all(|l| l.pid.is_some()) => Some(v),
            Some(v) => via_lsof(port)?.filter(|l| !l.is_empty()).or(Some(v)),
            None => via_lsof(port)?,
        }
    } else {
        via_lsof(port)?
    };
    let mut v = found.ok_or("neither lsof nor ss is installed")?;
    enrich(&mut v);
    Ok(v)
}

/// Whether binding `port` fails because something already listens there.
#[must_use]
pub fn port_in_use(port: u16, bind: Option<&str>) -> bool {
    let mut addrs: Vec<IpAddr> = vec![
        Ipv4Addr::LOCALHOST.into(),
        Ipv4Addr::UNSPECIFIED.into(),
        Ipv6Addr::LOCALHOST.into(),
    ];
    if let Some(ip) = bind.and_then(|b| b.parse::<IpAddr>().ok()) {
        addrs.insert(0, ip);
    }
    addrs.into_iter().any(|ip| {
        matches!(
            TcpListener::bind(SocketAddr::new(ip, port)),
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse
        )
    })
}

/// Who listens on `port`.
pub fn lookup(port: u16) -> Result<PortReport, String> {
    let listeners = listeners(Some(port))?;
    let in_use_by_other = listeners.is_empty() && port_in_use(port, None);
    Ok(PortReport {
        port,
        listeners,
        in_use_by_other,
    })
}

/// `node (pid 123)` / `pid 123` / `another user's process`.
#[must_use]
pub fn describe_owner(report: &PortReport) -> String {
    match report.listeners.first() {
        Some(l) => match (l.process.is_empty(), l.pid) {
            (false, Some(pid)) => format!("{} (pid {pid})", l.process),
            (true, Some(pid)) => format!("pid {pid}"),
            (false, None) => l.process.clone(),
            (true, None) => "an unknown process".into(),
        },
        None => "another user's process".into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Term,
    Kill,
}

/// Sends SIGTERM / SIGKILL with `/bin/kill`. Refuses pid 0/1 and FerrumTerm itself.
pub fn signal(pid: u32, sig: Signal) -> Result<(), String> {
    if pid <= 1 || pid == std::process::id() {
        return Err(format!("refusing to signal pid {pid}"));
    }
    let name = match sig {
        Signal::Term => "TERM",
        Signal::Kill => "KILL",
    };
    let out = Command::new("/bin/kill")
        .args(["-s", name, &pid.to_string()])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run kill: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let message = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    Err(if message.is_empty() {
        format!("kill {pid} failed")
    } else {
        message
    })
}

/// Whether a process exists (any owner).
#[must_use]
pub fn is_alive(pid: u32) -> bool {
    Command::new("ps")
        .args(["-o", "pid=", "-p", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LSOF: &str = "\
COMMAND                   PID           USER   FD   TYPE             DEVICE SIZE/OFF NODE NAME
rapportd                  626 me               10u  IPv4 0xe3d7ee0f65b71f71      0t0  TCP *:56996 (LISTEN)
rapportd                  626 me               11u  IPv6 0xc9d07487146f8bf2      0t0  TCP *:56996 (LISTEN)
adb                      1053 me                8u  IPv4 0xbda726d263894082      0t0  TCP 127.0.0.1:5037 (LISTEN)
node                     8077 me               42u  IPv6 0xbd8456e937d6e004      0t0  TCP [::1]:5310 (LISTEN)
Google\\x20Chrome\\x20Helper 21135 501   23u  IPv4 0x3c51      0t0  TCP 127.0.0.1:9222 (LISTEN)
curl                     4242 me   5u  IPv4 0x1      0t0  TCP 10.0.0.2:51000->1.1.1.1:443 (ESTABLISHED)
";

    #[test]
    fn parses_lsof_fixture() {
        let v = parse_lsof(LSOF);
        let summary: Vec<_> = v
            .iter()
            .map(|l| (l.port, l.address.as_str(), l.pid, l.process.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                (5037, "127.0.0.1", Some(1053), "adb"),
                (5310, "[::1]", Some(8077), "node"),
                (9222, "127.0.0.1", Some(21135), "Google Chrome Helper"),
                (56996, "*", Some(626), "rapportd"),
            ],
            "IPv4/IPv6 wildcard duplicates merge; connections are skipped"
        );
        assert_eq!(v[0].user.as_deref(), Some("me"));
        assert_eq!(v[2].user, None, "numeric uid is not a name");
        assert!(parse_lsof("").is_empty());
        assert!(parse_lsof("garbage line\n").is_empty());
    }

    const SS: &str = r#"State  Recv-Q Send-Q Local Address:Port  Peer Address:Port Process
LISTEN 0      4096   127.0.0.53%lo:53         0.0.0.0:*     users:(("systemd-resolve",pid=812,fd=14))
LISTEN 0      128          0.0.0.0:22         0.0.0.0:*     users:(("sshd",pid=1001,fd=3),("sshd",pid=1002,fd=3))
LISTEN 0      511             [::]:80            [::]:*
LISTEN 0      244        127.0.0.1:5432       0.0.0.0:*     users:(("postgres",pid=77,fd=6))
"#;

    #[test]
    fn parses_ss_fixture() {
        let v = parse_ss(SS);
        let summary: Vec<_> = v
            .iter()
            .map(|l| (l.port, l.address.as_str(), l.pid, l.process.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                (22, "*", Some(1001), "sshd"),
                (22, "*", Some(1002), "sshd"),
                (53, "127.0.0.53", Some(812), "systemd-resolve"),
                (80, "*", None, ""),
                (5432, "127.0.0.1", Some(77), "postgres"),
            ]
        );
    }

    #[test]
    fn parses_ps_and_helpers() {
        let ps = parse_ps(
            "  626 me /usr/libexec/rapportd\n 1053 root adb -L tcp:5037 fork-server\nbad\n",
        );
        assert_eq!(ps[&626], ("me".into(), "/usr/libexec/rapportd".into()));
        assert_eq!(ps[&1053].1, "adb -L tcp:5037 fork-server");
        assert_eq!(unescape(r"a\x20b\x2"), "a b\\x2");
        assert_eq!(split_addr("*.8080"), Some(("*".into(), 8080)));
        let report = PortReport {
            port: 1,
            listeners: Vec::new(),
            in_use_by_other: true,
        };
        assert_eq!(describe_owner(&report), "another user's process");
        assert!(signal(1, Signal::Term).is_err());
        assert!(signal(std::process::id(), Signal::Kill).is_err());
    }

    #[test]
    fn finds_our_own_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        assert!(port_in_use(port, Some("127.0.0.1")));
        let report = lookup(port).expect("lookup");
        let me = report
            .listeners
            .iter()
            .find(|l| l.pid == Some(std::process::id()))
            .unwrap_or_else(|| panic!("our listener on {port} not found: {report:?}"));
        assert_eq!(me.port, port);
        assert!(me.command.is_some() && me.user.is_some());
        assert!(is_alive(std::process::id()));
        drop(listener);
        assert!(!port_in_use(port, None));
    }
}
