//! The architecture of one system, as saved in `.architecture/system.json`:
//! layers (the horizontal bands), components on them, the connections
//! between components, and what explains the design (requirements, APIs,
//! sockets, data, flows, reliability, where each component runs).

use serde::{Deserialize, Serialize};

use crate::catalog::Provider;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct System {
    pub name: String,
    /// One line under the name.
    pub tagline: String,
    /// A paragraph: what the system does and the main idea of the design.
    pub overview: String,
    /// Which cloud's service names the diagram shows.
    pub provider: Provider,
    pub layers: Vec<Layer>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub concepts: Vec<String>,
    pub functional: Vec<String>,
    pub non_functional: Vec<String>,
    pub apis: Vec<Api>,
    pub sockets: Vec<Socket>,
    pub databases: Vec<Database>,
    pub flows: Vec<Flow>,
    pub reliability: Vec<Note>,
    /// Kubernetes cluster the pods run in, for the Runtime tab.
    pub cluster: Option<Cluster>,
    /// Technologies the catalogue lacks, with an icon in `.architecture/icons/`.
    pub custom_techs: Vec<CustomTech>,
}

/// A horizontal band of the diagram ("Clients", "Data"…).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub id: String,
    pub name: String,
    /// `0xRRGGBB` of the band's marker.
    pub color: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Node {
    pub id: String,
    pub name: String,
    /// What it is, a [`crate::catalog::Kind`] id (`load-balancer`,
    /// `sql-db`…): decides the cloud service name.
    pub kind: String,
    /// What it is built with, a technology id (`rust`, `postgresql`…).
    pub tech: Option<String>,
    pub layer: String,
    /// Horizontal slot within the layer (0 = leftmost); `None` = in order.
    pub slot: Option<f32>,
    pub description: String,
    /// Id of the [`Database`] it stores (double-click opens the schema).
    pub database: Option<String>,
    /// Where it runs.
    pub deploy: Option<Deploy>,
    /// What is inside it (modules, endpoints, tables, jobs…), shown on the card.
    pub items: Vec<Item>,
    /// How the card looks; empty = the defaults.
    pub style: NodeStyle,
}

/// A card's look. Every field is optional: `None` keeps the default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeStyle {
    /// Accent (`0xRRGGBB`): the stripe, border and tint; default the layer's.
    pub color: Option<u32>,
    /// Card fill (`0xRRGGBB`).
    pub fill: Option<u32>,
    /// Draw a border at all.
    pub border: Option<bool>,
    /// Border widths: top, right, bottom, left.
    pub border_widths: Option<[f32; 4]>,
    pub radius: Option<f32>,
    /// Small text above the title; default the cloud service name.
    pub above: Option<String>,
    /// Text under the title.
    pub below: Option<String>,
    /// A paragraph inside the card.
    pub body: Option<String>,
    /// Show the icon on the left.
    pub icon: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    pub name: String,
    pub detail: String,
}

/// A side of a component card, where a connection leaves or arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    pub const ALL: [Self; 4] = [Self::Top, Self::Right, Self::Bottom, Self::Left];

    pub fn label(self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Right => "Right",
            Self::Bottom => "Bottom",
            Self::Left => "Left",
        }
    }
}

/// How a component is deployed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Deploy {
    pub target: DeployTarget,
    /// Container image (`ghcr.io/acme/ledger:1.4`).
    pub image: Option<String>,
    pub replicas: Option<u32>,
    /// Kubernetes label `app=<selector>` (defaults to the node id), or the
    /// Docker container name.
    pub selector: Option<String>,
    pub port: Option<u16>,
    /// How its health is checked (`GET /healthz`, a readiness probe…);
    /// `None` = not at all.
    pub health: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeployTarget {
    #[default]
    Pod,
    Container,
    Vm,
    Serverless,
    /// A managed cloud service (nothing to run).
    Managed,
    /// Runs on the user's device (apps, browsers).
    Device,
}

impl DeployTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pod => "Kubernetes pods",
            Self::Container => "Docker container",
            Self::Vm => "Virtual machine",
            Self::Serverless => "Serverless",
            Self::Managed => "Managed service",
            Self::Device => "User's device",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub style: EdgeStyle,
    pub label: Option<String>,
    /// Side it leaves from; `None` = chosen from the layout.
    pub from_side: Option<Side>,
    /// Side it arrives at; `None` = chosen from the layout.
    pub to_side: Option<Side>,
    /// `0xRRGGBB`; `None` = the style's colour.
    pub color: Option<u32>,
    /// Line width in points; `None` = the default.
    pub width: Option<f32>,
    /// How the caller is authenticated (`mTLS`, `JWT`, `API key`…);
    /// `None` = not said.
    pub auth: Option<String>,
}

/// How two components talk; drawn differently and listed in the legend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeStyle {
    /// Request / response over HTTPS.
    #[default]
    Request,
    /// Internal RPC (gRPC).
    Rpc,
    /// Message or job through a queue or stream.
    Async,
    /// Long-lived socket (WebSocket, SSE).
    Socket,
    /// Change data capture / replication.
    Cdc,
}

impl EdgeStyle {
    pub const ALL: [Self; 5] = [
        Self::Request,
        Self::Rpc,
        Self::Async,
        Self::Socket,
        Self::Cdc,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Request => "Request (HTTPS)",
            Self::Rpc => "Internal RPC (gRPC)",
            Self::Async => "Async message / job",
            Self::Socket => "Socket (WebSocket / SSE)",
            Self::Cdc => "Change data capture",
        }
    }

    /// Drawn dashed.
    pub fn dashed(self) -> bool {
        matches!(self, Self::Async | Self::Cdc | Self::Socket)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Api {
    pub method: String,
    pub path: String,
    /// Node id that serves it.
    pub service: String,
    pub summary: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Socket {
    /// Channel or endpoint (`wss://…/ws/transfers`).
    pub channel: String,
    pub service: String,
    /// `server → client`, `both`…
    pub direction: String,
    pub summary: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Database {
    pub id: String,
    pub name: String,
    pub tables: Vec<Table>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Table {
    pub name: String,
    pub note: String,
    pub columns: Vec<Column>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Column {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub primary: bool,
    pub nullable: bool,
    /// `table.column` it references.
    pub references: Option<String>,
}

/// A request or event traced through the system, step by step.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Flow {
    pub name: String,
    pub summary: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Step {
    pub from: String,
    pub to: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Note {
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cluster {
    /// `kubectl` context; `None` = the current one.
    pub context: Option<String>,
    pub namespace: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomTech {
    pub id: String,
    pub name: String,
    pub category: crate::catalog::Category,
    pub color: u32,
}

impl System {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn database(&self, id: &str) -> Option<&Database> {
        self.databases.iter().find(|d| d.id == id)
    }

    /// Nodes of `layer`, left to right.
    pub fn nodes_in(&self, layer: &str) -> Vec<&Node> {
        let mut nodes: Vec<&Node> = self.nodes.iter().filter(|n| n.layer == layer).collect();
        nodes.sort_by(|a, b| {
            let (x, y) = (a.slot.unwrap_or(f32::MAX), b.slot.unwrap_or(f32::MAX));
            x.total_cmp(&y)
        });
        nodes
    }

    /// A fresh id from `name` that no node uses yet (`ledger`, `ledger-2`).
    pub fn new_node_id(&self, name: &str) -> String {
        let base: String = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let base = if base.is_empty() {
            "node".to_owned()
        } else {
            base
        };
        let mut id = base.clone();
        let mut n = 2;
        while self.node(&id).is_some() {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }

    /// Removes a node and every connection, API and step that uses it.
    pub fn remove_node(&mut self, id: &str) {
        self.nodes.retain(|n| n.id != id);
        self.edges.retain(|e| e.from != id && e.to != id);
        for flow in &mut self.flows {
            flow.steps.retain(|s| s.from != id && s.to != id);
        }
    }

    /// Adds `from → to`, or removes it when it is already there.
    pub fn toggle_edge(&mut self, from: &str, to: &str, style: EdgeStyle) {
        if let Some(i) = self.edges.iter().position(|e| e.from == from && e.to == to) {
            self.edges.remove(i);
        } else if from != to {
            self.edges.push(Edge {
                from: from.into(),
                to: to.into(),
                style,
                ..Edge::default()
            });
        }
    }

    /// Adds `from → to` between the given sides (replacing an existing one).
    pub fn connect(
        &mut self,
        from: &str,
        to: &str,
        sides: (Option<Side>, Option<Side>),
        style: EdgeStyle,
    ) {
        if from == to {
            return;
        }
        self.edges.retain(|e| !(e.from == from && e.to == to));
        self.edges.push(Edge {
            from: from.into(),
            to: to.into(),
            style,
            from_side: sides.0,
            to_side: sides.1,
            ..Edge::default()
        });
    }

    /// Moves node `id` into `layer` at position `index` (0 = leftmost),
    /// renumbering the layer's slots.
    pub fn place(&mut self, id: &str, layer: &str, index: usize) {
        let Some(pos) = self.nodes.iter().position(|n| n.id == id) else {
            return;
        };
        self.nodes[pos].layer = layer.to_owned();
        let mut order: Vec<String> = self
            .nodes_in(layer)
            .iter()
            .filter(|n| n.id != id)
            .map(|n| n.id.clone())
            .collect();
        order.insert(index.min(order.len()), id.to_owned());
        for (slot, nid) in order.iter().enumerate() {
            if let Some(n) = self.nodes.iter_mut().find(|n| n.id == *nid) {
                n.slot = Some(slot as f32);
            }
        }
    }

    /// Moves layer `from` to position `to`.
    pub fn move_layer(&mut self, from: usize, to: usize) {
        if from < self.layers.len() && to < self.layers.len() && from != to {
            let layer = self.layers.remove(from);
            self.layers.insert(to, layer);
        }
    }

    /// Adds a layer named `name` (at the end) and returns its id.
    pub fn add_layer(&mut self, name: &str, color: u32) -> String {
        let base: String = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let base = if base.trim_matches('-').is_empty() {
            "layer".to_owned()
        } else {
            base
        };
        let mut id = base.clone();
        let mut n = 2;
        while self.layers.iter().any(|l| l.id == id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        self.layers.push(Layer {
            id: id.clone(),
            name: name.to_owned(),
            color,
        });
        id
    }

    /// Removes an empty layer; `false` when components are still on it.
    pub fn remove_layer(&mut self, id: &str) -> bool {
        if self.nodes.iter().any(|n| n.layer == id) {
            return false;
        }
        self.layers.retain(|l| l.id != id);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_moves_and_connects() {
        let mut s = crate::demos::starter("T");
        s.place("db", "services", 0);
        let order: Vec<_> = s
            .nodes_in("services")
            .iter()
            .map(|n| n.id.clone())
            .collect();
        assert_eq!(order, ["db", "api"]);
        let id = s.add_layer("Edge AI", 0x14b8a6);
        assert_eq!(id, "edge-ai");
        s.move_layer(s.layers.len() - 1, 0);
        assert_eq!(s.layers[0].id, "edge-ai");
        assert!(s.remove_layer("edge-ai"));
        assert!(!s.remove_layer("services"));
        s.connect(
            "web",
            "api",
            (Some(Side::Right), Some(Side::Left)),
            EdgeStyle::Socket,
        );
        let e = s
            .edges
            .iter()
            .find(|e| e.from == "web" && e.to == "api")
            .expect("edge");
        assert_eq!(e.from_side, Some(Side::Right));
        // Older files without the new fields still load.
        let old: Edge =
            serde_json::from_str(r#"{"from":"a","to":"b","style":"rpc"}"#).expect("edge");
        assert_eq!(old.color, None);
    }
}
