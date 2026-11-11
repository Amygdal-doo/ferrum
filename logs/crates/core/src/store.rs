//! `~/.ferrum/logs/projects.json`: each project's saved sources, keyed by
//! the project folder.

use crate::source::SourceConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub projects: BTreeMap<String, ProjectConfig>,
}

/// `~/.ferrum/logs/projects.json`.
#[must_use]
pub fn store_file() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        Path::new(&home)
            .join(".ferrum")
            .join("logs")
            .join("projects.json"),
    )
}

fn key(project: &Path) -> String {
    project.to_string_lossy().into_owned()
}

impl Store {
    /// A missing or unreadable file is an empty store.
    #[must_use]
    pub fn load(file: &Path) -> Self {
        std::fs::read_to_string(file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Writes atomically (temp file + rename).
    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = file.with_extension(format!("json.{}.tmp", std::process::id()));
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, file)
    }

    #[must_use]
    pub fn sources(&self, project: &Path) -> Vec<SourceConfig> {
        self.projects
            .get(&key(project))
            .map(|p| p.sources.clone())
            .unwrap_or_default()
    }

    /// Replaces a project's sources; an empty list removes the entry.
    pub fn set_sources(&mut self, project: &Path, sources: Vec<SourceConfig>) {
        if sources.is_empty() {
            self.projects.remove(&key(project));
        } else {
            self.projects
                .insert(key(project), ProjectConfig { sources });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceKind;

    #[test]
    fn round_trips_per_project() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("logs/projects.json");
        assert_eq!(Store::load(&file), Store::default());
        let mut store = Store::default();
        let source = SourceConfig::new(
            "app",
            SourceKind::File {
                path: "logs/*.log".into(),
            },
        );
        store.set_sources(Path::new("/p/a"), vec![source.clone()]);
        store.save(&file).expect("save");
        let loaded = Store::load(&file);
        assert_eq!(loaded.sources(Path::new("/p/a")), [source]);
        assert!(loaded.sources(Path::new("/p/b")).is_empty());
        let mut loaded = loaded;
        loaded.set_sources(Path::new("/p/a"), Vec::new());
        assert!(loaded.projects.is_empty());
        std::fs::write(&file, "nope").expect("write");
        assert_eq!(Store::load(&file), Store::default());
    }
}
