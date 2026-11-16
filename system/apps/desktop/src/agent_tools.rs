//! The agent's read-only diagram tools: an overview of the system shown
//! (layers, components, connections), one component in full, and one
//! part of the design (APIs, data, flows…) as JSON. They read a copy of
//! the diagram the window refreshes before each message.

use std::sync::{Arc, Mutex, PoisonError};

use ferrumagent_ui::core::host::HostTool;
use ferrumagent_ui::core::provider::ToolSpec;
use ferrumagent_ui::core::tools::{Access, ToolDef, ToolOutput};
use ferrumsystem_core::System;
use serde_json::{Value, json};

/// The parts `system_details` returns.
const PARTS: [&str; 8] = [
    "requirements",
    "apis",
    "sockets",
    "databases",
    "flows",
    "reliability",
    "deploy",
    "all",
];

/// The diagram the tools read: set by the window before each message
/// (`None`: nothing is shown).
#[derive(Clone, Default)]
pub struct SystemSlot(Arc<Mutex<Option<System>>>);

impl SystemSlot {
    pub fn set(&self, system: Option<System>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = system;
    }

    fn read<T>(&self, f: impl FnOnce(&System) -> Result<T, String>) -> Result<T, String> {
        match &*self.0.lock().unwrap_or_else(PoisonError::into_inner) {
            Some(system) => f(system),
            None => Err("No diagram is open in FerrumSystem.".into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Overview,
    Component,
    Details,
}

pub struct SystemTool {
    kind: Kind,
    slot: SystemSlot,
}

/// The tools, all on `slot`.
pub fn tools(slot: &SystemSlot) -> Vec<Arc<dyn HostTool>> {
    [Kind::Overview, Kind::Component, Kind::Details]
        .into_iter()
        .map(|kind| {
            Arc::new(SystemTool {
                kind,
                slot: slot.clone(),
            }) as Arc<dyn HostTool>
        })
        .collect()
}

#[async_trait::async_trait]
impl HostTool for SystemTool {
    fn def(&self) -> ToolDef {
        let (name, description, schema) = match self.kind {
            Kind::Overview => (
                "system_overview",
                "The diagram shown: what the system does, its layers, components (id, kind, technology) and the connections between them.",
                json!({"type": "object", "properties": {}}),
            ),
            Kind::Component => (
                "system_component",
                "One component in full (description, items, deployment, database, its connections and APIs), by id or name.",
                json!({"type": "object", "properties": {
                    "component": {"type": "string"}
                }, "required": ["component"]}),
            ),
            Kind::Details => (
                "system_details",
                "One part of the design as JSON: requirements, apis, sockets, databases (tables and columns), flows, reliability, deploy, or all.",
                json!({"type": "object", "properties": {
                    "part": {"type": "string", "enum": PARTS}
                }, "required": ["part"]}),
            ),
        };
        ToolDef {
            spec: ToolSpec {
                name: name.into(),
                description: description.into(),
                input_schema: schema,
            },
            access: Access::Read,
        }
    }

    fn title(&self, input: &Value) -> String {
        match self.kind {
            Kind::Overview => "System overview".into(),
            Kind::Component => format!(
                "Component {}",
                input["component"].as_str().unwrap_or_default()
            ),
            Kind::Details => format!("System {}", input["part"].as_str().unwrap_or_default()),
        }
    }

    async fn call(&self, input: Value) -> ToolOutput {
        let result = self.slot.read(|system| match self.kind {
            Kind::Overview => Ok(overview(system)),
            Kind::Component => component(system, input["component"].as_str().unwrap_or_default()),
            Kind::Details => details(system, input["part"].as_str().unwrap_or_default()),
        });
        result.map_or_else(ToolOutput::error, ToolOutput::ok)
    }
}

/// Name, overview, layers with their components, connections.
pub fn overview(system: &System) -> String {
    let mut out = format!("{} — {}\n", system.name, system.tagline);
    if !system.overview.trim().is_empty() {
        out.push_str(&format!("{}\n", system.overview.trim()));
    }
    out.push_str(&format!("Cloud: {}\n", system.provider.label()));
    for layer in &system.layers {
        out.push_str(&format!("\nLayer {} ({}):\n", layer.name, layer.id));
        for node in system.nodes_in(&layer.id) {
            out.push_str(&format!("- {} [{}] kind {}", node.name, node.id, node.kind));
            if let Some(tech) = &node.tech {
                out.push_str(&format!(", built with {tech}"));
            }
            out.push('\n');
        }
    }
    if !system.edges.is_empty() {
        out.push_str("\nConnections:\n");
        for edge in &system.edges {
            out.push_str(&format!(
                "- {} → {}: {}",
                edge.from,
                edge.to,
                edge.style.label()
            ));
            if let Some(label) = &edge.label {
                out.push_str(&format!(" ({label})"));
            }
            out.push('\n');
        }
    }
    out
}

fn component(system: &System, wanted: &str) -> Result<String, String> {
    let wanted = wanted.trim();
    let node = system
        .node(wanted)
        .or_else(|| {
            system
                .nodes
                .iter()
                .find(|n| n.name.eq_ignore_ascii_case(wanted))
        })
        .ok_or_else(|| format!("No component is called {wanted}."))?;
    let edges: Vec<_> = system
        .edges
        .iter()
        .filter(|e| e.from == node.id || e.to == node.id)
        .collect();
    let apis: Vec<_> = system
        .apis
        .iter()
        .filter(|a| a.service == node.id)
        .collect();
    let database = node.database.as_deref().and_then(|id| system.database(id));
    let value = json!({
        "component": node,
        "connections": edges,
        "apis": apis,
        "database": database,
    });
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

fn details(system: &System, part: &str) -> Result<String, String> {
    let value = match part {
        "requirements" => json!({
            "concepts": system.concepts,
            "functional": system.functional,
            "non_functional": system.non_functional,
        }),
        "apis" => json!(system.apis),
        "sockets" => json!(system.sockets),
        "databases" => json!(system.databases),
        "flows" => json!(system.flows),
        "reliability" => json!(system.reliability),
        "deploy" => json!({
            "cluster": system.cluster,
            "components": system.nodes.iter()
                .map(|n| json!({"id": n.id, "deploy": n.deploy}))
                .collect::<Vec<_>>(),
        }),
        "all" => json!(system),
        other => return Err(format!("Unknown part {other:?}: use one of {PARTS:?}.")),
    };
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumsystem_core::demos;

    fn run(tools: &[Arc<dyn HostTool>], name: &str, input: Value) -> ToolOutput {
        let tool = tools
            .iter()
            .find(|t| t.def().spec.name == name)
            .expect("tool");
        futures::executor::block_on(tool.call(input))
    }

    #[test]
    fn every_tool_is_read_only_and_reads_the_shown_diagram() {
        let slot = SystemSlot::default();
        let tools = tools(&slot);
        assert!(tools.iter().all(|t| t.def().access == Access::Read));
        let out = run(&tools, "system_overview", json!({}));
        assert!(out.is_error && out.text.contains("No diagram"));

        let system = demos::all().remove(0);
        let node = system.nodes[0].clone();
        slot.set(Some(system.clone()));
        let out = run(&tools, "system_overview", json!({}));
        assert!(out.text.starts_with(&system.name), "{}", out.text);
        assert!(out.text.contains(&format!("[{}]", node.id)));

        let out = run(&tools, "system_component", json!({"component": node.name}));
        assert!(!out.is_error, "{}", out.text);
        assert!(out.text.contains(&format!("\"id\": \"{}\"", node.id)));

        let out = run(&tools, "system_details", json!({"part": "databases"}));
        assert!(!out.is_error);
        let out = run(&tools, "system_details", json!({"part": "secrets"}));
        assert!(out.is_error);
    }
}
