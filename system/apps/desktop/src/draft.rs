//! "Ask agent to draft the architecture": the Agent pane opens with the
//! request in its composer, and while it drafts the chat's context
//! carries what the project's files say (the chat has no file tools).
//! The agent answers with `system_propose_diagram`, which the user
//! approves before anything is written (see [`crate::agent_write`]).

use std::path::Path;

use ferrum_ui::prelude::*;
use ferrumsystem_core::import;
use ferrumsystem_core::{System, demos};

use crate::app::{FerrumSystem, Source};

/// What the composer asks.
pub const REQUEST: &str = "Draft the architecture of this project from its files (in your \
                           context): layers, components with their kinds and technologies, and \
                           the connections between them, with a short overview. Propose it with \
                           system_propose_diagram.";
/// How much of each file the brief quotes.
const QUOTE: usize = 4000;
const MAX_FILES: usize = 8;

/// What the project's files say: the files the importer read (quoted)
/// and what it made of them, else the folder's top level and README.
pub fn brief(project: &Path) -> String {
    let mut out = format!(
        "Drafting the architecture of {} (FerrumSystem read these files for you):\n",
        project.display()
    );
    let quote = |out: &mut String, rel: &Path| {
        if let Ok(text) = std::fs::read_to_string(project.join(rel)) {
            let cut: String = text.chars().take(QUOTE).collect();
            out.push_str(&format!("--- {} ---\n{cut}\n", rel.display()));
        }
    };
    match import::from_project(project) {
        Some(imported) => {
            for rel in imported.sources.iter().take(MAX_FILES) {
                quote(&mut out, rel);
            }
            out.push_str("What FerrumSystem's importer made of them:\n");
            out.push_str(&crate::agent_tools::overview(&imported.system));
        }
        None => {
            let mut names: Vec<String> = std::fs::read_dir(project)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.starts_with('.'))
                .collect();
            names.sort();
            out.push_str(&format!("Top level: {}\n", names.join(", ")));
            quote(&mut out, Path::new("README.md"));
        }
    }
    out
}

/// Makes a proposed diagram whole: layers when it has none, ids, every
/// component on a layer that exists, slots in order, and no connection
/// to a missing component.
pub fn tidy(draft: &mut System) {
    if draft.layers.is_empty() {
        draft.layers = demos::starter("").layers;
    }
    for i in 0..draft.nodes.len() {
        if draft.nodes[i].id.trim().is_empty() {
            draft.nodes[i].id = draft.new_node_id(&draft.nodes[i].name);
        }
        if !draft.layers.iter().any(|l| l.id == draft.nodes[i].layer) {
            let usual = ferrumsystem_core::Kind::find(&draft.nodes[i].kind)
                .map(|k| crate::dialogs::default_layer(k.category))
                .filter(|l| draft.layers.iter().any(|x| x.id == *l))
                .map(str::to_owned);
            draft.nodes[i].layer = usual.unwrap_or_else(|| draft.layers[0].id.clone());
        }
    }
    for layer in draft.layers.clone() {
        for (slot, id) in draft
            .nodes_in(&layer.id)
            .iter()
            .map(|n| n.id.clone())
            .collect::<Vec<_>>()
            .into_iter()
            .enumerate()
        {
            if let Some(n) = draft.nodes.iter_mut().find(|n| n.id == id) {
                n.slot = Some(slot as f32);
            }
        }
    }
    let ids: Vec<String> = draft.nodes.iter().map(|n| n.id.clone()).collect();
    draft
        .edges
        .retain(|e| ids.contains(&e.from) && ids.contains(&e.to));
}

impl FerrumSystem {
    /// Opens the Agent pane with the draft request, once the project's
    /// files are read.
    pub fn ask_agent_to_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else {
            self.say(Notice::info("Open a project folder first."), cx);
            return;
        };
        let root = project.clone();
        let task = cx.background_executor().spawn(async move { brief(&root) });
        cx.spawn_in(window, async move |this, cx| {
            let brief = task.await;
            this.update_in(cx, |this, window, cx| {
                if this.project.as_ref() != Some(&project) {
                    return;
                }
                this.draft_brief = Some(brief);
                if !this.agent.is_shown() {
                    this.toggle_agent(window, cx);
                }
                if let Some(chat) = this.agent.chat().cloned() {
                    chat.update(cx, |chat, cx| chat.set_draft(REQUEST, window, cx));
                }
            })
            .ok();
        })
        .detach();
    }

    /// The agent's proposed diagram, approved by the user: the project's
    /// first diagram, or what the one shown lacks (one edit, ⌘Z undoes).
    pub(crate) fn apply_draft(
        &mut self,
        mut draft: System,
        cx: &mut Context<Self>,
    ) -> Result<String, String> {
        tidy(&mut draft);
        let (nodes, edges) = (draft.nodes.len(), draft.edges.len());
        if self.project.is_some() && !self.project_has_system {
            self.create_architecture(Some(draft), cx);
            if !self.project_has_system {
                return Err("Could not create the project's .architecture folder.".into());
            }
            self.draft_brief = None;
            return Ok(format!(
                "Created the project's diagram: {nodes} components, {edges} connections."
            ));
        }
        if self.is_empty() {
            return Err("Open a project folder first.".into());
        }
        if self.project_has_system && !matches!(self.source, Source::Project(_)) {
            self.show_project_system(cx);
        }
        let before = (self.system.nodes.len(), self.system.edges.len());
        self.edit(cx, |s| import::merge(s, &draft));
        self.draft_brief = None;
        Ok(format!(
            "Added {} components and {} connections to the diagram (⌘Z undoes).",
            self.system.nodes.len() - before.0,
            self.system.edges.len() - before.1
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draft_is_made_whole() {
        let mut draft: System = serde_json::from_value(serde_json::json!({
            "name": "Shop",
            "nodes": [
                {"name": "Orders API", "kind": "service", "layer": "nowhere"},
                {"id": "db", "name": "DB", "kind": "sql-db"}
            ],
            "edges": [{"from": "orders-api", "to": "db"}, {"from": "db", "to": "ghost"}]
        }))
        .expect("draft");
        tidy(&mut draft);
        let placed: Vec<_> = draft
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.layer.as_str(), n.slot))
            .collect();
        assert_eq!(
            placed,
            [
                ("orders-api", "services", Some(0.)),
                ("db", "data", Some(0.))
            ]
        );
        assert_eq!(draft.edges.len(), 1);
    }

    #[gpui::test]
    fn asking_for_a_draft_opens_the_agent_with_the_projects_files(cx: &mut gpui::TestAppContext) {
        use crate::tests::{Fixture, draw, seen};
        let f = Fixture::new();
        f.set_prefs(&seen());
        let project = f.project("bare", false);
        std::fs::write(project.join("go.mod"), "module shop\n").expect("go.mod");
        let (app, cx) = f.open(cx, Some(project.clone()));
        cx.dispatch_action(crate::app::AskAgentToDraft);
        draw(cx);
        app.read_with(cx, |app, _| {
            assert!(app.agent.is_shown());
            let context = app.agent_context();
            assert!(context.contains("--- go.mod ---"), "{context}");
        });
        assert!(
            !ferrumsystem_core::store::system_file(&project).exists(),
            "nothing is written before the user approves a proposal"
        );
    }

    #[test]
    fn the_brief_quotes_the_files_the_importer_read() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("compose.yaml"),
            "services:\n  api: { image: node:22 }\n",
        )
        .expect("compose");
        let text = brief(dir.path());
        assert!(text.contains("--- compose.yaml ---\nservices:"), "{text}");
        assert!(text.contains("[api]"), "{text}");
        std::fs::remove_file(dir.path().join("compose.yaml")).expect("rm");
        std::fs::write(dir.path().join("README.md"), "# Shop\nSells things.").expect("readme");
        let text = brief(dir.path());
        assert!(
            text.contains("Top level: README.md") && text.contains("Sells things."),
            "{text}"
        );
    }
}
