//! The folder open in the Ferrum project bar: its compose files and which
//! containers belong to it. Plain data, tested without a window.

use ferrumdock_engine::Container;
use ferrumdock_engine::project::{self, ComposeFile, normalize_project_name};
use std::path::PathBuf;

pub struct ProjectState {
    pub root: PathBuf,
    pub files: Vec<ComposeFile>,
    /// List only this project's containers ("This project" toggle).
    pub only: bool,
}

impl ProjectState {
    /// Reads the compose files of `root` (blocking: a few small files).
    pub fn load(root: PathBuf) -> Self {
        let files = project::load_compose_files(&root);
        Self::with_files(root, files)
    }

    pub fn with_files(root: PathBuf, files: Vec<ComposeFile>) -> Self {
        Self {
            root,
            files,
            only: true,
        }
    }

    /// Compose project names that count as this folder: each file's project
    /// and the folder name as compose would normalize it.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.files.iter().map(|f| f.project.clone()).collect();
        if let Some(folder) = self.root.file_name() {
            let folder = normalize_project_name(&folder.to_string_lossy());
            if !folder.is_empty() && !names.contains(&folder) {
                names.push(folder);
            }
        }
        names
    }

    pub fn contains(&self, container: &Container) -> bool {
        project::belongs_to(container, &self.root, &self.names())
    }

    /// Compose files whose project has no container yet: "Up" creates them.
    /// Only the first file per project name (compose's preferred one).
    pub fn idle_files(&self, containers: &[Container]) -> Vec<ComposeFile> {
        let mut seen: Vec<&str> = Vec::new();
        self.files
            .iter()
            .filter(|f| {
                let first = !seen.contains(&f.project.as_str());
                seen.push(&f.project);
                first
            })
            .filter(|f| {
                !containers
                    .iter()
                    .any(|c| c.project() == Some(f.project.as_str()))
            })
            .cloned()
            .collect()
    }

    /// How many of `containers` belong to the project.
    pub fn count(&self, containers: &[Container]) -> usize {
        let names = self.names();
        containers
            .iter()
            .filter(|c| project::belongs_to(c, &self.root, &names))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Row, container_rows};
    use ferrumdock_engine::fake::container;
    use ferrumdock_engine::project::compose_file;
    use std::collections::HashSet;
    use std::path::Path;

    fn shop() -> ProjectState {
        let root = PathBuf::from("/src/shop");
        let yaml = "services:\n  web: {}\n  db: {}\n";
        let worker = "name: jobs\nservices:\n  worker: {}\n";
        ProjectState::with_files(
            root.clone(),
            vec![
                compose_file(&root, root.join("compose.yaml"), Ok(yaml.into())),
                compose_file(&root, root.join("docker-compose.yml"), Ok(worker.into())),
            ],
        )
    }

    fn containers() -> Vec<Container> {
        vec![
            container("1", "shop-web-1", Some(("shop", "web")), "running"),
            container("2", "other-x-1", Some(("other", "x")), "running"),
            container("3", "loose", None, "exited"),
        ]
    }

    fn names(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                Row::Group { project, .. } => format!("[{project}]"),
                Row::Container { container, .. } => container.name.clone(),
                Row::ComposeFile(f) => format!("<{}>", f.project),
            })
            .collect()
    }

    #[test]
    fn knows_its_compose_projects_and_idle_files() {
        let project = shop();
        assert_eq!(project.names(), ["shop", "jobs"]);
        let idle = project.idle_files(&containers());
        assert_eq!(idle.len(), 1, "shop already has containers");
        assert_eq!(idle[0].project, "jobs");
        assert_eq!(idle[0].path, Path::new("/src/shop/docker-compose.yml"));
        assert_eq!(project.count(&containers()), 1);
    }

    #[test]
    fn scopes_the_container_list_to_the_project() {
        let mut project = shop();
        let none = HashSet::new();
        assert_eq!(
            names(&container_rows(&containers(), "", &none, Some(&project))),
            ["<jobs>", "[shop]", "shop-web-1"]
        );
        project.only = false;
        assert_eq!(
            names(&container_rows(&containers(), "", &none, Some(&project))),
            [
                "<jobs>",
                "[other]",
                "other-x-1",
                "[shop]",
                "shop-web-1",
                "loose"
            ]
        );
        // The filter applies to idle files too (project or service names).
        assert_eq!(
            names(&container_rows(
                &containers(),
                "worker",
                &none,
                Some(&project)
            )),
            ["<jobs>"]
        );
    }
}
