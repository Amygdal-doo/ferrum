//! The shared Ferrum project bar above the toolbar: the active repository's
//! work tree, its branch, and the recent-projects switcher.

use crate::app::GitView;
use crate::state::AppState;
use ferrum_ui::{ProjectBar, ProjectBarEvent};
use ferrumgit_domain::RepoId;
use gpui::{AppContext as _, Context, Entity, Subscription, Window};
use std::path::{Path, PathBuf};

/// The executable name recorded in `~/.ferrum/projects.json`.
const APP_BINARY: &str = "ferrumgit";

/// The bar and the subscription that turns its picks into open repositories.
pub fn new_bar(
    window: &mut Window,
    cx: &mut Context<GitView>,
) -> (Entity<ProjectBar>, Subscription) {
    let bar = cx.new(|cx| ProjectBar::new(APP_BINARY, window, cx));
    let subscription = cx.subscribe(&bar, |app: &mut GitView, _, event, cx| match event {
        ProjectBarEvent::Open(path) => app.open_project(path.clone(), cx),
    });
    (bar, subscription)
}

/// The folder the bar shows: the active repository's work tree.
#[must_use]
pub fn bar_project(state: &AppState) -> Option<PathBuf> {
    state.active_entry().map(|entry| PathBuf::from(entry.path))
}

/// The notice for a folder that is not a repository: points at the home
/// screen's "Init" (the only way this app creates repositories).
#[must_use]
pub fn not_a_repo_notice(path: &Path, error: &str) -> String {
    if error.contains("not a git repository") {
        format!(
            "{} is not a Git repository. Use Init on the Repositories screen to create one.",
            path.display()
        )
    } else {
        error.to_owned()
    }
}

impl GitView {
    /// Keeps the bar on the active repository (called on every render;
    /// `set_project` ignores an unchanged folder).
    pub(crate) fn sync_project_bar(&mut self, cx: &mut Context<Self>) {
        let project = bar_project(&self.state);
        if self.project_bar.read(cx).project() != project.as_deref() {
            self.project_bar
                .update(cx, |bar, cx| bar.set_project(project, cx));
        }
    }

    /// Re-reads the branch after repository `id` reloaded (checkout, commit,
    /// pull, external changes found by polling).
    pub(crate) fn refresh_project_branch(&mut self, id: RepoId, cx: &mut Context<Self>) {
        if self.state.active == Some(id) {
            self.project_bar
                .update(cx, |bar, cx| bar.refresh_branch(cx));
        }
    }

    /// Opens a folder picked in the bar like "Open" on the home screen: a
    /// known repository's tab is focused, a new one gets a tab.
    pub fn open_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let services = self.services.clone();
        let p = path.display().to_string();
        self.spawn_run(
            cx,
            async move { services.add_repo(&p).await },
            move |app, result, cx| match result {
                Ok(entry) => app.upsert_and_open(entry, cx),
                Err(e) => app.notify(not_a_repo_notice(&path, &e), true, cx),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumgit_domain::RepoEntry;

    #[test]
    fn the_bar_follows_the_active_repository() {
        let mut state = AppState::default();
        assert_eq!(bar_project(&state), None);
        let app = RepoEntry::new("/code/app");
        let lib = RepoEntry::new("/code/lib");
        state.repos = vec![app.clone(), lib.clone()];
        state.open = vec![app.id, lib.id];
        state.active = Some(lib.id);
        assert_eq!(bar_project(&state), Some(PathBuf::from("/code/lib")));
        state.active = Some(app.id);
        assert_eq!(bar_project(&state), Some(PathBuf::from("/code/app")));
        // Home screen: no project.
        state.active = None;
        assert_eq!(bar_project(&state), None);
    }

    #[test]
    fn a_folder_without_git_points_at_init() {
        let path = Path::new("/tmp/notes");
        let message = not_a_repo_notice(path, "not a git repository: /tmp/notes");
        assert!(message.starts_with("/tmp/notes is not a Git repository"));
        assert!(message.contains("Init"));
        assert_eq!(not_a_repo_notice(path, "git not found"), "git not found");
    }
}
