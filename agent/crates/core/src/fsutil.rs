//! Small file-system helpers shared by the snapshot, the review and the
//! tools: finding a repository, its git directory, and atomic writes.

use std::io;
use std::path::{Path, PathBuf};

/// The closest ancestor of `path` (itself included) holding `.git`.
pub fn repo_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

/// The git directory of a working tree: `.git` itself, or the target of a
/// `gitdir:` file (worktrees, submodules).
pub fn git_dir(repo_root: &Path) -> Option<PathBuf> {
    let dot_git = repo_root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let contents = std::fs::read_to_string(&dot_git).ok()?;
    let target = PathBuf::from(contents.trim().strip_prefix("gitdir:")?.trim());
    Some(if target.is_absolute() {
        target
    } else {
        repo_root.join(target)
    })
}

/// Writes through a temporary file in the same folder and renames it over
/// `path`, keeping the old file's permissions.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(".{name}.ferrum-tmp"));
    std::fs::write(&tmp, bytes)?;
    if let Ok(metadata) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, metadata.permissions());
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}
