//! The tree view's model: a lazily-read directory tree with expand/collapse
//! state, flattened into rows for rendering, plus small file-system helpers.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

/// Names never shown in the tree.
pub fn is_hidden_name(name: &str) -> bool {
    matches!(name, ".git" | ".DS_Store")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntryInfo {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    /// 0 for direct children of the root.
    pub depth: usize,
    pub expanded: bool,
}

/// Reads `dir`: directories first, then files, each sorted case-insensitively.
pub fn read_dir_sorted(dir: &Path) -> io::Result<Vec<DirEntryInfo>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_hidden_name(&name) {
            continue;
        }
        let path = entry.path();
        // Follows symlinks so a link to a folder behaves like a folder.
        let is_dir = std::fs::metadata(&path).is_ok_and(|m| m.is_dir());
        entries.push(DirEntryInfo { path, name, is_dir });
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

#[derive(Debug, Clone)]
pub struct FileTree {
    root: PathBuf,
    expanded: HashSet<PathBuf>,
    children: HashMap<PathBuf, Vec<DirEntryInfo>>,
}

impl FileTree {
    /// A tree rooted at `root` with the root's children loaded.
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

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_expanded(&self, path: &Path) -> bool {
        self.expanded.contains(path)
    }

    /// Expands a directory, reading it if it was not read yet.
    pub fn expand(&mut self, path: &Path) {
        if !self.children.contains_key(path) {
            let entries = read_dir_sorted(path).unwrap_or_default();
            self.children.insert(path.to_path_buf(), entries);
        }
        self.expanded.insert(path.to_path_buf());
    }

    pub fn collapse(&mut self, path: &Path) {
        if path != self.root {
            self.expanded.remove(path);
        }
    }

    pub fn toggle(&mut self, path: &Path) {
        if self.is_expanded(path) {
            self.collapse(path);
        } else {
            self.expand(path);
        }
    }

    /// Re-reads every loaded directory, forgetting ones that disappeared.
    pub fn refresh(&mut self) {
        let dirs: Vec<PathBuf> = self.children.keys().cloned().collect();
        for dir in dirs {
            self.refresh_dir(&dir);
        }
    }

    /// Re-reads one directory (no-op for directories never loaded).
    pub fn refresh_dir(&mut self, dir: &Path) {
        if !self.children.contains_key(dir) {
            return;
        }
        match read_dir_sorted(dir) {
            Ok(entries) => {
                self.children.insert(dir.to_path_buf(), entries);
            }
            Err(_) if dir != self.root => {
                self.children.remove(dir);
                self.expanded.remove(dir);
            }
            Err(_) => {
                self.children.insert(dir.to_path_buf(), Vec::new());
            }
        }
    }

    /// Expands every ancestor of `path` below the root so it becomes visible.
    /// Returns false when `path` is outside the root.
    pub fn reveal(&mut self, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return false;
        };
        let mut current = self.root.clone();
        let components: Vec<_> = relative.components().collect();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            current.push(component);
            self.expand(&current);
        }
        true
    }

    /// Rows to draw, depth-first, children of expanded directories inline.
    pub fn visible_entries(&self) -> Vec<VisibleEntry> {
        let mut out = Vec::new();
        self.push_children(&self.root, 0, &mut out);
        out
    }

    fn push_children(&self, dir: &Path, depth: usize, out: &mut Vec<VisibleEntry>) {
        let Some(entries) = self.children.get(dir) else {
            return;
        };
        for entry in entries {
            let expanded = entry.is_dir && self.expanded.contains(&entry.path);
            out.push(VisibleEntry {
                path: entry.path.clone(),
                name: entry.name.clone(),
                is_dir: entry.is_dir,
                depth,
                expanded,
            });
            if expanded {
                self.push_children(&entry.path, depth + 1, out);
            }
        }
    }
}

/// Creates an empty file (and missing parent folders); fails if it exists.
pub fn create_file(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map(|_| ())
}

/// Creates a folder (and missing parents); fails if it already exists.
pub fn create_dir(path: &Path) -> io::Result<()> {
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "already exists",
        ));
    }
    std::fs::create_dir_all(path)
}

/// Renames / moves; refuses to overwrite an existing target.
pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    if to.exists() && !same_file_ignoring_case(from, to) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "target already exists",
        ));
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(from, to)
}

/// Renaming `a.txt` to `A.txt` on a case-insensitive file system.
fn same_file_ignoring_case(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

/// Files under `root` relative to it, in name order, respecting `.gitignore`,
/// including dotfiles but never `.git`; at most `limit` entries.
pub fn list_project_files(root: &Path, limit: usize) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in crate::project_search::walker(root).build() {
        if files.len() >= limit {
            break;
        }
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        if entry.file_name() == ".DS_Store" {
            continue;
        }
        if let Ok(relative) = entry.path().strip_prefix(root) {
            files.push(relative.to_path_buf());
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("src/nested")).expect("mkdir");
        fs::create_dir_all(root.join("Docs")).expect("mkdir");
        fs::create_dir_all(root.join(".git")).expect("mkdir");
        fs::create_dir_all(root.join("target")).expect("mkdir");
        fs::write(root.join("b.txt"), "").expect("write");
        fs::write(root.join("A.txt"), "").expect("write");
        fs::write(root.join(".env"), "").expect("write");
        fs::write(root.join(".DS_Store"), "").expect("write");
        fs::write(root.join(".gitignore"), "target/\n").expect("write");
        fs::write(root.join("src/main.rs"), "").expect("write");
        fs::write(root.join("src/nested/deep.rs"), "").expect("write");
        fs::write(root.join("target/junk"), "").expect("write");
        dir
    }

    fn names(tree: &FileTree) -> Vec<(usize, String)> {
        tree.visible_entries()
            .into_iter()
            .map(|e| (e.depth, e.name))
            .collect()
    }

    #[test]
    fn dirs_first_case_insensitive_hidden_names_skipped() {
        let dir = sample();
        let tree = FileTree::new(dir.path());
        let top: Vec<String> = names(&tree).into_iter().map(|(_, n)| n).collect();
        assert_eq!(
            top,
            vec![
                "Docs",
                "src",
                "target",
                ".env",
                ".gitignore",
                "A.txt",
                "b.txt"
            ]
        );
    }

    #[test]
    fn expand_collapse_toggle() {
        let dir = sample();
        let mut tree = FileTree::new(dir.path());
        let src = dir.path().join("src");
        tree.toggle(&src);
        assert!(tree.is_expanded(&src));
        assert!(names(&tree).contains(&(1, "main.rs".to_string())));
        assert!(names(&tree).contains(&(1, "nested".to_string())));
        tree.toggle(&src);
        assert!(!names(&tree).contains(&(1, "main.rs".to_string())));
        // The root never collapses.
        tree.collapse(dir.path());
        assert!(!tree.visible_entries().is_empty());
    }

    #[test]
    fn reveal_expands_ancestors() {
        let dir = sample();
        let mut tree = FileTree::new(dir.path());
        assert!(tree.reveal(&dir.path().join("src/nested/deep.rs")));
        assert!(names(&tree).contains(&(2, "deep.rs".to_string())));
        assert!(!tree.reveal(Path::new("/definitely/elsewhere")));
    }

    #[test]
    fn refresh_picks_up_changes() {
        let dir = sample();
        let mut tree = FileTree::new(dir.path());
        tree.expand(&dir.path().join("src"));
        fs::write(dir.path().join("src/new.rs"), "").expect("write");
        fs::remove_dir_all(dir.path().join("src/nested")).expect("rm");
        tree.refresh();
        let all = names(&tree);
        assert!(all.contains(&(1, "new.rs".to_string())));
        assert!(!all.contains(&(1, "nested".to_string())));
    }

    #[test]
    fn create_and_rename_helpers() {
        let dir = sample();
        let file = dir.path().join("x/y/new.txt");
        create_file(&file).expect("create");
        assert!(file.is_file());
        assert!(create_file(&file).is_err());
        let folder = dir.path().join("made");
        create_dir(&folder).expect("mkdir");
        assert!(create_dir(&folder).is_err());
        let renamed = dir.path().join("made2");
        rename(&folder, &renamed).expect("rename");
        assert!(renamed.is_dir() && !folder.exists());
        assert!(rename(&renamed, &dir.path().join("b.txt")).is_err());
    }

    #[test]
    fn project_files_respect_gitignore() {
        let dir = sample();
        let files = list_project_files(dir.path(), 100);
        let files: Vec<String> = files
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(files.contains(&"src/nested/deep.rs".to_string()));
        assert!(files.contains(&".env".to_string()));
        assert!(
            !files
                .iter()
                .any(|f| f.starts_with("target") || f.starts_with(".git/"))
        );
        assert!(!files.contains(&".DS_Store".to_string()));
        assert_eq!(list_project_files(dir.path(), 2).len(), 2);
    }
}
