//! The open project folder (from the Ferrum project bar): its compose files
//! and which containers belong to it.

use crate::compose::ComposeProject;
use crate::model::{COMPOSE_CONFIG_FILES, COMPOSE_WORKING_DIR, Container};
use std::path::{Path, PathBuf};

/// The names `docker compose` looks for, in its order of preference.
pub const COMPOSE_FILE_NAMES: [&str; 4] = [
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

/// A compose file found in the project root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeFile {
    pub path: PathBuf,
    /// Project name: the file's top-level `name:`, else the folder name
    /// normalized like compose does.
    pub project: String,
    /// Service names in file order (empty when the file did not parse).
    pub services: Vec<String>,
    /// Why the file could not be read, if it could not.
    pub error: Option<String>,
}

impl ComposeFile {
    /// What `docker compose up` needs to run this file.
    #[must_use]
    pub fn compose_project(&self, root: &Path) -> ComposeProject {
        ComposeProject {
            name: self.project.clone(),
            working_dir: Some(root.display().to_string()),
            config_files: vec![self.path.display().to_string()],
        }
    }
}

/// Compose files present in `root` (no search upwards or in subfolders).
#[must_use]
pub fn find_compose_files(root: &Path, exists: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    COMPOSE_FILE_NAMES
        .iter()
        .map(|name| root.join(name))
        .filter(|p| exists(p))
        .collect()
}

/// compose's default project name: lowercase, only `[a-z0-9_-]`, starting
/// with a letter or digit.
#[must_use]
pub fn normalize_project_name(name: &str) -> String {
    let cleaned: String = name
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_' || *c == '-')
        .collect();
    cleaned
        .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
        .to_owned()
}

/// Reads `name:` and the service names from compose YAML.
pub fn parse_compose(text: &str) -> Result<(Option<String>, Vec<String>), String> {
    let doc: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(text).map_err(|e| format!("invalid YAML: {e}"))?;
    let name = doc
        .get("name")
        .and_then(serde_yaml_ng::Value::as_str)
        .map(normalize_project_name)
        .filter(|n| !n.is_empty());
    let services = doc
        .get("services")
        .and_then(serde_yaml_ng::Value::as_mapping)
        .map(|m| {
            m.keys()
                .filter_map(|k| k.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    Ok((name, services))
}

/// Builds a [`ComposeFile`] from a path and its contents (`Err` = unreadable).
#[must_use]
pub fn compose_file(root: &Path, path: PathBuf, contents: Result<String, String>) -> ComposeFile {
    let folder = root
        .file_name()
        .map(|n| normalize_project_name(&n.to_string_lossy()))
        .unwrap_or_default();
    let parsed = contents.and_then(|text| parse_compose(&text));
    let (name, services, error) = match parsed {
        Ok((name, services)) => (name, services, None),
        Err(error) => (None, Vec::new(), Some(error)),
    };
    ComposeFile {
        path,
        project: name.unwrap_or(folder),
        services,
        error,
    }
}

/// Reads the compose files of `root` from disk.
#[must_use]
pub fn load_compose_files(root: &Path) -> Vec<ComposeFile> {
    find_compose_files(root, Path::is_file)
        .into_iter()
        .map(|path| {
            let contents = std::fs::read_to_string(&path).map_err(|e| e.to_string());
            compose_file(root, path, contents)
        })
        .collect()
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let strip = |p: &Path| PathBuf::from(p.to_string_lossy().trim_end_matches('/'));
    strip(a) == strip(b)
}

/// Whether `container` belongs to the project at `root`: created by compose
/// from that folder (working-dir label, or a config file inside it), or part
/// of a compose project named like one of `projects`.
#[must_use]
pub fn belongs_to(container: &Container, root: &Path, projects: &[String]) -> bool {
    let labels = &container.labels;
    if labels
        .get(COMPOSE_WORKING_DIR)
        .is_some_and(|dir| same_dir(Path::new(dir), root))
    {
        return true;
    }
    if labels.get(COMPOSE_CONFIG_FILES).is_some_and(|files| {
        files
            .split(',')
            .map(str::trim)
            .any(|f| Path::new(f).parent().is_some_and(|p| same_dir(p, root)))
    }) {
        return true;
    }
    container
        .project()
        .is_some_and(|p| projects.iter().any(|name| name == p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::container;

    #[test]
    fn finds_compose_files_in_compose_order() {
        let root = Path::new("/src/app");
        let present = |p: &Path| {
            p == Path::new("/src/app/docker-compose.yml") || p == Path::new("/src/app/compose.yaml")
        };
        assert_eq!(
            find_compose_files(root, present),
            [
                PathBuf::from("/src/app/compose.yaml"),
                PathBuf::from("/src/app/docker-compose.yml")
            ]
        );
        assert!(find_compose_files(root, |_| false).is_empty());
    }

    #[test]
    fn names_projects_like_compose() {
        assert_eq!(normalize_project_name("My App.v2"), "myappv2");
        assert_eq!(normalize_project_name("_-x"), "x");
        assert_eq!(normalize_project_name("Ferrum Dock"), "ferrumdock");
        let root = Path::new("/src/My App");
        let yaml = "name: Shop\nservices:\n  web:\n    image: nginx\n  db:\n    image: postgres\n";
        let file = compose_file(root, root.join("compose.yaml"), Ok(yaml.into()));
        assert_eq!(file.project, "shop");
        assert_eq!(file.services, ["web", "db"]);
        let unnamed = compose_file(root, root.join("compose.yaml"), Ok("services: {}".into()));
        assert_eq!(unnamed.project, "myapp");
        let bad = compose_file(root, root.join("compose.yaml"), Ok("services: [".into()));
        assert!(bad.error.is_some_and(|e| e.starts_with("invalid YAML")));
        assert_eq!(bad.project, "myapp");
        let project = file.compose_project(root);
        assert_eq!(project.config_files, ["/src/My App/compose.yaml"]);
        assert_eq!(project.working_dir.as_deref(), Some("/src/My App"));
    }

    #[test]
    fn matches_containers_to_the_project() {
        let root = Path::new("/src/app");
        let mut by_dir = container("1", "app-web-1", Some(("app", "web")), "running");
        by_dir
            .labels
            .insert(COMPOSE_WORKING_DIR.into(), "/src/app/".into());
        let mut by_file = container("2", "x-db-1", Some(("x", "db")), "running");
        by_file
            .labels
            .insert(COMPOSE_CONFIG_FILES.into(), "/src/app/compose.yaml".into());
        let by_name = container("3", "shop-db-1", Some(("shop", "db")), "exited");
        let other = container("4", "other", Some(("other", "x")), "running");
        let loose = container("5", "loose", None, "running");
        let names = ["shop".to_owned()];
        assert!(belongs_to(&by_dir, root, &names));
        assert!(belongs_to(&by_file, root, &names));
        assert!(belongs_to(&by_name, root, &names));
        assert!(!belongs_to(&other, root, &names));
        assert!(!belongs_to(&loose, root, &names));
        assert!(!belongs_to(&by_name, root, &[]));
    }

    #[test]
    fn loads_files_from_disk() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        std::fs::write(dir.path().join("compose.yml"), "services:\n  api: {}\n")
            .unwrap_or_else(|e| panic!("write: {e}"));
        let files = load_compose_files(dir.path());
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].services, ["api"]);
    }
}
