//! The agent's diagram edits: add a component, connect two, rename one,
//! and propose a whole diagram (the "Ask agent to draft" button). Each is
//! a write, so the chat asks the user before it runs. The diagram lives on
//! the UI thread: the tools hand their calls to the window over a channel
//! and the window applies them as ordinary edits — saved into
//! `system.json` and undone with ⌘Z like the user's own.

use std::sync::Arc;

use ferrumagent_ui::core::host::HostTool;
use ferrumagent_ui::core::provider::ToolSpec;
use ferrumagent_ui::core::tools::{Access, ToolDef, ToolOutput};
use ferrumsystem_core::catalog::KINDS;
use ferrumsystem_core::{EdgeStyle, System};
use futures::channel::{mpsc, oneshot};
use serde_json::{Value, json};

/// What a tool asks the window to do.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SystemCall {
    Add {
        name: String,
        kind: String,
        layer: Option<String>,
        tech: Option<String>,
        description: String,
    },
    Connect {
        from: String,
        to: String,
        style: EdgeStyle,
        label: Option<String>,
    },
    Rename {
        component: String,
        name: String,
    },
    /// A whole diagram: the project's first, or added to the one there.
    Propose(Box<System>),
}

/// A call and where its answer goes.
pub(crate) type Request = (SystemCall, oneshot::Sender<Result<String, String>>);

/// The tools' side of the channel to the window.
#[derive(Clone)]
pub(crate) struct Bridge(mpsc::UnboundedSender<Request>);

impl Bridge {
    pub(crate) fn new() -> (Self, mpsc::UnboundedReceiver<Request>) {
        let (tx, rx) = mpsc::unbounded();
        (Self(tx), rx)
    }

    async fn ask(&self, call: SystemCall) -> Result<String, String> {
        let closed = || "The FerrumSystem window is closed.".to_string();
        let (tx, rx) = oneshot::channel();
        self.0.unbounded_send((call, tx)).map_err(|_| closed())?;
        rx.await.map_err(|_| closed())?
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Add,
    Connect,
    Rename,
    Propose,
}

pub(crate) struct EditTool {
    kind: Kind,
    bridge: Bridge,
}

/// The tools, answered by the window behind `bridge`.
pub(crate) fn tools(bridge: &Bridge) -> Vec<Arc<dyn HostTool>> {
    [Kind::Add, Kind::Connect, Kind::Rename, Kind::Propose]
        .into_iter()
        .map(|kind| {
            Arc::new(EditTool {
                kind,
                bridge: bridge.clone(),
            }) as Arc<dyn HostTool>
        })
        .collect()
}

const STYLES: [&str; 5] = ["request", "rpc", "async", "socket", "cdc"];

fn style(name: &str) -> Option<EdgeStyle> {
    STYLES
        .iter()
        .position(|s| s.eq_ignore_ascii_case(name.trim()))
        .map(|i| EdgeStyle::ALL[i])
}

fn text(input: &Value, key: &str) -> Option<String> {
    input[key]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

#[async_trait::async_trait]
impl HostTool for EditTool {
    fn def(&self) -> ToolDef {
        let kinds: Vec<&str> = KINDS.iter().map(|k| k.id).collect();
        let (name, description, schema) = match self.kind {
            Kind::Add => (
                "system_add_component",
                "Add a component to the diagram shown. The user approves it first; it is saved into .architecture/system.json and can be undone.",
                json!({"type": "object", "properties": {
                    "name": {"type": "string"},
                    "kind": {"type": "string", "enum": kinds},
                    "layer": {"type": "string", "description": "Layer id (see system_overview); default: the usual layer for the kind"},
                    "tech": {"type": "string", "description": "Technology id, e.g. rust, postgresql, kafka"},
                    "description": {"type": "string"}
                }, "required": ["name", "kind"]}),
            ),
            Kind::Connect => (
                "system_connect",
                "Connect two components of the diagram shown (by id or name), replacing a connection between them in that direction. The user approves it first.",
                json!({"type": "object", "properties": {
                    "from": {"type": "string"},
                    "to": {"type": "string"},
                    "style": {"type": "string", "enum": STYLES, "description": "request (HTTPS), rpc (gRPC), async (queue / stream), socket, cdc; default request"},
                    "label": {"type": "string"}
                }, "required": ["from", "to"]}),
            ),
            Kind::Rename => (
                "system_rename",
                "Rename a component of the diagram shown (its id stays). The user approves it first.",
                json!({"type": "object", "properties": {
                    "component": {"type": "string", "description": "Id or current name"},
                    "name": {"type": "string"}
                }, "required": ["component", "name"]}),
            ),
            Kind::Propose => (
                "system_propose_diagram",
                "Propose a whole architecture diagram, in the format of system_details part all: name, tagline, overview, layers [{id, name, color}], nodes [{id, name, kind, tech, layer, description}], edges [{from, to, style, label}]. The user sees it and approves it before anything is written; a project without a diagram gets it as its diagram, an existing diagram gains the layers, components and connections it lacks.",
                json!({"type": "object", "properties": {
                    "diagram": {"type": "object"}
                }, "required": ["diagram"]}),
            ),
        };
        ToolDef {
            spec: ToolSpec {
                name: name.into(),
                description: description.into(),
                input_schema: schema,
            },
            access: Access::Write,
        }
    }

    fn title(&self, input: &Value) -> String {
        let s = |k: &str| input[k].as_str().unwrap_or_default().to_owned();
        match self.kind {
            Kind::Add => format!("Add {} ({}) to the diagram", s("name"), s("kind")),
            Kind::Connect => format!("Connect {} → {}", s("from"), s("to")),
            Kind::Rename => format!("Rename {} to {}", s("component"), s("name")),
            Kind::Propose => {
                let d = &input["diagram"];
                let count = |k: &str| d[k].as_array().map_or(0, Vec::len);
                format!(
                    "Draft the diagram: {} components, {} connections",
                    count("nodes"),
                    count("edges")
                )
            }
        }
    }

    async fn call(&self, input: Value) -> ToolOutput {
        let call = match self.parse(&input) {
            Ok(call) => call,
            Err(e) => return ToolOutput::error(e),
        };
        self.bridge
            .ask(call)
            .await
            .map_or_else(ToolOutput::error, ToolOutput::ok)
    }
}

impl EditTool {
    fn parse(&self, input: &Value) -> Result<SystemCall, String> {
        let need = |k: &str| text(input, k).ok_or_else(|| format!("Give {k}."));
        Ok(match self.kind {
            Kind::Add => SystemCall::Add {
                name: need("name")?,
                kind: need("kind")?,
                layer: text(input, "layer"),
                tech: text(input, "tech"),
                description: text(input, "description").unwrap_or_default(),
            },
            Kind::Connect => SystemCall::Connect {
                from: need("from")?,
                to: need("to")?,
                style: match text(input, "style") {
                    Some(s) => style(&s)
                        .ok_or_else(|| format!("Unknown style {s:?}: use one of {STYLES:?}."))?,
                    None => EdgeStyle::Request,
                },
                label: text(input, "label"),
            },
            Kind::Rename => SystemCall::Rename {
                component: need("component")?,
                name: need("name")?,
            },
            Kind::Propose => {
                let system: System = serde_json::from_value(input["diagram"].clone())
                    .map_err(|e| format!("The diagram does not read as a system: {e}"))?;
                if system.nodes.is_empty() {
                    return Err("The diagram has no components.".into());
                }
                SystemCall::Propose(Box::new(system))
            }
        })
    }
}

#[cfg(test)]
mod tests;
