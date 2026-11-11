//! Parser for Linux `ss -ltnpH` / `ss -lunpH` output:
//!
//! `LISTEN 0 4096 127.0.0.1:5432 0.0.0.0:* users:(("postgres",pid=812,fd=6))`
//!
//! Without permission to see the owner, the `users:` column is missing.

use crate::model::{Family, Protocol, Socket, normalize_address, split_host_port};

pub const TCP_ARGS: &[&str] = &["-ltnpH"];
pub const UDP_ARGS: &[&str] = &["-lunpH"];

#[must_use]
pub fn parse(text: &str, protocol: Protocol) -> Vec<Socket> {
    text.lines()
        .filter_map(|line| parse_line(line, protocol))
        .collect()
}

fn parse_line(line: &str, protocol: Protocol) -> Option<Socket> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 5 || fields[0] == "State" || fields[0] == "Netid" {
        return None;
    }
    let (host, port) = split_host_port(fields[3])?;
    let family = host.contains(':').then_some(Family::V6);
    let (process, pid) = fields
        .get(5..)
        .map(|rest| users(&rest.join(" ")))
        .unwrap_or_default();
    Some(Socket {
        protocol,
        address: normalize_address(host, family),
        port,
        pid,
        process: process.unwrap_or_default(),
        user: None,
    })
}

/// The first process of `users:(("name",pid=1,fd=2),…)`.
fn users(text: &str) -> (Option<String>, Option<u32>) {
    let name = text
        .split_once("((\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(name, _)| name.to_string());
    let pid = text.split_once("pid=").and_then(|(_, rest)| {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    });
    (name, pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TCP: &str = "LISTEN 0      4096   127.0.0.53%lo:53        0.0.0.0:*    users:((\"systemd-resolve\",pid=651,fd=14))\nLISTEN 0      511          [::]:3000           [::]:*    users:((\"node\",pid=4242,fd=21),(\"node\",pid=4243,fd=21))\nLISTEN 0      128             *:22               *:*\n";

    #[test]
    fn parses_ss_output() {
        let sockets = parse(TCP, Protocol::Tcp);
        assert_eq!(sockets.len(), 3);
        assert_eq!(sockets[0].address, "127.0.0.53");
        assert_eq!(sockets[0].port, 53);
        assert_eq!(sockets[0].process, "systemd-resolve");
        assert_eq!(sockets[0].pid, Some(651));
        assert_eq!(sockets[1].address, "::");
        assert_eq!(sockets[1].pid, Some(4242));
        // Owner not visible.
        assert_eq!(sockets[2].pid, None);
        assert_eq!(sockets[2].address, "0.0.0.0");
    }

    #[test]
    fn parses_udp_and_skips_headers() {
        let text = "State Recv-Q Send-Q Local Address:Port Peer Address:Port Process\nUNCONN 0 0 0.0.0.0:5353 0.0.0.0:* users:((\"avahi-daemon\",pid=700,fd=12))\n";
        let sockets = parse(text, Protocol::Udp);
        assert_eq!(sockets.len(), 1);
        assert_eq!(sockets[0].protocol, Protocol::Udp);
        assert_eq!(sockets[0].port, 5353);
    }
}
