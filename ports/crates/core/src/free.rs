//! "Is port N free?" and "find a free port near N".

use std::io::ErrorKind;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener};

use crate::model::{Listener, Protocol};

/// Whether a TCP server could listen on `port` right now: it binds on
/// `127.0.0.1`, `0.0.0.0` and `::` (when IPv6 exists) and lets go.
#[must_use]
pub fn is_port_free(port: u16) -> bool {
    if port == 0 {
        return false;
    }
    let v4 = [Ipv4Addr::LOCALHOST, Ipv4Addr::UNSPECIFIED]
        .into_iter()
        .all(|ip| TcpListener::bind(SocketAddr::from((ip, port))).is_ok());
    v4 && match TcpListener::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, port))) {
        Ok(_) => true,
        // No IPv6 on this machine is not "taken".
        Err(e) => e.kind() != ErrorKind::AddrInUse,
    }
}

/// The first free port from `port` upwards (at most `tries` ports), else
/// downwards; ports `taken` are skipped without binding.
#[must_use]
pub fn find_free_port_near(port: u16, tries: u16, taken: &[u16]) -> Option<u16> {
    let up = (port..=port.saturating_add(tries)).filter(|p| *p >= 1024);
    let down = (port.saturating_sub(tries).max(1024)..port).rev();
    up.chain(down)
        .find(|p| !taken.contains(p) && is_port_free(*p))
}

/// The answer to "is port N free?".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortCheck {
    pub port: u16,
    pub free: bool,
    /// Known listeners on the port (from the last scan).
    pub users: Vec<Listener>,
    /// Nearest free port when `port` is taken.
    pub suggestion: Option<u16>,
}

impl PortCheck {
    /// One-line summary, e.g. `Port 3000 is used by node (pid 42). 3001 is free.`
    #[must_use]
    pub fn message(&self) -> String {
        if self.free {
            return format!("Port {} is free.", self.port);
        }
        let by = match self.users.first() {
            Some(l) => match l.pid {
                Some(pid) => format!(" by {} (pid {pid})", l.process),
                None => format!(" by {}", l.process),
            },
            None => String::new(),
        };
        let next = self
            .suggestion
            .map_or_else(String::new, |p| format!(" {p} is free."));
        format!("Port {} is in use{by}.{next}", self.port)
    }
}

/// Checks `port` against the system and the last scan's `listeners`.
#[must_use]
pub fn check_port(port: u16, listeners: &[Listener]) -> PortCheck {
    let users: Vec<Listener> = listeners
        .iter()
        .filter(|l| l.port == port && l.protocol == Protocol::Tcp)
        .cloned()
        .collect();
    let free = users.is_empty() && is_port_free(port);
    let taken: Vec<u16> = listeners.iter().map(|l| l.port).collect();
    let suggestion = (!free)
        .then(|| find_free_port_near(port, 100, &taken))
        .flatten();
    PortCheck {
        port,
        free,
        users,
        suggestion,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_a_taken_port() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        assert!(!is_port_free(port));
        assert!(!is_port_free(0));
        let next = find_free_port_near(port, 50, &[]).expect("free port");
        assert_ne!(next, port);
        let check = check_port(port, &[]);
        assert!(!check.free);
        assert!(check.suggestion.is_some());
        assert!(
            check
                .message()
                .starts_with(&format!("Port {port} is in use."))
        );
        drop(listener);
    }

    #[test]
    fn skips_ports_known_to_be_taken() {
        let start = find_free_port_near(40_000, 200, &[]).expect("free");
        let next = find_free_port_near(start, 200, &[start]).expect("next");
        assert_ne!(next, start);
    }

    #[test]
    fn messages() {
        let check = PortCheck {
            port: 3000,
            free: false,
            users: vec![Listener {
                protocol: Protocol::Tcp,
                port: 3000,
                addresses: vec!["::".into()],
                pid: Some(42),
                process: "node".into(),
                user: None,
                command: None,
                started: None,
                cwd: None,
                project: None,
            }],
            suggestion: Some(3001),
        };
        assert_eq!(
            check.message(),
            "Port 3000 is in use by node (pid 42). 3001 is free."
        );
        let free = PortCheck {
            free: true,
            users: Vec::new(),
            suggestion: None,
            ..check
        };
        assert_eq!(free.message(), "Port 3000 is free.");
    }
}
