//! A first diagram generated from a project's files: docker-compose files,
//! Kubernetes manifests and, when neither is there, package manifests
//! (`package.json`, `Cargo.toml`, `pyproject.toml`, `go.mod`…). Each
//! service becomes a component (its kind and technology guessed from the
//! image or the dependencies), `depends_on`, links and addresses in the
//! environment become connections. The same files always give the same
//! diagram; nothing is written here.

mod compose;
mod k8s;
mod manifests;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::catalog::{Category, Kind, Tech};
use crate::model::{Edge, EdgeStyle, Node, System};

/// Folders never looked into.
const SKIP_DIRS: [&str; 14] = [
    ".git",
    ".architecture",
    ".claude",
    "node_modules",
    "target",
    "vendor",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    ".terraform",
    "coverage",
];
const MAX_DEPTH: usize = 4;
const MAX_FILE: u64 = 1 << 20;

/// What was generated, and from which files (relative to the project).
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub system: System,
    pub sources: Vec<PathBuf>,
}

/// The diagram the project's files describe; `None` when none of them
/// says anything about its services.
pub fn from_project(project: &Path) -> Option<Imported> {
    let mut files = Vec::new();
    walk(project, 0, &mut files);
    files.sort();
    let mut b = Builder::default();
    let mut objects = Vec::new();
    for path in &files {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let is_yaml = name.ends_with(".yml") || name.ends_with(".yaml");
        if !is_yaml {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let found = if compose::is_compose(name) {
            compose::read(path, &text, &mut b)
        } else {
            let docs = k8s::parse(&text);
            let found = !docs.is_empty();
            objects.extend(docs);
            found
        };
        if found {
            b.sources.push(relative(project, path));
        }
    }
    k8s::read(&objects, &mut b);
    if b.nodes.is_empty() {
        for path in &files {
            if manifests::read(path, &mut b) {
                b.sources.push(relative(project, path));
            }
        }
    }
    if b.nodes.is_empty() {
        return None;
    }
    let name = project
        .file_name()
        .map_or_else(|| "System".to_owned(), |n| n.to_string_lossy().into_owned());
    let sources = b.sources.clone();
    Some(Imported {
        system: b.finish(&name, &sources),
        sources,
    })
}

/// Adds what `imported` has and `system` lacks (layers, components,
/// connections, the cluster); what is already there is left alone.
pub fn merge(system: &mut System, imported: &System) {
    for layer in &imported.layers {
        if !system.layers.iter().any(|l| l.id == layer.id) {
            system.layers.push(layer.clone());
        }
    }
    for node in &imported.nodes {
        if system.node(&node.id).is_none() {
            let mut node = node.clone();
            node.slot = Some(system.nodes_in(&node.layer).len() as f32);
            system.nodes.push(node);
        }
    }
    for edge in &imported.edges {
        if !system
            .edges
            .iter()
            .any(|e| e.from == edge.from && e.to == edge.to)
        {
            system.edges.push(edge.clone());
        }
    }
    if system.cluster.is_none() {
        system.cluster.clone_from(&imported.cluster);
    }
}

fn relative(project: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(project).unwrap_or(path).to_path_buf()
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if kind.is_dir() {
            if depth < MAX_DEPTH && !SKIP_DIRS.contains(&name.as_ref()) {
                walk(&path, depth + 1, out);
            }
        } else if kind.is_file() && entry.metadata().is_ok_and(|m| m.len() <= MAX_FILE) {
            out.push(path);
        }
    }
}

/// The components and connections found so far.
#[derive(Default)]
pub(crate) struct Builder {
    nodes: BTreeMap<String, Node>,
    edges: Vec<(String, String)>,
    cluster: Option<crate::model::Cluster>,
    sources: Vec<PathBuf>,
}

impl Builder {
    /// Adds `node` unless one with its id is there already (the first
    /// found wins; it only gains what it lacked).
    fn add(&mut self, node: Node) {
        match self.nodes.get_mut(&node.id) {
            Some(old) => {
                if old.tech.is_none() {
                    old.tech = node.tech;
                }
                if old.deploy.is_none() {
                    old.deploy = node.deploy;
                }
            }
            None => {
                self.nodes.insert(node.id.clone(), node);
            }
        }
    }

    fn connect(&mut self, from: &str, to: &str) {
        let pair = (from.to_owned(), to.to_owned());
        if from != to && !self.edges.contains(&pair) {
            self.edges.push(pair);
        }
    }

    fn finish(mut self, name: &str, sources: &[PathBuf]) -> System {
        self.edges
            .retain(|(a, b)| self.nodes.contains_key(a) && self.nodes.contains_key(b));
        self.edges.sort();
        let mut layers = crate::demos::standard_layers();
        layers.retain(|l| self.nodes.values().any(|n| n.layer == l.id));
        let mut slots: BTreeMap<String, f32> = BTreeMap::new();
        let mut nodes: Vec<Node> = self.nodes.into_values().collect();
        for node in &mut nodes {
            let slot = slots.entry(node.layer.clone()).or_default();
            node.slot = Some(*slot);
            *slot += 1.;
        }
        let edges = self
            .edges
            .iter()
            .map(|(from, to)| {
                let async_target = nodes
                    .iter()
                    .find(|n| n.id == *to)
                    .and_then(|n| Kind::find(&n.kind))
                    .is_some_and(|k| k.category == Category::Messaging);
                Edge {
                    from: from.clone(),
                    to: to.clone(),
                    style: if async_target {
                        EdgeStyle::Async
                    } else {
                        EdgeStyle::Request
                    },
                    ..Edge::default()
                }
            })
            .collect();
        let from: Vec<String> = sources.iter().map(|p| p.display().to_string()).collect();
        System {
            name: name.to_owned(),
            tagline: "Generated from the project's files".into(),
            overview: format!(
                "Generated by FerrumSystem from {}. Check the components and connections, \
                 then describe what the system does.",
                from.join(", ")
            ),
            layers,
            nodes,
            edges,
            cluster: self.cluster,
            ..System::default()
        }
    }
}

/// A component id from a service name (`Orders API` → `orders-api`).
fn slug(name: &str) -> String {
    let id: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let id = id
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if id.is_empty() { "service".into() } else { id }
}

/// What an image (or a service name) most likely is: (kind, technology).
const IMAGES: &[(&str, &str, &str)] = &[
    ("postgis", "sql-db", "postgresql"),
    ("timescale", "time-series", "postgresql"),
    ("pgvector", "vector-db", "pgvector"),
    ("postgres", "sql-db", "postgresql"),
    ("mysql", "sql-db", "mysql"),
    ("mariadb", "sql-db", "mysql"),
    ("mssql", "sql-db", "sqlserver"),
    ("oracle", "sql-db", "oracle"),
    ("mongo", "nosql-db", "mongodb"),
    ("cassandra", "wide-column", "cassandra"),
    ("scylla", "wide-column", "scylladb"),
    ("clickhouse", "warehouse", "clickhouse"),
    ("valkey", "cache", "redis"),
    ("redis", "cache", "redis"),
    ("memcached", "cache", "memcached"),
    ("rabbitmq", "queue", "rabbitmq"),
    ("redpanda", "stream", "kafka"),
    ("kafka", "stream", "kafka"),
    ("nats", "queue", "nats"),
    ("elasticsearch", "search", "elasticsearch"),
    ("opensearch", "search", "opensearch"),
    ("minio", "object-storage", "minio"),
    ("qdrant", "vector-db", "qdrant"),
    ("weaviate", "vector-db", "weaviate"),
    ("chroma", "vector-db", "chroma"),
    ("ollama", "llm", "ollama"),
    ("vllm", "llm", "vllm"),
    ("keycloak", "auth", "keycloak"),
    ("vault", "secrets", "vault"),
    ("prometheus", "monitoring", "prometheus"),
    ("grafana", "monitoring", "grafana"),
    ("otel", "logging", "opentelemetry"),
    ("jaeger", "logging", "opentelemetry"),
    ("loki", "logging", "grafana"),
    ("traefik", "load-balancer", "traefik"),
    ("envoy", "load-balancer", "envoy"),
    ("nginx", "load-balancer", "nginx"),
    ("haproxy", "load-balancer", ""),
    ("caddy", "load-balancer", ""),
    ("node", "service", "nodejs"),
    ("python", "service", "python"),
    ("golang", "service", "go"),
    ("rust", "service", "rust"),
    ("openjdk", "service", "java"),
    ("temurin", "service", "java"),
    ("dotnet", "service", "dotnet"),
    ("php", "service", "php"),
    ("ruby", "service", "ruby"),
];

/// (kind, technology) for an image like `bitnami/postgresql:16`.
fn guess(image: &str) -> (&'static str, Option<String>) {
    let name = image.rsplit('/').next().unwrap_or(image);
    let name = name.split([':', '@']).next().unwrap_or(name).to_lowercase();
    IMAGES
        .iter()
        .find(|(needle, _, _)| name.contains(needle))
        .map_or(("service", None), |(_, kind, tech)| {
            (*kind, Tech::find(tech).map(|t| t.id.to_owned()))
        })
}

/// The standard layer a component of `kind` goes in.
fn layer_for(kind: &str) -> &'static str {
    match kind {
        "load-balancer" | "api-gateway" | "cdn" | "waf" | "dns" | "auth" => "edge",
        "vector-db" => "data",
        _ => match Kind::find(kind).map(|k| k.category) {
            Some(Category::Client | Category::Mobile | Category::Frontend) => "clients",
            Some(Category::Messaging) => "async",
            Some(Category::Database) => "data",
            Some(Category::Infra | Category::Cloud) => "platform",
            _ => "services",
        },
    }
}

/// A component `name` of `kind`, in its layer.
fn node(name: &str, kind: &str, tech: Option<String>) -> Node {
    Node {
        id: slug(name),
        name: name.to_owned(),
        kind: kind.to_owned(),
        tech,
        layer: layer_for(kind).to_owned(),
        ..Node::default()
    }
}

/// The services among `names` an address in `value` points at
/// (`postgres://db:5432/app`, `http://orders/`, `redis:6379`, `db`).
fn mentioned<'a>(value: &str, names: impl IntoIterator<Item = &'a String>) -> Vec<&'a String> {
    let value = value.to_lowercase();
    names
        .into_iter()
        .filter(|name| {
            let name = name.to_lowercase();
            value == name
                || [":", "/", "."].iter().any(|after| {
                    ["//", "@", ""].iter().any(|before| {
                        let needle = format!("{before}{name}{after}");
                        value.starts_with(&needle)
                            || (!before.is_empty() && value.contains(&needle))
                    })
                })
                || value.ends_with(&format!("//{name}"))
        })
        .collect()
}

#[cfg(test)]
mod tests;
