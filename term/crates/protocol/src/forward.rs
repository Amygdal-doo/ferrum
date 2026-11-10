//! SSH port forwards: `-L` local, `-R` remote, `-D` dynamic (SOCKS).
//!
//! Specs use ssh's own syntax so they can be copied from a command line:
//! `L 8080:localhost:80`, `-L 127.0.0.1:5432:db:5432`, `R 9000:localhost:3000`,
//! `D 1080`. The server runs each one as an `ssh -N` child process.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForwardKind {
    /// `-L [bind:]port:host:hostport`: a local port reaches `host:hostport` via the server.
    Local,
    /// `-R [bind:]port:host:hostport`: a port on the server reaches `host:hostport` here.
    Remote,
    /// `-D [bind:]port`: a local SOCKS proxy.
    Dynamic,
}

impl ForwardKind {
    #[must_use]
    pub fn flag(self) -> &'static str {
        match self {
            Self::Local => "-L",
            Self::Remote => "-R",
            Self::Dynamic => "-D",
        }
    }

    #[must_use]
    pub fn letter(self) -> char {
        match self {
            Self::Local => 'L',
            Self::Remote => 'R',
            Self::Dynamic => 'D',
        }
    }
}

/// One forward definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ForwardSpec {
    pub kind: ForwardKind,
    /// Listen address (`127.0.0.1`, `0.0.0.0`…); `None` = ssh's default (loopback).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind: Option<String>,
    /// Listening port: local for `-L`/`-D`, on the remote host for `-R`.
    pub port: u16,
    /// Destination host for `-L`/`-R`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_port: Option<u16>,
}

fn port(s: &str, what: &str) -> Result<u16, String> {
    match s.trim().parse::<u16>() {
        Ok(p) if p > 0 => Ok(p),
        _ => Err(format!("invalid {what} “{s}” (1–65535)")),
    }
}

/// Splits `a:b:c` on colons outside `[…]` (IPv6 addresses).
fn split_colons(s: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut depth = 0u32;
    for c in s.chars() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            ':' if depth == 0 => {
                parts.push(String::new());
                continue;
            }
            _ => {}
        }
        if let Some(last) = parts.last_mut() {
            last.push(c);
        }
    }
    parts
}

fn unbracket(s: &str) -> String {
    s.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_owned()
}

fn bracket(s: &str) -> String {
    if s.contains(':') {
        format!("[{s}]")
    } else {
        s.to_owned()
    }
}

fn check_host(h: &str) -> Result<(), String> {
    if h.is_empty() {
        return Err("empty host in forward".into());
    }
    if h.starts_with('-') || h.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!("invalid host “{h}” in forward"));
    }
    Ok(())
}

impl ForwardSpec {
    /// Parses `L 8080:localhost:80`, `-L8080:localhost:80`, `D 1080`,
    /// `R 0.0.0.0:9000:localhost:3000`. A bare `8080:host:80` means `-L`.
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        let rest = s.strip_prefix('-').unwrap_or(s);
        let (kind, body) = match rest.chars().next() {
            Some('L' | 'l') => (ForwardKind::Local, &rest[1..]),
            Some('R' | 'r') => (ForwardKind::Remote, &rest[1..]),
            Some('D' | 'd') => (ForwardKind::Dynamic, &rest[1..]),
            Some(c) if c.is_ascii_digit() || c == '[' => (ForwardKind::Local, rest),
            _ => {
                return Err(format!(
                    "invalid forward “{s}”: use L port:host:port, R port:host:port or D port"
                ));
            }
        };
        let body = body.trim();
        let parts = split_colons(body);
        let spec = match kind {
            ForwardKind::Dynamic => match parts.as_slice() {
                [p] => Self {
                    kind,
                    bind: None,
                    port: port(p, "port")?,
                    target_host: None,
                    target_port: None,
                },
                [b, p] => Self {
                    kind,
                    bind: Some(unbracket(b)),
                    port: port(p, "port")?,
                    target_host: None,
                    target_port: None,
                },
                _ => return Err(format!("invalid dynamic forward “{s}”: use D [bind:]port")),
            },
            ForwardKind::Local | ForwardKind::Remote => {
                let (bind, p, h, hp) = match parts.as_slice() {
                    [p, h, hp] => (None, p, h, hp),
                    [b, p, h, hp] => (Some(unbracket(b)), p, h, hp),
                    _ => {
                        return Err(format!(
                            "invalid forward “{s}”: use {} [bind:]port:host:hostport",
                            kind.letter()
                        ));
                    }
                };
                let host = unbracket(h);
                check_host(&host)?;
                Self {
                    kind,
                    bind,
                    port: port(p, "port")?,
                    target_host: Some(host),
                    target_port: Some(port(hp, "target port")?),
                }
            }
        };
        if let Some(b) = &spec.bind {
            check_host(b)?;
        }
        Ok(spec)
    }

    /// Parses a list separated by commas or newlines; empty input = no forwards.
    pub fn parse_list(s: &str) -> Result<Vec<Self>, String> {
        s.split([',', '\n', ';'])
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(Self::parse)
            .collect()
    }

    /// The ssh argument after the flag: `8080:localhost:80`.
    #[must_use]
    pub fn ssh_value(&self) -> String {
        let mut v = String::new();
        if let Some(b) = &self.bind {
            v.push_str(&bracket(b));
            v.push(':');
        }
        v.push_str(&self.port.to_string());
        if let (Some(h), Some(p)) = (&self.target_host, self.target_port) {
            v.push_str(&format!(":{}:{p}", bracket(h)));
        }
        v
    }

    /// Short stable id within a host: `L8080`, `R9000`, `D1080`.
    #[must_use]
    pub fn id(&self) -> String {
        format!("{}{}", self.kind.letter(), self.port)
    }

    /// The port that must be free on this machine before starting (`-L`, `-D`).
    #[must_use]
    pub fn local_port(&self) -> Option<u16> {
        (self.kind != ForwardKind::Remote).then_some(self.port)
    }

    /// Human description: `localhost:8080 → db:5432`, `SOCKS on localhost:1080`.
    #[must_use]
    pub fn describe(&self) -> String {
        let bind = self.bind.as_deref().unwrap_or("localhost");
        let target = match (&self.target_host, self.target_port) {
            (Some(h), Some(p)) => format!("{h}:{p}"),
            _ => String::new(),
        };
        match self.kind {
            ForwardKind::Local => format!("{bind}:{} → {target}", self.port),
            ForwardKind::Remote => format!("remote {bind}:{} → {target}", self.port),
            ForwardKind::Dynamic => format!("SOCKS on {bind}:{}", self.port),
        }
    }
}

impl fmt::Display for ForwardSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.kind.letter(), self.ssh_value())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForwardState {
    Running,
    /// Did not start or exited; see `message`.
    Failed,
}

/// A forward the server knows about (running, or failed and not yet cleared).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForwardInfo {
    /// `host/L8080`.
    pub key: String,
    pub host: String,
    pub spec: ForwardSpec,
    pub state: ForwardState,
    pub pid: Option<u32>,
    pub started_ms: u64,
    pub message: Option<String>,
    pub command: Vec<String>,
}

/// `host/L8080`: how forwards are named on the server and in the CLI.
#[must_use]
pub fn forward_key(host: &str, spec: &ForwardSpec) -> String {
    format!("{host}/{}", spec.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_style_specs() {
        let l = ForwardSpec::parse("-L 8080:localhost:80").expect("L");
        assert_eq!(l.kind, ForwardKind::Local);
        assert_eq!((l.port, l.target_port), (8080, Some(80)));
        assert_eq!(l.target_host.as_deref(), Some("localhost"));
        assert_eq!(l.ssh_value(), "8080:localhost:80");
        assert_eq!(l.to_string(), "L 8080:localhost:80");
        assert_eq!(l.id(), "L8080");
        assert_eq!(l.local_port(), Some(8080));

        let b = ForwardSpec::parse("L127.0.0.1:5432:db.internal:5432").expect("bind");
        assert_eq!(b.bind.as_deref(), Some("127.0.0.1"));
        assert_eq!(b.ssh_value(), "127.0.0.1:5432:db.internal:5432");

        let r = ForwardSpec::parse("R 9000:localhost:3000").expect("R");
        assert_eq!(r.kind, ForwardKind::Remote);
        assert_eq!(r.local_port(), None);

        let d = ForwardSpec::parse("d 1080").expect("D");
        assert_eq!(
            (d.kind, d.port, d.target_host.clone()),
            (ForwardKind::Dynamic, 1080, None)
        );
        assert_eq!(d.to_string(), "D 1080");
        assert_eq!(d.describe(), "SOCKS on localhost:1080");

        let bare = ForwardSpec::parse("3000:localhost:3000").expect("bare");
        assert_eq!(bare.kind, ForwardKind::Local);

        let v6 = ForwardSpec::parse("L [::1]:8080:[fe80::1]:80").expect("v6");
        assert_eq!(v6.bind.as_deref(), Some("::1"));
        assert_eq!(v6.target_host.as_deref(), Some("fe80::1"));
        assert_eq!(v6.ssh_value(), "[::1]:8080:[fe80::1]:80");
        assert_eq!(ForwardSpec::parse(&v6.to_string()).expect("again"), v6);
    }

    #[test]
    fn rejects_bad_specs() {
        for bad in [
            "",
            "X 1:a:2",
            "L 8080",
            "L 0:localhost:80",
            "L 70000:localhost:80",
            "L 8080:-oProxyCommand=x:80",
            "D a:b:c",
            "L 8080:local host:80",
        ] {
            assert!(ForwardSpec::parse(bad).is_err(), "{bad} should fail");
        }
    }

    #[test]
    fn parses_lists_and_round_trips_json() {
        let list = ForwardSpec::parse_list("L 8080:localhost:80, D 1080\nR 9000:localhost:3000")
            .expect("list");
        assert_eq!(list.len(), 3);
        assert!(ForwardSpec::parse_list("  ").expect("empty").is_empty());
        assert!(ForwardSpec::parse_list("L 1:a:2, nope").is_err());
        let json = serde_json::to_string(&list).expect("json");
        assert_eq!(
            serde_json::from_str::<Vec<ForwardSpec>>(&json).expect("parse"),
            list
        );
        assert_eq!(forward_key("prod", &list[1]), "prod/D1080");
    }
}
