//! Problems in a design, found from what the diagram already says: where
//! each component runs (replicas, health check, image), which layer and
//! kind it is, the connections and the flows. Each problem points at the
//! component to fix.

use std::collections::{BTreeSet, VecDeque};

use crate::catalog::{Category, Kind};
use crate::model::{DeployTarget, EdgeStyle, Node, System};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rule {
    /// A database a client or edge component talks to directly.
    PublicDatabase,
    /// A request from a client or edge component into a service, with no
    /// authentication said.
    NoAuth,
    /// One replica of something every request goes through.
    SingleReplica,
    /// A pod, container or VM nothing checks the health of.
    NoHealthCheck,
    /// A connection, API, flow step, layer or database that names
    /// something that is not there.
    Dangling,
    /// An image without a version (`:latest` or none).
    UnpinnedImage,
    /// A component with no connection at all.
    Isolated,
}

impl Rule {
    pub fn label(self) -> &'static str {
        match self {
            Self::PublicDatabase => "Database reachable from a public layer",
            Self::NoAuth => "Connection without auth",
            Self::SingleReplica => "Single replica on a critical path",
            Self::NoHealthCheck => "No health check",
            Self::Dangling => "Unknown reference",
            Self::UnpinnedImage => "Image without a version",
            Self::Isolated => "Not connected",
        }
    }

    pub fn severity(self) -> Severity {
        match self {
            Self::UnpinnedImage | Self::Isolated => Severity::Info,
            _ => Severity::Warning,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub rule: Rule,
    /// The component to fix (selected when the problem is clicked).
    pub node: Option<String>,
    pub message: String,
}

/// Words in a connection's label that say how it is authenticated.
const AUTH_WORDS: [&str; 12] = [
    "auth", "jwt", "oauth", "oidc", "mtls", "token", "api key", "apikey", "iam", "saml", "signed",
    "session",
];

/// Kinds at the edge of the system, reachable from outside.
const EDGE_KINDS: [&str; 6] = [
    "load-balancer",
    "api-gateway",
    "cdn",
    "waf",
    "dns",
    "static-site",
];

fn category(node: &Node) -> Option<Category> {
    Kind::find(&node.kind).map(|k| k.category)
}

/// Reachable from outside: a client, or on the edge (by kind or layer).
fn is_public(system: &System, node: &Node) -> bool {
    let layer = system
        .layers
        .iter()
        .find(|l| l.id == node.layer)
        .map(|l| format!("{} {}", l.id, l.name).to_lowercase())
        .unwrap_or_default();
    matches!(
        category(node),
        Some(Category::Client | Category::Mobile | Category::Frontend)
    ) || EDGE_KINDS.contains(&node.kind.as_str())
        || ["client", "public", "edge", "dmz"]
            .iter()
            .any(|w| layer.contains(w))
}

fn is_datastore(node: &Node) -> bool {
    node.database.is_some()
        || category(node) == Some(Category::Database)
        || node.kind == "vector-db"
}

/// Runs as pods, containers or VMs: replicas and health checks are ours.
fn runs(node: &Node) -> Option<&crate::model::Deploy> {
    node.deploy.as_ref().filter(|d| {
        matches!(
            d.target,
            DeployTarget::Pod | DeployTarget::Container | DeployTarget::Vm
        )
    })
}

fn synchronous(style: EdgeStyle) -> bool {
    matches!(
        style,
        EdgeStyle::Request | EdgeStyle::Rpc | EdgeStyle::Socket
    )
}

/// Components on a critical path: in a flow, or reached from a public
/// component through synchronous connections.
fn critical(system: &System) -> BTreeSet<&str> {
    let mut seen: BTreeSet<&str> = system
        .flows
        .iter()
        .flat_map(|f| &f.steps)
        .flat_map(|s| [s.from.as_str(), s.to.as_str()])
        .collect();
    let mut queue: VecDeque<&str> = system
        .nodes
        .iter()
        .filter(|n| is_public(system, n))
        .map(|n| n.id.as_str())
        .collect();
    let mut visited: BTreeSet<&str> = queue.iter().copied().collect();
    while let Some(id) = queue.pop_front() {
        for e in system
            .edges
            .iter()
            .filter(|e| e.from == id && synchronous(e.style))
        {
            if visited.insert(e.to.as_str()) {
                seen.insert(e.to.as_str());
                queue.push_back(e.to.as_str());
            }
        }
    }
    seen
}

/// Every problem, the most serious first, in diagram order.
pub fn check(system: &System) -> Vec<Problem> {
    let mut out = Vec::new();
    let mut add = |rule: Rule, node: Option<&str>, message: String| {
        out.push(Problem {
            rule,
            node: node.map(str::to_owned),
            message,
        });
    };
    let critical = critical(system);
    for n in &system.nodes {
        let name = &n.name;
        if let Some(d) = runs(n) {
            if d.health.as_deref().is_none_or(|h| h.trim().is_empty()) {
                add(
                    Rule::NoHealthCheck,
                    Some(&n.id),
                    format!(
                        "{name} runs as {} with no health check.",
                        d.target.label().to_lowercase()
                    ),
                );
            }
            if d.replicas.unwrap_or(1) <= 1 && critical.contains(n.id.as_str()) {
                add(
                    Rule::SingleReplica,
                    Some(&n.id),
                    format!("{name} has one replica and requests go through it."),
                );
            }
            if let Some(image) = &d.image
                && image_unpinned(image)
            {
                add(
                    Rule::UnpinnedImage,
                    Some(&n.id),
                    format!("{name} runs {image}: pin a version."),
                );
            }
        }
        if !system.layers.iter().any(|l| l.id == n.layer) {
            add(
                Rule::Dangling,
                Some(&n.id),
                format!("{name} is on layer \"{}\", which does not exist.", n.layer),
            );
        }
        if let Some(db) = &n.database
            && system.database(db).is_none()
        {
            add(
                Rule::Dangling,
                Some(&n.id),
                format!("{name} stores database \"{db}\", which is not described."),
            );
        }
        if !system.edges.iter().any(|e| e.from == n.id || e.to == n.id)
            && category(n) != Some(Category::Infra)
        {
            add(
                Rule::Isolated,
                Some(&n.id),
                format!("{name} has no connections."),
            );
        }
    }
    for e in &system.edges {
        let (Some(from), Some(to)) = (system.node(&e.from), system.node(&e.to)) else {
            let known = system.node(&e.from).map(|n| n.id.as_str());
            add(
                Rule::Dangling,
                known,
                format!(
                    "The connection {} → {} names a missing component.",
                    e.from, e.to
                ),
            );
            continue;
        };
        if !is_public(system, from) {
            continue;
        }
        if is_datastore(to) && !is_public(system, to) {
            add(
                Rule::PublicDatabase,
                Some(&to.id),
                format!(
                    "{} is reached directly from {}, a public component.",
                    to.name, from.name
                ),
            );
        } else if synchronous(e.style)
            && !has_auth(e)
            && !authenticates(system, from)
            && needs_auth(to)
            && !is_public(system, to)
        {
            add(
                Rule::NoAuth,
                Some(&to.id),
                format!(
                    "{} → {} does not say how callers are authenticated.",
                    from.name, to.name
                ),
            );
        }
    }
    let missing = |id: &str| system.node(id).is_none();
    for api in system.apis.iter().filter(|a| missing(&a.service)) {
        add(
            Rule::Dangling,
            None,
            format!(
                "{} {} is served by \"{}\", which is not on the diagram.",
                api.method, api.path, api.service
            ),
        );
    }
    for socket in system.sockets.iter().filter(|s| missing(&s.service)) {
        add(
            Rule::Dangling,
            None,
            format!(
                "{} is served by \"{}\", which is not on the diagram.",
                socket.channel, socket.service
            ),
        );
    }
    for flow in &system.flows {
        for step in flow
            .steps
            .iter()
            .filter(|s| missing(&s.from) || missing(&s.to))
        {
            add(
                Rule::Dangling,
                None,
                format!(
                    "Flow \"{}\": {} → {} names a missing component.",
                    flow.name, step.from, step.to
                ),
            );
        }
    }
    out.sort_by_key(|p| (p.rule.severity(), p.rule));
    out
}

fn has_auth(edge: &crate::model::Edge) -> bool {
    edge.auth.as_deref().is_some_and(|a| !a.trim().is_empty())
        || edge
            .label
            .as_deref()
            .map(str::to_lowercase)
            .is_some_and(|l| AUTH_WORDS.iter().any(|w| l.contains(w)))
}

/// Checks its callers itself: an identity service, or a gateway that
/// talks to one.
fn authenticates(system: &System, node: &Node) -> bool {
    let is_auth = |id: &str| system.node(id).is_some_and(|n| n.kind == "auth");
    node.kind == "auth"
        || system
            .edges
            .iter()
            .any(|e| e.from == node.id && is_auth(&e.to))
}

/// Serves requests of its own: a service, a gateway, a function…
fn needs_auth(node: &Node) -> bool {
    node.kind == "api-gateway"
        || (category(node) == Some(Category::Backend) && node.kind != "auth")
        || node.kind == "realtime"
}

fn image_unpinned(image: &str) -> bool {
    let name = image.rsplit('/').next().unwrap_or(image);
    !image.contains('@') && name.split_once(':').is_none_or(|(_, tag)| tag == "latest")
}

#[cfg(test)]
mod tests;
