//! Project membership of collections and their files in the repository
//! (`<project>/.ferrum/api/`).

use crate::app::FerrumApi;
use ferrumapi_domain::{Collection, CollectionId};
use gpui::Context;

impl FerrumApi {
    /// Assigns a collection to a project folder, or makes it shared (`None`).
    pub fn move_collection(
        &mut self,
        c: CollectionId,
        project: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.mutate_collection(c, cx, move |col| col.project = project);
    }

    /// Writes the current project's collections to `<project>/.ferrum/api/`.
    pub fn export_project_to_repo(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.state.project.clone() else {
            return;
        };
        let collections: Vec<Collection> = self
            .state
            .collections
            .iter()
            .filter(|c| c.project.as_deref() == Some(project.as_str()))
            .cloned()
            .collect();
        if collections.is_empty() {
            self.notify("This project has no collections yet.", true, cx);
            return;
        }
        let svc = self.services.clone();
        let path = std::path::PathBuf::from(&project);
        self.io(
            cx,
            async move { svc.export_project_collections(path, collections).await },
            |this, result, cx| match result {
                Ok(files) => this.notify(
                    format!("Saved {} collection file(s) to .ferrum/api (secret variable values left out).", files.len()),
                    false,
                    cx,
                ),
                Err(e) => this.notify(format!("Could not save to the repository: {e}"), true, cx),
            },
        );
    }

    /// Loads `<project>/.ferrum/api/*.json` into the current project.
    pub fn import_project_from_repo(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.state.project.clone() else {
            return;
        };
        let svc = self.services.clone();
        let path = std::path::PathBuf::from(&project);
        self.io(
            cx,
            async move { svc.import_project_collections(path).await },
            |this, result, cx| match result {
                Ok((imported, skipped)) => {
                    let n = imported.len();
                    for c in imported {
                        this.state.collections.retain(|x| x.id != c.id);
                        this.state.collections.push(c);
                    }
                    this.state
                        .collections
                        .sort_by_key(|c| c.name.to_lowercase());
                    let mut message = format!("Loaded {n} collection(s) from .ferrum/api.");
                    if !skipped.is_empty() {
                        message.push_str(&format!(" Skipped unreadable: {}.", skipped.join(", ")));
                    }
                    this.notify(message, !skipped.is_empty(), cx);
                }
                Err(e) => this.notify(format!("Could not load from the repository: {e}"), true, cx),
            },
        );
    }
}
