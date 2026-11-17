//! Generating the diagram from the project's files (docker-compose files,
//! Kubernetes manifests, package manifests; see
//! [`ferrumsystem_core::import`]): a project's first diagram, or what its
//! diagram lacks, added as one edit that undo takes back.

use std::path::PathBuf;

use ferrum_ui::prelude::*;
use ferrumsystem_core::import::{self, Imported};

use crate::app::{FerrumSystem, Source};

impl FerrumSystem {
    /// Reads the open project's files in the background, then applies
    /// what they describe.
    pub fn generate_from_project(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else {
            self.say(Notice::info("Open a project folder first."), cx);
            return;
        };
        let root = project.clone();
        let task = cx
            .background_executor()
            .spawn(async move { import::from_project(&root) });
        cx.spawn(async move |this, cx| {
            let imported = task.await;
            this.update(cx, |this, cx| this.apply_import(&project, imported, cx))
                .ok();
        })
        .detach();
    }

    /// Creates the diagram of `project` from `imported`, or adds to it.
    pub(crate) fn apply_import(
        &mut self,
        project: &PathBuf,
        imported: Option<Imported>,
        cx: &mut Context<Self>,
    ) {
        if self.project.as_ref() != Some(project) {
            return;
        }
        let Some(imported) = imported else {
            self.say(
                Notice::info(
                    "Found nothing to generate from: no docker-compose file, Kubernetes \
                     manifest or package manifest.",
                ),
                cx,
            );
            return;
        };
        let from = imported
            .sources
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        if !self.project_has_system {
            self.create_architecture(Some(imported.system), cx);
            if self.project_has_system {
                self.say(
                    Notice::info(format!("Generated the diagram from {from}")),
                    cx,
                );
            }
            return;
        }
        if !matches!(self.source, Source::Project(_)) {
            self.show_project_system(cx);
        }
        let before = self.system.nodes.len();
        self.edit(cx, |s| import::merge(s, &imported.system));
        let added = self.system.nodes.len() - before;
        let text = if added == 0 {
            format!("The diagram already has every component in {from}")
        } else {
            format!("Added {added} components from {from} (⌘Z to undo)")
        };
        self.say(Notice::info(text), cx);
    }
}

#[cfg(test)]
mod tests {
    use ferrumsystem_core::store;

    use crate::app::{GenerateFromProject, Source};
    use crate::tests::{Fixture, draw, seen};

    const COMPOSE: &str = "services:\n  api:\n    image: node:22\n    depends_on: [db]\n  db:\n    image: postgres:16\n";

    #[gpui::test]
    fn generates_a_new_projects_diagram_and_adds_to_an_existing_one(cx: &mut gpui::TestAppContext) {
        let f = Fixture::new();
        f.set_prefs(&seen());
        let project = f.project("shop", false);
        std::fs::write(project.join("compose.yaml"), COMPOSE).expect("compose");
        let (app, cx) = f.open(cx, Some(project.clone()));
        assert!(
            !store::system_file(&project).exists(),
            "opening writes nothing"
        );

        cx.dispatch_action(GenerateFromProject);
        draw(cx);
        app.read_with(cx, |app, _| {
            assert_eq!(app.source, Source::Project(project.clone()));
            let ids: Vec<_> = app.system.nodes.iter().map(|n| n.id.as_str()).collect();
            assert_eq!(ids, ["api", "db"]);
            assert_eq!(app.system.edges.len(), 1);
        });
        assert!(store::system_file(&project).is_file());

        // Later the compose file gains a cache: only that is added, and
        // undo takes it back.
        let more = format!("{COMPOSE}  cache:\n    image: redis:7\n");
        std::fs::write(project.join("compose.yaml"), more).expect("compose");
        app.update(cx, |app, cx| {
            app.edit(cx, |s| s.nodes[0].name = "Orders API".into())
        });
        cx.dispatch_action(GenerateFromProject);
        draw(cx);
        app.update(cx, |app, cx| {
            assert_eq!(app.system.nodes.len(), 3);
            assert_eq!(app.system.nodes[0].name, "Orders API", "kept");
            app.undo(false, cx);
            assert_eq!(app.system.nodes.len(), 2);
        });
    }
}
