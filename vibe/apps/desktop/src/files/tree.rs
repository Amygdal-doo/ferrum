//! The Files pane's model: the project folder read one directory at a time
//! (when it is expanded), respecting `.gitignore`, folders first, and the
//! expanded folders, flattened into rows for drawing.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Names never listed.
#[must_use]
pub fn is_hidden_name(name: &str) -> bool {
    matches!(name, ".git" | ".DS_Store")
}

/// A file or folder in a listed directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
}

/// A row to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    /// 0 for the root's children.
    pub depth: usize,
    pub expanded: bool,
}

/// Sorts folders first, then by name ignoring case (then exactly, so the
/// order is stable).
pub fn sort_entries(entries: &mut [Entry]) {
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// `dir`'s children that `.gitignore` (and `.ignore`, the global git
/// excludes, `.git/info/exclude`) keep, dotfiles included, `.git` never.
#[must_use]
pub fn read_dir(dir: &Path) -> Vec<Entry> {
    let walker = ignore::WalkBuilder::new(dir)
        .max_depth(Some(1))
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .parents(true)
        .follow_links(false)
        .filter_entry(|e| !is_hidden_name(&e.file_name().to_string_lossy()))
        .build();
    let mut entries: Vec<Entry> = walker
        .filter_map(Result::ok)
        .filter(|e| e.depth() == 1)
        .map(|e| {
            let path = e.path().to_path_buf();
            // A link to a folder behaves like a folder.
            let is_dir = e.file_type().is_some_and(|t| t.is_dir())
                || std::fs::metadata(&path).is_ok_and(|m| m.is_dir());
            Entry {
                name: e.file_name().to_string_lossy().into_owned(),
                path,
                is_dir,
            }
        })
        .collect();
    sort_entries(&mut entries);
    entries
}

#[derive(Debug, Clone)]
pub struct FileTree {
    root: PathBuf,
    expanded: HashSet<PathBuf>,
    /// Directories read so far.
    children: HashMap<PathBuf, Vec<Entry>>,
}

impl FileTree {
    /// A tree of `root` with only the root read.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let mut tree = Self {
            root: root.clone(),
            expanded: HashSet::new(),
            children: HashMap::new(),
        };
        tree.expand(&root);
        tree
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn is_expanded(&self, dir: &Path) -> bool {
        self.expanded.contains(dir)
    }

    /// Whether `dir` was read (folders are read when first expanded).
    #[cfg(test)]
    #[must_use]
    pub fn is_loaded(&self, dir: &Path) -> bool {
        self.children.contains_key(dir)
    }

    /// Expands `dir`, reading it the first time.
    pub fn expand(&mut self, dir: &Path) {
        if !self.children.contains_key(dir) {
            self.children.insert(dir.to_path_buf(), read_dir(dir));
        }
        self.expanded.insert(dir.to_path_buf());
    }

    /// Collapses `dir` (the root stays open).
    pub fn collapse(&mut self, dir: &Path) {
        if dir != self.root {
            self.expanded.remove(dir);
        }
    }

    pub fn toggle(&mut self, dir: &Path) {
        if self.is_expanded(dir) {
            self.collapse(dir);
        } else {
            self.expand(dir);
        }
    }

    /// Collapses every folder.
    pub fn collapse_all(&mut self) {
        let root = self.root.clone();
        self.expanded.retain(|dir| *dir == root);
    }

    /// Re-reads the folders read so far; ones that disappeared are
    /// forgotten (with their expanded state).
    pub fn refresh(&mut self) {
        let dirs: Vec<PathBuf> = self.children.keys().cloned().collect();
        for dir in dirs {
            if dir == self.root || dir.is_dir() {
                self.children.insert(dir.clone(), read_dir(&dir));
            } else {
                self.children.remove(&dir);
                self.expanded.remove(&dir);
            }
        }
    }

    /// The rows to draw: depth first, an expanded folder's children
    /// under it.
    #[must_use]
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        self.push_children(&self.root, 0, &mut rows);
        rows
    }

    fn push_children(&self, dir: &Path, depth: usize, rows: &mut Vec<Row>) {
        let Some(entries) = self.children.get(dir) else {
            return;
        };
        for entry in entries {
            let expanded = entry.is_dir && self.expanded.contains(&entry.path);
            rows.push(Row {
                path: entry.path.clone(),
                name: entry.name.clone(),
                is_dir: entry.is_dir,
                depth,
                expanded,
            });
            if expanded {
                self.push_children(&entry.path, depth + 1, rows);
            }
        }
    }
}
