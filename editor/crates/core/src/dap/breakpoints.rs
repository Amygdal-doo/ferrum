//! Breakpoints the user set, per project, saved in
//! `~/.ferrum/editor/breakpoints.json` (`{ "<project>": { "<file>":
//! [lines] } }`, files relative to the project, lines 1-based) so they are
//! there next time. Nothing is written into the project itself.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Breakpoints {
    /// Absolute file → 1-based lines.
    files: BTreeMap<PathBuf, BTreeSet<usize>>,
}

impl Breakpoints {
    /// Adds or removes the breakpoint at 1-based `line`; returns whether
    /// it is now set.
    pub fn toggle(&mut self, path: &Path, line: usize) -> bool {
        let lines = self.files.entry(path.to_path_buf()).or_default();
        let set = if lines.remove(&line) {
            false
        } else {
            lines.insert(line);
            true
        };
        if lines.is_empty() {
            self.files.remove(path);
        }
        set
    }

    pub fn lines(&self, path: &Path) -> Vec<usize> {
        self.files
            .get(path)
            .map(|l| l.iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn files(&self) -> impl Iterator<Item = (&PathBuf, &BTreeSet<usize>)> {
        self.files.iter()
    }

    pub fn count(&self) -> usize {
        self.files.values().map(BTreeSet::len).sum()
    }

    pub fn clear(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.files).into_keys().collect()
    }

    /// The project's breakpoints from the store document.
    pub fn load(store: &Path, project: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(store) else {
            return Self::default();
        };
        let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Self::default();
        };
        let mut out = Self::default();
        for (file, lines) in doc[project.to_string_lossy().as_ref()]
            .as_object()
            .into_iter()
            .flatten()
        {
            let lines: BTreeSet<usize> = lines
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|l| l.as_u64().map(|l| l as usize))
                .filter(|l| *l > 0)
                .collect();
            if !lines.is_empty() {
                out.files.insert(project.join(file), lines);
            }
        }
        out
    }

    /// Writes the project's breakpoints into the store, keeping other
    /// projects'. Files outside the project are not saved.
    pub fn save(&self, store: &Path, project: &Path) -> std::io::Result<()> {
        let mut doc = std::fs::read_to_string(store)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .filter(serde_json::Value::is_object)
            .unwrap_or_else(|| serde_json::json!({}));
        let mut files = serde_json::Map::new();
        for (path, lines) in &self.files {
            if let Ok(rel) = path.strip_prefix(project) {
                files.insert(rel.to_string_lossy().to_string(), serde_json::json!(lines));
            }
        }
        let key = project.to_string_lossy().to_string();
        if let Some(map) = doc.as_object_mut() {
            if files.is_empty() {
                map.remove(&key);
            } else {
                map.insert(key, serde_json::Value::Object(files));
            }
        }
        if let Some(parent) = store.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(&doc).map_err(std::io::Error::other)?;
        crate::files::write_atomic(store, text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_and_persists_per_project() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = dir.path().join("breakpoints.json");
        let project = PathBuf::from("/p/app");
        let other = PathBuf::from("/p/other");
        let mut bps = Breakpoints::default();
        assert!(bps.toggle(&project.join("src/main.rs"), 3));
        assert!(bps.toggle(&project.join("src/main.rs"), 10));
        assert!(!bps.toggle(&project.join("src/main.rs"), 3));
        bps.toggle(Path::new("/elsewhere/x.rs"), 1);
        bps.save(&store, &project).expect("save");
        let mut theirs = Breakpoints::default();
        theirs.toggle(&other.join("a.go"), 5);
        theirs.save(&store, &other).expect("save");

        let loaded = Breakpoints::load(&store, &project);
        assert_eq!(loaded.lines(&project.join("src/main.rs")), [10]);
        assert_eq!(loaded.count(), 1, "files outside the project are not saved");
        assert_eq!(
            Breakpoints::load(&store, &other).lines(&other.join("a.go")),
            [5]
        );
        assert_eq!(
            Breakpoints::load(&dir.path().join("none.json"), &project).count(),
            0
        );
    }
}
