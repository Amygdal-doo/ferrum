//! Compose projects: grouping containers by their compose label and driving
//! `docker compose -p <project>` (the Engine API has no compose endpoint).

use crate::cli::{DockerCli, run_capture};
use crate::endpoint::Endpoint;
use crate::error::EngineResult;
use crate::model::{COMPOSE_CONFIG_FILES, COMPOSE_WORKING_DIR, Container, ContainerState};
use std::collections::BTreeMap;
use std::path::Path;

/// Containers of one compose project, or the standalone ones (`project: None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerGroup {
    pub project: Option<String>,
    pub containers: Vec<Container>,
}

impl ContainerGroup {
    #[must_use]
    pub fn running(&self) -> usize {
        self.containers.iter().filter(|c| c.state.is_up()).count()
    }

    /// Where `docker compose` was run from and with which files (from the labels).
    #[must_use]
    pub fn compose_project(&self) -> Option<ComposeProject> {
        let name = self.project.clone()?;
        let labels = &self.containers.first()?.labels;
        Some(ComposeProject {
            name,
            working_dir: labels.get(COMPOSE_WORKING_DIR).cloned(),
            config_files: labels
                .get(COMPOSE_CONFIG_FILES)
                .map(|files| {
                    files
                        .split(',')
                        .map(str::trim)
                        .filter(|f| !f.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

/// Compose projects sorted by name, then the standalone containers. Inside a
/// group: by service (or name), running ones are not reordered.
#[must_use]
pub fn group_by_project(containers: Vec<Container>) -> Vec<ContainerGroup> {
    let mut projects: BTreeMap<String, Vec<Container>> = BTreeMap::new();
    let mut standalone = Vec::new();
    for container in containers {
        match container.project() {
            Some(project) => projects
                .entry(project.to_owned())
                .or_default()
                .push(container),
            None => standalone.push(container),
        }
    }
    let sort = |list: &mut Vec<Container>| {
        list.sort_by(|a, b| {
            let key = |c: &Container| c.service().unwrap_or(&c.name).to_lowercase();
            key(a).cmp(&key(b)).then_with(|| a.name.cmp(&b.name))
        });
    };
    let mut groups: Vec<ContainerGroup> = projects
        .into_iter()
        .map(|(project, mut containers)| {
            sort(&mut containers);
            ContainerGroup {
                project: Some(project),
                containers,
            }
        })
        .collect();
    if !standalone.is_empty() {
        sort(&mut standalone);
        groups.push(ContainerGroup {
            project: None,
            containers: standalone,
        });
    }
    groups
}

/// Case-insensitive filter on name, image, project, service and short id.
#[must_use]
pub fn matches_filter(container: &Container, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    if filter.is_empty() {
        return true;
    }
    [
        Some(container.name.as_str()),
        Some(container.image.as_str()),
        container.project(),
        container.service(),
        Some(container.short_id()),
        Some(container.state.label()),
    ]
    .into_iter()
    .flatten()
    .any(|field| field.to_lowercase().contains(&filter))
}

/// Whether every container of a group is stopped (so "Up" is the useful action).
#[must_use]
pub fn all_stopped(group: &ContainerGroup) -> bool {
    group
        .containers
        .iter()
        .all(|c| !c.state.is_up() && c.state != ContainerState::Paused)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeProject {
    pub name: String,
    pub working_dir: Option<String>,
    pub config_files: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeAction {
    Up,
    Down,
    Restart,
}

impl ComposeAction {
    #[must_use]
    pub fn verb(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Restart => "restart",
        }
    }
}

/// Arguments after `docker`. The project directory and compose files from the
/// labels are passed only if they all still exist; otherwise compose works
/// from the project name alone (fine for `down`/`restart`; `up` then needs the
/// files and reports that clearly).
#[must_use]
pub fn compose_args(
    project: &ComposeProject,
    action: ComposeAction,
    exists: impl Fn(&Path) -> bool,
) -> Vec<String> {
    let mut args = vec!["compose".to_owned(), "-p".to_owned(), project.name.clone()];
    let dir_ok = project
        .working_dir
        .as_deref()
        .is_some_and(|d| exists(Path::new(d)));
    let files_ok = !project.config_files.is_empty()
        && project.config_files.iter().all(|f| exists(Path::new(f)));
    if dir_ok && files_ok {
        if let Some(dir) = &project.working_dir {
            args.extend(["--project-directory".to_owned(), dir.clone()]);
        }
        for file in &project.config_files {
            args.extend(["-f".to_owned(), file.clone()]);
        }
    }
    match action {
        ComposeAction::Up => args.extend(["up".to_owned(), "-d".to_owned()]),
        ComposeAction::Down => args.push("down".to_owned()),
        ComposeAction::Restart => args.push("restart".to_owned()),
    }
    args
}

/// Runs `docker compose … <action>` against `endpoint`; returns the combined output.
pub async fn run_compose(
    cli: &DockerCli,
    endpoint: &Endpoint,
    project: &ComposeProject,
    action: ComposeAction,
) -> EngineResult<String> {
    let args = compose_args(project, action, Path::exists);
    tracing::info!(project = %project.name, action = action.verb(), "docker compose");
    run_capture(cli, endpoint, &args).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::container;

    #[test]
    fn groups_by_compose_project_with_standalone_last() {
        let groups = group_by_project(vec![
            container("1", "loose", None, "running"),
            container("2", "web-db-1", Some(("web", "db")), "running"),
            container("3", "api-app-1", Some(("api", "app")), "exited"),
            container("4", "web-app-1", Some(("web", "app")), "exited"),
        ]);
        let names: Vec<_> = groups.iter().map(|g| g.project.as_deref()).collect();
        assert_eq!(names, [Some("api"), Some("web"), None]);
        let web: Vec<_> = groups[1]
            .containers
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(web, ["web-app-1", "web-db-1"]);
        assert_eq!(groups[1].running(), 1);
        assert!(all_stopped(&groups[0]));
        assert!(!all_stopped(&groups[1]));
        assert!(group_by_project(Vec::new()).is_empty());
    }

    #[test]
    fn filters_on_several_fields() {
        let mut c = container(
            "abcdef123456789",
            "web-db-1",
            Some(("web", "db")),
            "running",
        );
        c.image = "postgres:16".into();
        for query in ["", "DB", "web", "abcdef", "running", "postgres"] {
            assert!(matches_filter(&c, query), "{query}");
        }
        assert!(!matches_filter(&c, "redis"));
    }

    #[test]
    fn builds_compose_arguments_from_labels() {
        let mut c = container("1", "web-db-1", Some(("web", "db")), "running");
        c.labels
            .insert(COMPOSE_WORKING_DIR.into(), "/src/web".into());
        c.labels.insert(
            COMPOSE_CONFIG_FILES.into(),
            "/src/web/compose.yaml, /src/web/override.yaml".into(),
        );
        let group = ContainerGroup {
            project: Some("web".into()),
            containers: vec![c],
        };
        let project = group
            .compose_project()
            .unwrap_or_else(|| panic!("compose project"));
        assert_eq!(project.config_files.len(), 2);

        let up = compose_args(&project, ComposeAction::Up, |_| true);
        assert_eq!(
            up.join(" "),
            "compose -p web --project-directory /src/web -f /src/web/compose.yaml -f /src/web/override.yaml up -d"
        );
        let missing = |p: &Path| p != Path::new("/src/web/override.yaml");
        assert_eq!(
            compose_args(&project, ComposeAction::Down, missing).join(" "),
            "compose -p web down"
        );
        assert_eq!(
            compose_args(&project, ComposeAction::Restart, |_| false).join(" "),
            "compose -p web restart"
        );
        assert_eq!(
            ContainerGroup {
                project: None,
                containers: Vec::new()
            }
            .compose_project(),
            None
        );
    }
}
