//! Git status colours for the Files pane: one `git status --porcelain -z`
//! run in the background, kept as a map from absolute path to status;
//! folders take the strongest status found under them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A path's state, weakest first (a folder shows its strongest child's).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileStatus {
    Untracked,
    Added,
    Modified,
    Deleted,
    Conflict,
}

impl FileStatus {
    /// From the two status letters of `git status --porcelain`.
    #[must_use]
    pub fn from_xy(x: u8, y: u8) -> Option<FileStatus> {
        Some(match (x, y) {
            (b'?', b'?') => FileStatus::Untracked,
            (b'!', b'!') => return None,
            (b'U', _) | (_, b'U') | (b'A', b'A') | (b'D', b'D') => FileStatus::Conflict,
            (b'D', _) | (_, b'D') => FileStatus::Deleted,
            (b'A', _) => FileStatus::Added,
            (b' ', b' ') => return None,
            _ => FileStatus::Modified,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitMarks {
    /// Changed files, and untracked folders (listed whole by git).
    paths: HashMap<PathBuf, FileStatus>,
    /// Every folder above a changed path, with its strongest status.
    dirs: HashMap<PathBuf, FileStatus>,
}

impl GitMarks {
    /// Parses `git status --porcelain=v1 -z` run in the repository whose
    /// top level is `repo`.
    #[must_use]
    pub fn parse(repo: &Path, output: &str) -> GitMarks {
        let mut marks = GitMarks::default();
        let mut fields = output.split('\0');
        while let Some(field) = fields.next() {
            let bytes = field.as_bytes();
            if bytes.len() < 4 {
                continue;
            }
            let (x, y) = (bytes[0], bytes[1]);
            if matches!(x, b'R' | b'C') {
                // The source of a rename or copy follows.
                fields.next();
            }
            let Some(status) = FileStatus::from_xy(x, y) else {
                continue;
            };
            let path = repo.join(field[3..].trim_end_matches('/'));
            marks.insert(repo, path, status);
        }
        marks
    }

    fn insert(&mut self, repo: &Path, path: PathBuf, status: FileStatus) {
        for dir in path.ancestors().skip(1) {
            if !dir.starts_with(repo) {
                break;
            }
            let entry = self.dirs.entry(dir.to_path_buf()).or_insert(status);
            *entry = (*entry).max(status);
        }
        let entry = self.paths.entry(path).or_insert(status);
        *entry = (*entry).max(status);
    }

    /// Runs `git status` for the repository `root` is in (empty when it is
    /// not in one, or git fails). Blocking: call it off the UI thread.
    #[must_use]
    pub fn read(root: &Path) -> GitMarks {
        let Some(repo) = ferrum_ui::project::git_root(root) else {
            return GitMarks::default();
        };
        let output = Command::new("git")
            .args(["status", "--porcelain=v1", "-z", "--untracked-files=normal"])
            .current_dir(&repo)
            // A read-only look: never take the index lock.
            .env("GIT_OPTIONAL_LOCKS", "0")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        match output {
            Ok(out) if out.status.success() => {
                GitMarks::parse(&repo, &String::from_utf8_lossy(&out.stdout))
            }
            _ => GitMarks::default(),
        }
    }

    /// The colour to give `path`: its own status, its strongest child's for
    /// a folder, or that of an untracked folder it is in.
    #[must_use]
    pub fn status(&self, path: &Path, is_dir: bool) -> Option<FileStatus> {
        if let Some(status) = self.paths.get(path) {
            return Some(*status);
        }
        if is_dir && let Some(status) = self.dirs.get(path) {
            return Some(*status);
        }
        path.ancestors()
            .skip(1)
            .find_map(|dir| self.paths.get(dir))
            .filter(|s| **s == FileStatus::Untracked)
            .copied()
    }

    #[cfg(test)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}
