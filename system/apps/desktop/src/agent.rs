//! The Agent pane: FerrumChat beside the diagram, without a project (the
//! diagram is the subject; the agent does not edit it). Before each
//! message it learns what is shown — the system, the selected component,
//! the panel's tab, the flow being walked through — and the diagram tools
//! (see [`crate::agent_tools`]) get a copy of the diagram.

use std::rc::Rc;

use ferrum_ui::prelude::*;
use ferrumagent_ui::core::host::AppHost;
use ferrumagent_ui::core::session::SessionConfig;
use ferrumagent_ui::{AgentPane, ChatOptions, ChatView};

use crate::agent_tools;
use crate::app::{FerrumSystem, Source};

const ABOUT: &str = "FerrumSystem draws a system's architecture: layers, components on any \
                     cloud, the connections between them, APIs, data, flows, reliability and \
                     deployment. Help the user understand and improve the design.";

impl FerrumSystem {
    /// The top-right Agent toggle: shows the chat (made the first time) or
    /// hides it.
    pub fn toggle_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let slot = self.agent_system.clone();
        let home = self.agent_home.clone();
        let weak = cx.weak_entity();
        self.agent.toggle(window, cx, move |window, cx| {
            let host = AppHost {
                tools: agent_tools::tools(&slot),
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
