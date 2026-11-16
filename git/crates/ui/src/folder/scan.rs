//! Reading a folder that is not a git work tree: which files to look at,
//! a snapshot of their text, and the repositories found below it. Plain
//! functions on the file system (run on a background thread).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

/// Folders never looked into: dependencies and build output (and, see
/// [`skip_dir`], hidden folders such as `.git`).
const HEAVY_DIRS: [&str; 5] = ["node_modules", "target", "dist", "build", ".git"];

/// How much of a folder a snapshot reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Text files at most; past it the folder is too big.
    pub max_files: usize,
    /// Bytes of text at most; past it the folder is too big.
    pub max_bytes: u64,
    /// Larger files are left out.
    pub max_file: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_files: 20_000,
            max_bytes: 200 * 1024 * 1024,
            max_file: 1024 * 1024,
        }
    }
}

/// A text file as last read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub len: u64,
    pub modified: Option<SystemTime>,
    pub text: Arc<str>,
}

/// The text files of a folder, by `/`-separated path relative to it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub files: BTreeMap<String, Entry>,
}

/// Why a folder was not snapshotted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TooBig {
    /// The home folder or the file system root: never read whole.
    Home,
    /// More files or bytes than [`Limits`] allow.
    Over { files: usize, bytes: u64 },
}

/// Whether a folder named `name` is skipped: heavy or hidden.
#[must_use]
pub fn skip_dir(name: &str) -> bool {
    name.starts_with('.') || HEAVY_DIRS.contains(&name)
}

/// Whether a change at `path` (inside `root`) concerns a file a snapshot
/// of `root` would read: not under a skipped folder or `~/Library`.
#[must_use]
pub fn is_watched(root: &Path, path: &Path, home: Option<&Path>) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let mut parents: Vec<_> = relative.components().collect();
    parents.pop();
    let mut dir = root.to_path_buf();
    parents.into_iter().all(|part| {
        dir.push(part);
        !skip_dir(&part.as_os_str().to_string_lossy()) && !is_library(&dir, home)
    })
}

/// `~/Library` (macOS caches, mail, app data).
fn is_library(dir: &Path, home: Option<&Path>) -> bool {
    home.is_some_and(|home| dir == home.join("Library"))
}

/// Whether `root` is too broad to snapshot at all.
#[must_use]
pub fn too_broad(root: &Path, home: Option<&Path>) -> bool {
    root.parent().is_none() || home.is_some_and(|home| root == home)
}

/// The folder's text files. Stops with [`TooBig`] as soon as a limit is
/// passed. `previous` saves reading files whose size and time are
/// unchanged.
pub fn snapshot(
    root: &Path,
    limits: Limits,
    home: Option<&Path>,
    previous: Option<&Snapshot>,
) -> Result<Snapshot, TooBig> {
    if too_broad(root, home) {
        return Err(TooBig::Home);
    }
    let mut files = BTreeMap::new();
    let mut bytes = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                let name = entry.file_name();
                if !skip_dir(&name.to_string_lossy()) && !is_library(&path, home) {
                    stack.push(path);
                }
                continue;
            }
            // Symbolic links are not followed (nor read).
            if !kind.is_file() {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.len() > limits.max_file {
                continue;
            }
            let Some(key) = relative_key(root, &path) else {
                continue;
            };
            let modified = meta.modified().ok();
            let known = previous
                .and_then(|p| p.files.get(&key))
                .filter(|e| e.len == meta.len() && e.modified == modified && modified.is_some());
            let text = match known {
                Some(entry) => Some(entry.text.clone()),
                None => read_text(&path),
            };
            let Some(text) = text else { continue };
            bytes += meta.len();
            files.insert(
                key,
                Entry {
                    len: meta.len(),
                    modified,
                    text,
                },
            );
            if files.len() > limits.max_files || bytes > limits.max_bytes {
                return Err(TooBig::Over {
                    files: files.len(),
                    bytes,
                });
            }
        }
    }
    Ok(Snapshot { files })
}

/// `path` relative to `root`, `/`-separated.
fn relative_key(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let parts: Vec<_> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(parts.join("/"))
}

/// The file's text; `None` for binary (a NUL byte, or not UTF-8) or
/// unreadable files.
fn read_text(path: &Path) -> Option<Arc<str>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.iter().take(8000).any(|b| *b == 0) {
        return None;
    }
    String::from_utf8(bytes).ok().map(Arc::from)
}

/// Repositories found at most this many folders below the one searched.
pub const REPO_DEPTH: usize = 3;
/// Repositories listed at most.
pub const MAX_REPOS: usize = 200;

/// The git repositories below `root` (not `root` itself), at most `depth`
/// folders down, sorted. A repository's own subfolders are not searched.
#[must_use]
pub fn find_repos(root: &Path, depth: usize, home: Option<&Path>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut level = vec![root.to_path_buf()];
    for _ in 0..depth {
        let mut next = Vec::new();
        for dir in level {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                if !entry.file_type().is_ok_and(|k| k.is_dir()) {
                    continue;
                }
                let path = entry.path();
                if skip_dir(&entry.file_name().to_string_lossy()) || is_library(&path, home) {
                    continue;
                }
                if path.join(".git").exists() {
                    found.push(path);
                    if found.len() >= MAX_REPOS {
                        found.sort();
                        return found;
                    }
                } else {
                    next.push(path);
                }
            }
        }
        level = next;
    }
    found.sort();
    found
}
