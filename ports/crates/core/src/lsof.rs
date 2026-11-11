//! Parsers for `lsof -F` machine output (macOS, BSD, Linux fallback).
//!
//! Each line is one field: a letter, then the value. `p` starts a process
//! (`c` command, `u` uid, `L` login), `f` starts one of its files (`t`
//! IPv4/IPv6, `P` TCP/UDP, `n` address, `T` TCP info such as `ST=LISTEN`).

use std::collections::HashMap;
use std::path::PathBuf;

use crate::model::{Family, Protocol, Socket, normalize_address, split_host_port};

/// Arguments for listening TCP sockets.
pub const TCP_ARGS: &[&str] = &[
    "-nP",
    "-w",
    "+c",
    "0",
    "-iTCP",
    "-sTCP:LISTEN",
    "-F",
    "pcuLftPnT",
];
/// Arguments for UDP sockets (bound ones are kept, connected ones dropped).
pub const UDP_ARGS: &[&str] = &["-nP", "-w", "+c", "0", "-iUDP", "-F", "pcuLftPnT"];

#[derive(Default)]
struct File {
    family: Option<Family>,
    protocol: Option<Protocol>,
    name: Option<String>,
    state: Option<String>,
}

#[derive(Default)]
struct Process {
    pid: Option<u32>,
    command: String,
    uid: Option<String>,
    login: Option<String>,
}

/// Sockets with a concrete local port. Connected sockets (`a->b`), `*:*`
/// and TCP sockets in a state other than LISTEN are skipped.
#[must_use]
pub fn parse_sockets(text: &str) -> Vec<Socket> {
    let mut sockets = Vec::new();
    let mut process = Process::default();
    let mut file: Option<File> = None;
    let flush = |file: &mut Option<File>, process: &Process, sockets: &mut Vec<Socket>| {
        if let Some(f) = file.take()
            && let Some(socket) = to_socket(f, process)
        {
            sockets.push(socket);
        }
    };
    for line in text.lines() {
        let mut chars = line.chars();
        let Some(field) = chars.next() else { continue };
        let value = chars.as_str();
        match field {
            'p' => {
                flush(&mut file, &process, &mut sockets);
                process = Process {
                    pid: value.parse().ok(),
                    ..Process::default()
                };
            }
            'c' => process.command = value.to_string(),
            'u' => process.uid = Some(value.to_string()),
            'L' => process.login = Some(value.to_string()),
            'f' => {
                flush(&mut file, &process, &mut sockets);
                file = Some(File::default());
            }
            't' | 'P' | 'n' | 'T' => {
                let f = file.get_or_insert_with(File::default);
                match field {
                    't' => {
                        f.family = match value {
                            "IPv4" => Some(Family::V4),
                            "IPv6" => Some(Family::V6),
                            _ => None,
                        }
                    }
                    'P' => {
                        f.protocol = match value {
                            "TCP" => Some(Protocol::Tcp),
                            "UDP" => Some(Protocol::Udp),
                            _ => None,
                        }
                    }
                    'n' => f.name = Some(value.to_string()),
                    _ => {
                        if let Some(state) = value.strip_prefix("ST=") {
                            f.state = Some(state.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    flush(&mut file, &process, &mut sockets);
    sockets
}

fn to_socket(file: File, process: &Process) -> Option<Socket> {
    let protocol = file.protocol?;
    let name = file.name?;
    if name.contains("->") {
        return None;
    }
    if protocol == Protocol::Tcp && file.state.as_deref().is_some_and(|s| s != "LISTEN") {
        return None;
    }
    let (host, port) = split_host_port(&name)?;
    Some(Socket {
        protocol,
        address: normalize_address(host, file.family),
        port,
        pid: process.pid,
        process: process.command.clone(),
        user: process.login.clone().or_else(|| process.uid.clone()),
    })
}

/// Working directories from `lsof -a -d cwd -p <pids> -Fn`.
#[must_use]
pub fn parse_cwds(text: &str) -> HashMap<u32, PathBuf> {
    let mut cwds = HashMap::new();
    let mut pid = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.parse().ok();
        } else if let (Some(value), Some(pid)) = (line.strip_prefix('n'), pid) {
            cwds.entry(pid).or_insert_with(|| PathBuf::from(value));
        }
    }
    cwds
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured on macOS 26 with `TCP_ARGS` (trimmed).
    const TCP: &str = "p626\ncrapportd\nu501\nLabdulrahimklis\nf10\ntIPv4\nPTCP\nn*:56996\nTST=LISTEN\nTQR=0\nTQS=0\nf11\ntIPv6\nPTCP\nn*:56996\nTST=LISTEN\nTQR=0\nTQS=0\np1053\ncadb\nu501\nLabdulrahimklis\nf8\ntIPv4\nPTCP\nn127.0.0.1:5037\nTST=LISTEN\np2001\ncnode\nu0\nf22\ntIPv6\nPTCP\nn[::1]:3000\nTST=LISTEN\n";

    const UDP: &str = "p626\ncrapportd\nu501\nLabdulrahimklis\nf31\ntIPv6\nPUDP\nn*:3722\nf38\ntIPv4\nPUDP\nn*:3722\np648\ncidentityservicesd\nu501\nf7\ntIPv4\nPUDP\nn*:*\nf8\ntIPv4\nPUDP\nn10.0.0.2:5353->10.0.0.1:53\n";

    #[test]
    fn parses_listening_tcp() {
        let sockets = parse_sockets(TCP);
        assert_eq!(sockets.len(), 4);
        assert_eq!(sockets[0].address, "0.0.0.0");
        assert_eq!(sockets[1].address, "::");
        assert_eq!(sockets[1].port, 56996);
        assert_eq!(sockets[2].process, "adb");
        assert_eq!(sockets[2].user.as_deref(), Some("abdulrahimklis"));
        assert_eq!(sockets[2].pid, Some(1053));
        // No login name: the uid is shown.
        assert_eq!(sockets[3].user.as_deref(), Some("0"));
        assert_eq!(sockets[3].address, "::1");
        assert_eq!(sockets[3].port, 3000);
    }

    #[test]
    fn keeps_only_bound_udp() {
        let sockets = parse_sockets(UDP);
        assert_eq!(sockets.len(), 2);
        assert!(
            sockets
                .iter()
                .all(|s| s.protocol == Protocol::Udp && s.port == 3722)
        );
    }

    #[test]
    fn skips_tcp_not_listening() {
        let text = "p1\ncx\nf3\ntIPv4\nPTCP\nn127.0.0.1:5000\nTST=ESTABLISHED\n";
        assert!(parse_sockets(text).is_empty());
        assert!(parse_sockets("").is_empty());
    }

    #[test]
    fn parses_cwds() {
        let text = "p626\nfcwd\nn/\np1053\nfcwd\nn/Users/me/code/app\n";
        let cwds = parse_cwds(text);
        assert_eq!(cwds[&626], PathBuf::from("/"));
        assert_eq!(cwds[&1053], PathBuf::from("/Users/me/code/app"));
    }
}
