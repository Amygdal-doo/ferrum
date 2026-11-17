//! Data types shared by the parsers, the scanner and the UI.

use std::net::IpAddr;
use std::path::PathBuf;

use chrono::NaiveDateTime;

use crate::container::Container;
use crate::hints::{self, Hint};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Protocol {
    Tcp,
    Udp,
}

impl Protocol {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Protocol::Tcp => "TCP",
            Protocol::Udp => "UDP",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    V4,
    V6,
}

/// One bound socket as `lsof` / `ss` report it, before process details
/// are joined in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Socket {
    pub protocol: Protocol,
    /// Normalized: `0.0.0.0`, `::`, `127.0.0.1`, `::1`, …
    pub address: String,
    pub port: u16,
    /// `None` when the tool may not see the owner (another user's process).
    pub pid: Option<u32>,
    pub process: String,
    pub user: Option<String>,
}

/// How reachable a listener is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Exposure {
    /// Only this machine (`127.0.0.1`, `::1`).
    Loopback,
    /// One interface address, e.g. `192.168.1.20`.
    Specific,
    /// Every interface (`0.0.0.0`, `::`): reachable from the network.
    AllInterfaces,
}

impl Exposure {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Exposure::Loopback => "local",
            Exposure::Specific => "interface",
            Exposure::AllInterfaces => "all interfaces",
        }
    }
}

/// `host` as a plain address: brackets and `%iface` dropped, `*` written
/// as the wildcard of its family.
#[must_use]
pub fn normalize_address(host: &str, family: Option<Family>) -> String {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let host = host.split('%').next().unwrap_or(host);
    match (host, family) {
        ("*", Some(Family::V6)) => "::".to_string(),
        ("*", _) => "0.0.0.0".to_string(),
        _ => host.to_string(),
    }
}

/// Splits `host:port` at the last colon (`[::1]:8080` works too). `None`
/// for a wildcard or bad port.
#[must_use]
pub fn split_host_port(text: &str) -> Option<(&str, u16)> {
    let (host, port) = text.rsplit_once(':')?;
    Some((host, port.parse().ok()?))
}

#[must_use]
pub fn exposure_of(address: &str) -> Exposure {
    match address {
        "0.0.0.0" | "::" => Exposure::AllInterfaces,
        "localhost" => Exposure::Loopback,
        _ => match address.parse::<IpAddr>() {
            Ok(ip) if ip.is_loopback() => Exposure::Loopback,
            Ok(IpAddr::V6(v6)) if v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback()) => {
                Exposure::Loopback
            }
            _ => Exposure::Specific,
        },
    }
}

/// Identifies a listener across refreshes (the selection survives them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListenerKey {
    pub protocol: Protocol,
    pub port: u16,
    pub pid: Option<u32>,
}

/// A listening port and the process behind it. IPv4 and IPv6 sockets of
/// the same process and port are merged into one listener.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    pub protocol: Protocol,
    pub port: u16,
    pub addresses: Vec<String>,
    pub pid: Option<u32>,
    /// Short process name (`node`, `postgres`).
    pub process: String,
    pub user: Option<String>,
    /// Full command line, when `ps` could read it.
    pub command: Option<String>,
    /// Process start, local time.
    pub started: Option<NaiveDateTime>,
    pub cwd: Option<PathBuf>,
    /// The project folder `cwd` belongs to (see [`crate::project`]).
    pub project: Option<PathBuf>,
    /// The container that published the port, when a container runtime
    /// holds it (see [`crate::container`]).
    pub container: Option<Container>,
}

impl Listener {
    #[must_use]
    pub fn key(&self) -> ListenerKey {
        ListenerKey {
            protocol: self.protocol,
            port: self.port,
            pid: self.pid,
        }
    }

    /// The widest exposure of its addresses.
    #[must_use]
    pub fn exposure(&self) -> Exposure {
        self.addresses
            .iter()
            .map(|a| exposure_of(a))
            .max()
            .unwrap_or(Exposure::Specific)
    }

    #[must_use]
    pub fn address_label(&self) -> String {
        self.addresses.join(", ")
    }

    /// `http://localhost:<port>`, or the interface address when it only
    /// listens on one.
    #[must_use]
    pub fn url(&self) -> String {
        match self.addresses.first() {
            Some(a) if self.exposure() == Exposure::Specific && a.contains(':') => {
                format!("http://[{a}]:{}", self.port)
            }
            Some(a) if self.exposure() == Exposure::Specific => {
                format!("http://{a}:{}", self.port)
            }
            _ => format!("http://localhost:{}", self.port),
        }
    }

    #[must_use]
    pub fn hint(&self) -> Option<&'static Hint> {
        hints::hint(self.port)
    }

    /// The project's folder name, e.g. `ferrum`.
    #[must_use]
    pub fn project_name(&self) -> Option<String> {
        let project = self.project.as_ref()?;
        Some(project.file_name().map_or_else(
            || project.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        ))
    }

    /// The command line, else the process name.
    #[must_use]
    pub fn command_or_name(&self) -> &str {
        self.command.as_deref().unwrap_or(&self.process)
    }

    /// `3000 web`: the port and what it probably is, for group summaries.
    #[must_use]
    pub fn short_label(&self) -> String {
        let what = self.hint().map_or(self.process.as_str(), |h| h.short);
        format!("{} {what}", self.port)
    }
}

/// The result of one scan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// Sorted by port, protocol, pid.
    pub listeners: Vec<Listener>,
    /// Non-fatal problems, e.g. command lines that could not be read.
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_normalized() {
        assert_eq!(normalize_address("*", Some(Family::V4)), "0.0.0.0");
        assert_eq!(normalize_address("*", Some(Family::V6)), "::");
        assert_eq!(normalize_address("[::1]", None), "::1");
        assert_eq!(normalize_address("127.0.0.53%lo", None), "127.0.0.53");
        assert_eq!(split_host_port("[::1]:8080"), Some(("[::1]", 8080)));
        assert_eq!(split_host_port("*:*"), None);
        assert_eq!(split_host_port("127.0.0.1:99999"), None);
    }

    #[test]
    fn exposure_and_urls() {
        assert_eq!(exposure_of("127.0.0.1"), Exposure::Loopback);
        assert_eq!(exposure_of("::1"), Exposure::Loopback);
        assert_eq!(exposure_of("::ffff:127.0.0.1"), Exposure::Loopback);
        assert_eq!(exposure_of("::"), Exposure::AllInterfaces);
        assert_eq!(exposure_of("192.168.1.2"), Exposure::Specific);
        let mut l = Listener {
            protocol: Protocol::Tcp,
            port: 3000,
            addresses: vec!["127.0.0.1".into(), "0.0.0.0".into()],
            pid: Some(1),
            process: "node".into(),
            user: None,
            command: None,
            started: None,
            cwd: None,
            project: Some(PathBuf::from("/code/shop")),
            container: None,
        };
        assert_eq!(l.exposure(), Exposure::AllInterfaces);
        assert_eq!(l.url(), "http://localhost:3000");
        assert_eq!(l.short_label(), "3000 web");
        assert_eq!(l.project_name().as_deref(), Some("shop"));
        assert_eq!(l.command_or_name(), "node");
        l.addresses = vec!["fe80::1".into()];
        assert_eq!(l.url(), "http://[fe80::1]:3000");
        l.addresses = vec!["10.0.0.5".into()];
        l.port = 4444;
        assert_eq!(l.url(), "http://10.0.0.5:4444");
        assert_eq!(l.short_label(), "4444 node");
    }
}
