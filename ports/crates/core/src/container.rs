//! Ports published by containers. On a Mac the socket belongs to the
//! container runtime (`com.docker.backend`, OrbStack, `gvproxy`, …), not
//! to the container, so when such a process holds a port, `docker ps`
//! tells which container published it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::model::{Listener, Protocol};

/// A running container that publishes a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
}

impl Container {
    /// The 12-character id `docker ps` shows.
    #[must_use]
    pub fn short_id(&self) -> &str {
        self.id.get(..12).unwrap_or(&self.id)
    }
}

/// `docker ps --format` template: id, names, image, ports, tab-separated.
pub const FORMAT: &str = "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Ports}}";

/// Whether `process` is a container runtime's port proxy.
#[must_use]
pub fn is_runtime(process: &str) -> bool {
    let process = process.to_lowercase();
    [
        "docker",
        "vpnkit",
        "orbstack",
        "limactl",
        "gvproxy",
        "rootlessport",
        "podman",
    ]
    .iter()
    .any(|name| process.contains(name))
}

/// Host ports of a `Ports` column: `0.0.0.0:5432->5432/tcp, [::]:5432->5432/tcp,
/// 127.0.0.1:8000-8001->80-81/tcp`. Exposed but unpublished ports
/// (`6379/tcp`) have no host side and are skipped.
fn host_ports(ports: &str) -> Vec<(u16, Protocol)> {
    let mut out = Vec::new();
    for mapping in ports.split(',').map(str::trim) {
        let Some((host, container)) = mapping.split_once("->") else {
            continue;
        };
        let protocol = match container.rsplit_once('/') {
            Some((_, "udp")) => Protocol::Udp,
            _ => Protocol::Tcp,
        };
        let Some((_, range)) = host.rsplit_once(':') else {
            continue;
        };
        let (start, end) = match range.split_once('-') {
            Some((a, b)) => (a.parse::<u16>(), b.parse::<u16>()),
            None => (range.parse::<u16>(), range.parse::<u16>()),
        };
        if let (Ok(start), Ok(end)) = (start, end) {
            for port in start..=end.max(start) {
                if !out.contains(&(port, protocol)) {
                    out.push((port, protocol));
                }
            }
        }
    }
    out
}

/// Containers by the host port and protocol they publish, from
/// `docker ps --format` [`FORMAT`] output.
#[must_use]
pub fn parse(text: &str) -> HashMap<(u16, Protocol), Container> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let [id, name, image, ports] = fields[..] else {
            continue;
        };
        let container = Container {
            id: id.trim().to_string(),
            name: name.split(',').next().unwrap_or(name).trim().to_string(),
            image: image.trim().to_string(),
        };
        for key in host_ports(ports) {
            map.entry(key).or_insert_with(|| container.clone());
        }
    }
    map
}

/// The `docker` CLI: on `PATH` from a terminal, but an app started from
/// the Finder gets a short `PATH`, so the usual install places are tried.
#[must_use]
pub fn docker_binary(home: Option<&Path>) -> PathBuf {
    let mut candidates = vec![
        PathBuf::from("/usr/local/bin/docker"),
        PathBuf::from("/opt/homebrew/bin/docker"),
        PathBuf::from("/Applications/Docker.app/Contents/Resources/bin/docker"),
    ];
    if let Some(home) = home {
        candidates.push(home.join(".docker/bin/docker"));
        candidates.push(home.join(".orbstack/bin/docker"));
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("docker"))
}

/// Runs `docker ps`. Blocking.
pub fn containers(home: Option<&Path>) -> Result<HashMap<(u16, Protocol), Container>, String> {
    let output = Command::new(docker_binary(home))
        .args(["ps", "--no-trunc", "--format", FORMAT])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("docker: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(parse(&String::from_utf8_lossy(&output.stdout)))
}

/// Sets [`Listener::container`] on listeners held by a container runtime.
pub fn attach(listeners: &mut [Listener], containers: &HashMap<(u16, Protocol), Container>) {
    for listener in listeners.iter_mut().filter(|l| is_runtime(&l.process)) {
        listener.container = containers.get(&(listener.port, listener.protocol)).cloned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS: &str = "0123456789abcdef0123\tshop-db-1\tpostgres:16\t0.0.0.0:5432->5432/tcp, [::]:5432->5432/tcp\n\
                      fedcba9876543210ffff\tcache,cache-alias\tredis:7\t6379/tcp\n\
                      aaaaaaaaaaaaaaaaaaaa\tweb\tnginx\t127.0.0.1:8000-8001->80-81/tcp, 0.0.0.0:5353->53/udp\n\
                      broken line\n";

    #[test]
    fn maps_published_ports_to_containers() {
        let map = parse(PS);
        let db = &map[&(5432, Protocol::Tcp)];
        assert_eq!(db.name, "shop-db-1");
        assert_eq!(db.image, "postgres:16");
        assert_eq!(db.short_id(), "0123456789ab");
        assert_eq!(map[&(8001, Protocol::Tcp)].name, "web");
        assert_eq!(map[&(5353, Protocol::Udp)].name, "web");
        // Exposed only: not published on the host.
        assert!(!map.contains_key(&(6379, Protocol::Tcp)));
        assert_eq!(map.len(), 4);
    }

    #[test]
    fn recognises_runtimes() {
        for process in [
            "com.docker.backend",
            "vpnkit-bridge",
            "OrbStack Helper",
            "gvproxy",
            "docker-proxy",
        ] {
            assert!(is_runtime(process), "{process}");
        }
        assert!(!is_runtime("node"));
        assert!(!is_runtime("postgres"));
    }

    #[test]
    fn attaches_to_runtime_listeners_only() {
        let listener = |port: u16, process: &str| Listener {
            protocol: Protocol::Tcp,
            port,
            addresses: vec!["::".into()],
            pid: Some(1),
            process: process.into(),
            user: None,
            command: None,
            started: None,
            cwd: None,
            project: None,
            container: None,
        };
        let mut listeners = vec![
            listener(5432, "com.docker.backend"),
            listener(8000, "node"),
            listener(9999, "com.docker.backend"),
        ];
        attach(&mut listeners, &parse(PS));
        assert_eq!(
            listeners[0].container.as_ref().map(|c| c.name.as_str()),
            Some("shop-db-1")
        );
        assert!(listeners[1].container.is_none());
        assert!(listeners[2].container.is_none());
    }
}
