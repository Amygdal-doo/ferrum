//! Three worked examples (core banking on microservices, a RAG assistant on
//! Ollama with Claude Code, live delivery tracking with mobile apps) and the
//! starter system a new `.architecture/` folder gets.

mod banking;
mod delivery;
mod rag;

pub use banking::banking;
pub use delivery::delivery;
pub use rag::rag;

use crate::catalog::Provider;
use crate::model::*;

/// The demos, in sidebar order.
pub fn all() -> Vec<System> {
    vec![banking(), rag(), delivery()]
}

/// What "Create architecture" writes: the usual layers and a client, a
/// gateway, one service and its database to start from.
pub fn starter(name: &str) -> System {
    System {
        name: name.to_owned(),
        tagline: "Describe what this system does in one line".into(),
        overview: "Start with the clients and the data, then add the services between \
                   them. Double-click a database to describe its tables."
            .into(),
        provider: Provider::Generic,
        layers: standard_layers(),
        nodes: vec![
            node("web", "Web app", "web-client", Some("react"), "clients", "").device(),
            node("gateway", "API gateway", "api-gateway", None, "edge", "").managed(),
            node(
                "api",
                "API service",
                "service",
                Some("rust"),
                "services",
                "",
            )
            .pods(2),
            node(
                "db",
                "Main database",
                "sql-db",
                Some("postgresql"),
                "data",
                "",
            )
            .db("main")
            .managed(),
        ],
        edges: vec![
            edge("web", "gateway", EdgeStyle::Request, Some("HTTPS")),
            edge("gateway", "api", EdgeStyle::Rpc, None),
            edge("api", "db", EdgeStyle::Request, Some("SQL")),
        ],
        databases: vec![Database {
            id: "main".into(),
            name: "Main database".into(),
            tables: vec![table(
                "users",
                "Who can sign in",
                &[
                    ("id", "uuid", "pk"),
                    ("email", "text", ""),
                    ("created_at", "timestamptz", ""),
                ],
            )],
        }],
        cluster: Some(Cluster {
            context: None,
            namespace: "default".into(),
        }),
        ..System::default()
    }
}

pub(crate) fn standard_layers() -> Vec<Layer> {
    vec![
        layer("clients", "Clients", 0x6366f1),
        layer("edge", "Edge & security", 0x0ea5e9),
        layer("services", "Services", 0x22c55e),
        layer("async", "Async & streaming", 0xa855f7),
        layer("data", "Data", 0xec4899),
        layer("platform", "Platform & operations", 0xeab308),
    ]
}

pub(crate) fn layer(id: &str, name: &str, color: u32) -> Layer {
    Layer {
        id: id.into(),
        name: name.into(),
        color,
    }
}

pub(crate) fn node(
    id: &str,
    name: &str,
    kind: &str,
    tech: Option<&str>,
    layer: &str,
    description: &str,
) -> Node {
    Node {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
        tech: tech.map(Into::into),
        layer: layer.into(),
        slot: None,
        description: description.into(),
        database: None,
        deploy: None,
        items: Vec::new(),
        style: NodeStyle::default(),
    }
}

impl Node {
    pub(crate) fn slot(mut self, slot: f32) -> Self {
        self.slot = Some(slot);
        self
    }

    pub(crate) fn db(mut self, id: &str) -> Self {
        self.database = Some(id.into());
        self
    }

    fn target(mut self, target: DeployTarget, replicas: Option<u32>) -> Self {
        self.deploy = Some(Deploy {
            target,
            replicas,
            image: None,
            selector: None,
            port: None,
            health: None,
        });
        self
    }

    pub(crate) fn pods(self, replicas: u32) -> Self {
        let id = self.id.clone();
        let mut node = self.target(DeployTarget::Pod, Some(replicas));
        if let Some(deploy) = node.deploy.as_mut() {
            deploy.image = Some(format!("registry.local/{id}:1.0"));
            deploy.port = Some(8080);
        }
        node
    }

    pub(crate) fn container(self) -> Self {
        self.target(DeployTarget::Container, Some(1))
    }

    pub(crate) fn managed(self) -> Self {
        self.target(DeployTarget::Managed, None)
    }

    pub(crate) fn vm(self) -> Self {
        self.target(DeployTarget::Vm, Some(1))
    }

    pub(crate) fn device(self) -> Self {
        self.target(DeployTarget::Device, None)
    }
}

pub(crate) fn edge(from: &str, to: &str, style: EdgeStyle, label: Option<&str>) -> Edge {
    Edge {
        from: from.into(),
        to: to.into(),
        style,
        label: label.map(Into::into),
        ..Edge::default()
    }
}

impl Node {
    /// What is inside it, shown on the card.
    pub(crate) fn items(mut self, names: &[&str]) -> Self {
        self.items = names
            .iter()
            .map(|n| Item {
                name: (*n).into(),
                detail: String::new(),
            })
            .collect();
        self
    }
}

pub(crate) fn api(method: &str, path: &str, service: &str, summary: &str) -> Api {
    Api {
        method: method.into(),
        path: path.into(),
        service: service.into(),
        summary: summary.into(),
    }
}

pub(crate) fn socket(channel: &str, service: &str, direction: &str, summary: &str) -> Socket {
    Socket {
        channel: channel.into(),
        service: service.into(),
        direction: direction.into(),
        summary: summary.into(),
    }
}

/// Columns as `(name, type, flags)`; flags: `pk`, `null`, `fk:table.column`.
pub(crate) fn table(name: &str, note: &str, columns: &[(&str, &str, &str)]) -> Table {
    Table {
        name: name.into(),
        note: note.into(),
        columns: columns
            .iter()
            .map(|(name, ty, flags)| Column {
                name: (*name).into(),
                ty: (*ty).into(),
                primary: flags.split(' ').any(|f| f == "pk"),
                nullable: flags.split(' ').any(|f| f == "null"),
                references: flags
                    .split(' ')
                    .find_map(|f| f.strip_prefix("fk:"))
                    .map(Into::into),
            })
            .collect(),
    }
}

pub(crate) fn flow(name: &str, summary: &str, steps: &[(&str, &str, &str)]) -> Flow {
    Flow {
        name: name.into(),
        summary: summary.into(),
        steps: steps
            .iter()
            .map(|(from, to, text)| Step {
                from: (*from).into(),
                to: (*to).into(),
                text: (*text).into(),
            })
            .collect(),
    }
}

pub(crate) fn note(title: &str, text: &str) -> Note {
    Note {
        title: title.into(),
        text: text.into(),
    }
}

pub(crate) fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Kind, Tech};

    /// Every reference in every demo resolves.
    #[test]
    fn demos_are_consistent() {
        for system in all().into_iter().chain([starter("New")]) {
            let layers: Vec<_> = system.layers.iter().map(|l| l.id.as_str()).collect();
            for n in &system.nodes {
                assert!(
                    layers.contains(&n.layer.as_str()),
                    "{}: {}",
                    system.name,
                    n.id
                );
                assert!(
                    Kind::find(&n.kind).is_some(),
                    "{}: kind {}",
                    system.name,
                    n.kind
                );
                if let Some(t) = &n.tech {
                    assert!(Tech::find(t).is_some(), "{}: tech {t}", system.name);
                }
                if let Some(db) = &n.database {
                    assert!(system.database(db).is_some(), "{}: db {db}", system.name);
                }
            }
            let ids: std::collections::HashSet<_> = system.nodes.iter().map(|n| &n.id).collect();
            assert_eq!(
                ids.len(),
                system.nodes.len(),
                "{}: duplicate ids",
                system.name
            );
            for e in &system.edges {
                assert!(
                    system.node(&e.from).is_some(),
                    "{}: {}",
                    system.name,
                    e.from
                );
                assert!(system.node(&e.to).is_some(), "{}: {}", system.name, e.to);
            }
            for f in &system.flows {
                for s in &f.steps {
                    assert!(
                        system.node(&s.from).is_some() && system.node(&s.to).is_some(),
                        "{}: {} {} -> {}",
                        system.name,
                        f.name,
                        s.from,
                        s.to
                    );
                }
            }
            for a in &system.apis {
                assert!(
                    system.node(&a.service).is_some(),
                    "{}: api {}",
                    system.name,
                    a.path
                );
            }
            for s in &system.sockets {
                assert!(
                    system.node(&s.service).is_some(),
                    "{}: socket {}",
                    system.name,
                    s.channel
                );
            }
        }
    }
}
