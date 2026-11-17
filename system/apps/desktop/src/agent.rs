//! The Agent pane: FerrumChat beside the diagram, without a project (the
//! diagram is the subject). Before each message it learns what is shown —
//! the system, the selected component, the panel's tab, the flow being
//! walked through — and the read tools (see [`crate::agent_tools`]) get a
//! copy of the diagram. Its edit tools (see [`crate::agent_write`]) ask
//! the user first, then come here and are applied as ordinary edits.

use std::rc::Rc;

use futures::StreamExt as _;

use ferrum_ui::prelude::*;
use ferrumagent_ui::core::host::AppHost;
use ferrumagent_ui::core::session::SessionConfig;
use ferrumagent_ui::{AgentPane, ChatOptions, ChatView};

use crate::agent_tools;
use crate::agent_write::{self, Bridge, SystemCall};
use crate::app::{FerrumSystem, Source};

/// The channel the edit tools call the window over, and the task that
/// answers them for as long as the window lives.
pub(crate) fn listen(cx: &mut Context<FerrumSystem>) -> (Bridge, Task<()>) {
    let (bridge, mut rx) = Bridge::new();
    let task = cx.spawn(async move |this, cx| {
        while let Some((call, reply)) = rx.next().await {
            match this.update(cx, |app, cx| app.agent_call(call, cx)) {
                Ok(answer) => {
                    let _ = reply.send(answer);
                }
                Err(_) => break,
            }
        }
    });
    (bridge, task)
}

const ABOUT: &str = "FerrumSystem draws a system's architecture: layers, components on any \
                     cloud, the connections between them, APIs, data, flows, reliability and \
                     deployment. Help the user understand and improve the design; the \
                     system_* edit tools change the diagram once the user approves.";

impl FerrumSystem {
    /// The top-right Agent toggle: shows the chat (made the first time) or
    /// hides it.
    pub fn toggle_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let slot = self.agent_system.clone();
        let bridge = self.agent_bridge.clone();
        let home = self.agent_home.clone();
        let weak = cx.weak_entity();
        self.agent.toggle(window, cx, move |window, cx| {
            let mut tools = agent_tools::tools(&slot);
            tools.extend(agent_write::tools(&bridge));
            let host = AppHost {
                tools,
                ..AppHost::new("FerrumSystem", ABOUT)
            };
            let context = Rc::new(move |cx: &mut App| {
                weak.update(cx, |app, _| {
                    let shown = (!app.is_empty()).then(|| app.system.clone());
                    app.agent_system.set(shown);
                    app.agent_context()
                })
                .unwrap_or_default()
            });
            let options = ChatOptions {
                config: home.map(|home| SessionConfig::isolated(&home, &home)),
                ..AgentPane::options(Some(context))
            };
            ChatView::without_project(host, options, window, cx)
        });
        cx.notify();
    }

    /// The Agent toggle, before the account.
    pub(crate) fn agent_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        ferrumagent_ui::agent_toggle("toggle-agent", self.agent.is_shown())
            .on_click(cx.listener(|this, _, window, cx| this.toggle_agent(window, cx)))
    }

    /// What the window shows, for the chat.
    pub(crate) fn agent_context(&self) -> String {
        let mut out = match (&self.source, &self.project) {
            (Source::Project(path), _) => format!(
                "Shown: the diagram of the project {} (.architecture/system.json)\n",
                path.display()
            ),
            (Source::Demo(_), _) => format!("Shown: the demo \"{}\"\n", self.system.name),
            (Source::Empty, Some(path)) => format!(
                "Shown: nothing; the project {} has no diagram yet\n",
                path.display()
            ),
            (Source::Empty, None) => "Shown: nothing; no project is open\n".to_string(),
        };
        if let Some(brief) = &self.draft_brief {
            out.push_str(brief);
        }
        if self.is_empty() {
            return out;
        }
        let system = &self.system;
        out.push_str(&format!(
            "System: {} — {} ({} components in {} layers, {} connections)\n",
            system.name,
            system.tagline,
            system.nodes.len(),
            system.layers.len(),
            system.edges.len()
        ));
        if let Some(node) = self.selected.as_deref().and_then(|id| system.node(id)) {
            out.push_str(&format!(
                "Selected component: {} [{}], kind {}, layer {}",
                node.name, node.id, node.kind, node.layer
            ));
            if let Some(tech) = &node.tech {
                out.push_str(&format!(", built with {tech}"));
            }
            out.push('\n');
            if !node.description.trim().is_empty() {
                out.push_str(&format!("  {}\n", node.description.trim()));
            }
        }
        if self.panel_open() {
            out.push_str(&format!("Panel tab: {}\n", self.tab.label()));
        }
        if let Some((flow, step)) = self.walk
            && let Some(flow) = system.flows.get(flow)
        {
            out.push_str(&format!(
                "Walking through the flow \"{}\", step {} of {}",
                flow.name,
                step + 1,
                flow.steps.len()
            ));
            if let Some(s) = flow.steps.get(step) {
                out.push_str(&format!(": {} → {}: {}", s.from, s.to, s.text));
            }
            out.push('\n');
        }
        out
    }
}

impl FerrumSystem {
    /// Applies a call of the agent's edit tools (the user has approved it)
    /// through [`Self::edit`]: saved, and undone with ⌘Z.
    pub(crate) fn agent_call(
        &mut self,
        call: SystemCall,
        cx: &mut Context<Self>,
    ) -> Result<String, String> {
        let result = match call {
            SystemCall::Propose(draft) => self.apply_draft(*draft, cx),
            _ if self.is_empty() => Err("No diagram is open in FerrumSystem.".into()),
            SystemCall::Add {
                name,
                kind,
                layer,
                tech,
                description,
            } => self.agent_add(name, kind, layer, tech, description, cx),
            SystemCall::Connect {
                from,
                to,
                style,
                label,
            } => {
                let (from, to) = (self.find(&from)?, self.find(&to)?);
                if from == to {
                    return Err("A component cannot connect to itself.".into());
                }
                self.edit(cx, |s| {
                    s.connect(&from, &to, (None, None), style);
                    if let Some(e) = s.edges.iter_mut().find(|e| e.from == from && e.to == to) {
                        e.label = label;
                    }
                });
                Ok(format!("Connected {from} → {to} ({}).", style.label()))
            }
            SystemCall::Rename { component, name } => {
                let id = self.find(&component)?;
                self.fields = None;
                self.edit(cx, |s| {
                    if let Some(n) = s.nodes.iter_mut().find(|n| n.id == id) {
                        n.name = name.clone();
                    }
                });
                Ok(format!("Renamed {id} to {name}."))
            }
        };
        // The read tools see the edit at once.
        self.agent_system
            .set((!self.is_empty()).then(|| self.system.clone()));
        result
    }

    /// The id of the component called `wanted` (id or name).
    fn find(&self, wanted: &str) -> Result<String, String> {
        let s = &self.system;
        s.node(wanted)
            .or_else(|| s.nodes.iter().find(|n| n.name.eq_ignore_ascii_case(wanted)))
            .map(|n| n.id.clone())
            .ok_or_else(|| format!("No component is called {wanted}."))
    }

    fn agent_add(
        &mut self,
        name: String,
        kind: String,
        layer: Option<String>,
        tech: Option<String>,
        description: String,
        cx: &mut Context<Self>,
    ) -> Result<String, String> {
        let found = ferrumsystem_core::Kind::find(&kind)
            .ok_or_else(|| format!("Unknown kind {kind:?}: see the tool's list."))?;
        let layers = &self.system.layers;
        let layer = match layer {
            Some(l) if layers.iter().any(|x| x.id == l) => l,
            Some(l) => return Err(format!("No layer {l:?}: see system_overview.")),
            None => {
                let usual = crate::dialogs::default_layer(found.category);
                layers
                    .iter()
                    .find(|l| l.id == usual)
                    .or(layers.first())
                    .map(|l| l.id.clone())
                    .ok_or("The diagram has no layers.")?
            }
        };
        let tech = tech.filter(|t| crate::icons::tech_info(&self.system, t).is_some());
        let id = self.system.new_node_id(&name);
        let node = ferrumsystem_core::Node {
            id: id.clone(),
            name: name.clone(),
            kind,
            tech,
            slot: Some(self.system.nodes_in(&layer).len() as f32),
            layer: layer.clone(),
            description,
            ..Default::default()
        };
        self.edit(cx, |s| s.nodes.push(node));
        Ok(format!("Added {name} [{id}] to layer {layer}."))
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::{Fixture, draw};

    #[gpui::test]
    fn the_agent_toggle_opens_the_chat_beside_the_diagram(cx: &mut gpui::TestAppContext) {
        let f = Fixture::new();
        let project = f.project("shop", true);
        let (app, cx) = f.open(cx, Some(project.clone()));
        app.update(cx, |app, cx| {
            let id = app.system.nodes[0].id.clone();
            app.select(Some(id), cx);
        });
        app.update_in(cx, |app, window, cx| app.toggle_agent(window, cx));
        draw(cx);
        let chat = app.read_with(cx, |app, _| {
            assert!(app.agent.is_shown());
            app.agent.chat().cloned().expect("chat")
        });
        cx.run_until_parked();
        // No project: the chat opens at once, without the "Opening
        // project" spinner.
        chat.read_with(cx, |chat, _| {
            assert!(!chat.has_project());
            assert!(chat.session().is_some());
        });

        let context = app.read_with(cx, |app, _| app.agent_context());
        assert!(
            context.contains(&format!("project {}", project.display())),
            "{context}"
        );
        assert!(context.contains("System: shop"), "{context}");
        assert!(context.contains("Selected component:"), "{context}");
        assert!(context.contains("Panel tab:"), "{context}");

        app.update_in(cx, |app, window, cx| app.toggle_agent(window, cx));
        draw(cx);
        app.read_with(cx, |app, _| assert!(!app.agent.is_shown()));
    }
}
