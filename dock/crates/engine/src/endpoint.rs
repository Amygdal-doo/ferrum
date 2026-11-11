//! Where the engine lives: `DOCKER_HOST`, a Docker context, or a well-known Unix socket.

use crate::context::DockerContext;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Endpoint {
    Unix(PathBuf),
    /// `host:port`, plain HTTP (TLS endpoints are not supported yet).
    Tcp(String),
}

impl Endpoint {
    /// Parses a `DOCKER_HOST` value: `unix:///path`, `tcp://host:port`, `http://…`
    /// or a bare absolute socket path. `ssh://` and `npipe://` are not supported.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if let Some(path) = value.strip_prefix("unix://") {
            return (!path.is_empty()).then(|| Self::Unix(PathBuf::from(path)));
        }
        if let Some(addr) = value
            .strip_prefix("tcp://")
            .or_else(|| value.strip_prefix("http://"))
        {
            let addr = addr.trim_end_matches('/');
            return (!addr.is_empty()).then(|| Self::Tcp(addr.to_owned()));
        }
        value
            .starts_with('/')
            .then(|| Self::Unix(PathBuf::from(value)))
    }

    /// The value to pass as `DOCKER_HOST` / `docker -H` so the CLI talks to the same engine.
    #[must_use]
    pub fn docker_host(&self) -> String {
        match self {
            Self::Unix(path) => format!("unix://{}", path.display()),
            Self::Tcp(addr) => format!("tcp://{addr}"),
        }
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unix(path) => write!(f, "{}", path.display()),
            Self::Tcp(addr) => write!(f, "tcp://{addr}"),
        }
    }
}

/// A place an engine may be listening, with a human name for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub endpoint: Endpoint,
    /// "DOCKER_HOST", "Docker Desktop", "OrbStack", "Colima", "Docker" or "Docker context".
    pub source: &'static str,
    /// The Docker context's name, for context candidates.
    pub context: Option<String>,
}

impl Candidate {
    #[must_use]
    pub fn new(endpoint: Endpoint, source: &'static str) -> Self {
        Self {
            endpoint,
            source,
            context: None,
        }
    }

    /// What the UI calls it: `context orbstack`, or the source.
    #[must_use]
    pub fn label(&self) -> String {
        self.context
            .as_ref()
            .map_or_else(|| self.source.to_owned(), |c| format!("context {c}"))
    }
}

fn dedupe(list: Vec<Candidate>) -> Vec<Candidate> {
    let mut unique: Vec<Candidate> = Vec::with_capacity(list.len());
    for candidate in list {
        if !unique.iter().any(|c| c.endpoint == candidate.endpoint) {
            unique.push(candidate);
        }
    }
    unique
}

/// All candidates in priority order: `DOCKER_HOST` first, then the per-user
/// sockets of Docker Desktop, OrbStack and Colima, then the system socket.
#[must_use]
pub fn candidates(docker_host: Option<&str>, home: Option<&Path>) -> Vec<Candidate> {
    let mut list = Vec::new();
    if let Some(endpoint) = docker_host.and_then(Endpoint::parse) {
        list.push(Candidate::new(endpoint, "DOCKER_HOST"));
    }
    if let Some(home) = home {
        for (relative, source) in [
            (".docker/run/docker.sock", "Docker Desktop"),
            (".orbstack/run/docker.sock", "OrbStack"),
            (".colima/default/docker.sock", "Colima"),
        ] {
            list.push(Candidate::new(Endpoint::Unix(home.join(relative)), source));
        }
    }
    list.push(Candidate::new(
        Endpoint::Unix(PathBuf::from("/var/run/docker.sock")),
        "Docker",
    ));
    dedupe(list)
}

/// Adds Docker contexts: the current one right after `DOCKER_HOST` (as the
/// CLI would use it), the others last. Contexts FerrumDock cannot reach
/// (ssh…) are left out; a context on an already listed socket replaces nothing.
#[must_use]
pub fn with_contexts(list: Vec<Candidate>, contexts: &[DockerContext]) -> Vec<Candidate> {
    let candidate = |c: &DockerContext| {
        c.endpoint.clone().map(|endpoint| Candidate {
            endpoint,
            source: "Docker context",
            context: Some(c.name.clone()),
        })
    };
    let split = usize::from(list.first().is_some_and(|c| c.source == "DOCKER_HOST"));
    let mut ordered: Vec<Candidate> = list[..split].to_vec();
    ordered.extend(contexts.iter().filter(|c| c.current).filter_map(candidate));
    ordered.extend(list[split..].iter().cloned());
    ordered.extend(contexts.iter().filter(|c| !c.current).filter_map(candidate));
    dedupe(ordered)
}

/// Moves the candidate at `endpoint` to the front (the engine picked in the toolbar).
#[must_use]
pub fn prefer(mut list: Vec<Candidate>, endpoint: Option<&Endpoint>) -> Vec<Candidate> {
    if let Some(index) = endpoint.and_then(|e| list.iter().position(|c| &c.endpoint == e)) {
        let chosen = list.remove(index);
        list.insert(0, chosen);
    }
    list
}

/// Candidates that may be live: TCP endpoints always, sockets only if the file exists.
#[must_use]
pub fn present(candidates: Vec<Candidate>, exists: impl Fn(&Path) -> bool) -> Vec<Candidate> {
    candidates
        .into_iter()
        .filter(|c| match &c.endpoint {
            Endpoint::Unix(path) => exists(path),
            Endpoint::Tcp(_) => true,
        })
        .collect()
}

/// Present candidates on this machine, from the real environment.
#[must_use]
pub fn detect() -> Vec<Candidate> {
    let docker_host = std::env::var("DOCKER_HOST").ok();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let list = with_contexts(
        candidates(docker_host.as_deref(), home.as_deref()),
        &crate::context::detect(),
    );
    present(list, Path::exists)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docker_host_values() {
        assert_eq!(
            Endpoint::parse("unix:///var/run/docker.sock"),
            Some(Endpoint::Unix("/var/run/docker.sock".into()))
        );
        assert_eq!(
            Endpoint::parse("tcp://127.0.0.1:2375/"),
            Some(Endpoint::Tcp("127.0.0.1:2375".into()))
        );
        assert_eq!(
            Endpoint::parse("/tmp/podman.sock"),
            Some(Endpoint::Unix("/tmp/podman.sock".into()))
        );
        assert_eq!(Endpoint::parse("ssh://me@box"), None);
        assert_eq!(Endpoint::parse("unix://"), None);
        assert_eq!(Endpoint::parse(""), None);
    }

    #[test]
    fn round_trips_docker_host() {
        for value in ["unix:///a/b.sock", "tcp://10.0.0.2:2375"] {
            assert_eq!(
                Endpoint::parse(value).map(|e| e.docker_host()).as_deref(),
                Some(value)
            );
        }
    }

    #[test]
    fn orders_candidates_with_docker_host_first_and_dedupes() {
        let home = Path::new("/Users/me");
        let list = candidates(
            Some("unix:///Users/me/.colima/default/docker.sock"),
            Some(home),
        );
        let sources: Vec<_> = list.iter().map(|c| c.source).collect();
        assert_eq!(
            sources,
            ["DOCKER_HOST", "Docker Desktop", "OrbStack", "Docker"],
            "Colima's socket is already covered by DOCKER_HOST"
        );
        assert_eq!(
            list[1].endpoint,
            Endpoint::Unix("/Users/me/.docker/run/docker.sock".into())
        );
        let without_home = candidates(None, None);
        assert_eq!(without_home.len(), 1);
        assert_eq!(without_home[0].source, "Docker");
    }

    #[test]
    fn keeps_only_existing_sockets_and_tcp() {
        let list = candidates(Some("tcp://10.0.0.2:2375"), Some(Path::new("/h")));
        let live = present(list, |p| p == Path::new("/h/.orbstack/run/docker.sock"));
        let sources: Vec<_> = live.iter().map(|c| c.source).collect();
        assert_eq!(sources, ["DOCKER_HOST", "OrbStack"]);
        assert!(present(candidates(None, Some(Path::new("/h"))), |_| false).is_empty());
    }

    #[test]
    fn orders_docker_contexts_and_prefers_the_picked_engine() {
        let ctx = |name: &str, host: &str, current| DockerContext {
            name: name.into(),
            host: host.into(),
            endpoint: Endpoint::parse(host),
            current,
        };
        let contexts = [
            ctx("colima-dev", "unix:///h/.colima/dev/docker.sock", true),
            ctx("orb", "unix:///h/.orbstack/run/docker.sock", false),
            ctx("remote", "ssh://me@box", false),
            ctx("lab", "tcp://10.0.0.9:2375", false),
        ];
        let list = with_contexts(
            candidates(Some("tcp://1.2.3.4:2375"), Some(Path::new("/h"))),
            &contexts,
        );
        let labels: Vec<_> = list.iter().map(Candidate::label).collect();
        assert_eq!(
            labels,
            [
                "DOCKER_HOST",
                "context colima-dev",
                "Docker Desktop",
                "OrbStack",
                "Colima",
                "Docker",
                "context lab"
            ],
            "orb's socket is already OrbStack; ssh is skipped"
        );
        let without_host = with_contexts(candidates(None, None), &contexts[..1]);
        assert_eq!(without_host[0].context.as_deref(), Some("colima-dev"));

        let picked = Endpoint::Unix("/var/run/docker.sock".into());
        let preferred = prefer(list.clone(), Some(&picked));
        assert_eq!(preferred[0].source, "Docker");
        assert_eq!(preferred.len(), list.len());
        assert_eq!(prefer(list.clone(), None), list);
        assert_eq!(
            prefer(list.clone(), Some(&Endpoint::Tcp("x:1".into()))),
            list
        );
    }
}
