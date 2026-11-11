//! The working tree as it was when an agent session started.
//!
//! In a git repository the snapshot is a tree object written from a copy
//! of the index after `git add -A` (like `git stash create`, but it also
//! holds untracked files and leaves the real index and stash alone; the
//! objects are unreferenced, so `git gc` drops them eventually). Elsewhere
//! it is the content of the folder's text files (`.gitignore` respected,
//! size-capped); files too big or binary are remembered as skipped and
//! never reported or touched.
//!
//! Accepting a change moves the baseline for that file, so reviews only
//! show what is still undecided. Paths in the API are relative to the
//! project root.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

/// Text files larger than this are not captured by a folder snapshot.
const FILE_LIMIT: u64 = 1024 * 1024;
/// Total bytes a folder snapshot holds at most.
const TOTAL_LIMIT: u64 = 64 * 1024 * 1024;
const COUNT_LIMIT: usize = 20_000;

#[derive(Debug, Clone)]
enum Kind {
    Git {
        repo: PathBuf,
        tree: String,
        /// The project root inside the repository (`""` at its top).
        prefix: PathBuf,
    },
    Files {
        files: Arc<BTreeMap<PathBuf, Vec<u8>>>,
        skipped: Arc<BTreeSet<PathBuf>>,
    },
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub root: PathBuf,
    kind: Kind,
    /// Accepted content per file (`None`: accepted as absent).
    accepted: BTreeMap<PathBuf, Option<Vec<u8>>>,
}

impl Snapshot {
    /// A git tree when `root` is inside a repository and git works, else a
    /// content snapshot. Blocks: run it in the background.
    pub fn take(root: &Path) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        if let Some(repo) = crate::git::repo_root(&root)
            && let Ok(tree) = write_tree(&repo)
        {
            let prefix = root
                .strip_prefix(&repo)
                .unwrap_or(Path::new(""))
                .to_path_buf();
            return Ok(Self {
                root,
                kind: Kind::Git { repo, tree, prefix },
                accepted: BTreeMap::new(),
            });
        }
        Self::take_files(&root)
    }

    /// A content snapshot of the folder's text files.
    pub fn take_files(root: &Path) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let mut files = BTreeMap::new();
        let mut skipped = BTreeSet::new();
        let mut total = 0u64;
        for rel in walk(&root) {
            let path = root.join(&rel);
            let size = path.metadata().map_or(u64::MAX, |m| m.len());
            if size > FILE_LIMIT || total + size > TOTAL_LIMIT || files.len() >= COUNT_LIMIT {
                skipped.insert(rel);
                continue;
            }
            match std::fs::read(&path) {
                Ok(bytes) if !is_binary(&bytes) => {
                    total += size;
                    files.insert(rel, bytes);
                }
                _ => {
                    skipped.insert(rel);
                }
            }
        }
        Ok(Self {
            root,
            kind: Kind::Files {
                files: Arc::new(files),
                skipped: Arc::new(skipped),
            },
            accepted: BTreeMap::new(),
        })
    }

    pub fn is_git(&self) -> bool {
        matches!(self.kind, Kind::Git { .. })
    }

    /// "git tree 1a2b3c4" or "412 files".
    pub fn describe(&self) -> String {
        match &self.kind {
            Kind::Git { tree, .. } => format!("git tree {}", &tree[..tree.len().min(7)]),
            Kind::Files { files, .. } => format!("{} files", files.len()),
        }
    }

    /// The baseline content of `rel` (`None`: it did not exist).
    pub fn base(&self, rel: &Path) -> Option<Vec<u8>> {
        if let Some(accepted) = self.accepted.get(rel) {
            return accepted.clone();
        }
        match &self.kind {
            Kind::Git { repo, tree, prefix } => {
                let spec = format!("{tree}:{}", git_path(&prefix.join(rel)));
                let output = git(repo)
                    .args(["cat-file", "--filters", &spec])
                    .stderr(Stdio::null())
                    .output()
                    .ok()?;
                output.status.success().then_some(output.stdout)
            }
            Kind::Files { files, .. } => files.get(rel).cloned(),
        }
    }

    /// Makes `content` the baseline of `rel` (after accepting a change).
    pub fn accept(&mut self, rel: &Path, content: Option<Vec<u8>>) {
        self.accepted.insert(rel.to_path_buf(), content);
    }

    /// Whether a path (absolute) may hold reviewable changes: inside the
    /// root, not in `.git`, not skipped.
    pub fn covers(&self, path: &Path) -> bool {
        let Ok(rel) = path.strip_prefix(&self.root) else {
            return false;
        };
        if rel.components().any(|c| c.as_os_str() == ".git") {
            return false;
        }
        match &self.kind {
            Kind::Files { skipped, .. } => !skipped.contains(rel),
            Kind::Git { .. } => true,
        }
    }

    /// Files whose content differs from the baseline, sorted. Blocks.
    pub fn changed_paths(&self) -> Result<Vec<PathBuf>, String> {
        let mut candidates: BTreeSet<PathBuf> = self.accepted.keys().cloned().collect();
        match &self.kind {
            Kind::Git { repo, tree, .. } => {
                let current = write_tree(repo)?;
                let output = git(repo)
                    .current_dir(&self.root)
                    .args(["diff", "--name-only", "-z", "--no-renames", "--relative"])
                    .arg(tree)
                    .arg(&current)
                    .output()
                    .map_err(|e| format!("git diff: {e}"))?;
                if !output.status.success() {
                    return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
                }
                candidates.extend(
                    output
                        .stdout
                        .split(|b| *b == 0)
                        .filter(|p| !p.is_empty())
                        .map(|p| PathBuf::from(String::from_utf8_lossy(p).to_string())),
                );
            }
            Kind::Files { files, skipped } => {
                candidates.extend(files.keys().cloned());
                candidates.extend(walk(&self.root).filter(|rel| !skipped.contains(rel)));
            }
        }
        Ok(candidates
            .into_iter()
            .filter(|rel| self.covers(&self.root.join(rel)))
            .filter(|rel| self.base(rel) != read(&self.root.join(rel)))
            .collect())
    }
}

/// A file's bytes, `None` when it does not exist (or is not a file).
pub fn read(path: &Path) -> Option<Vec<u8>> {
    path.is_file().then(|| std::fs::read(path).ok()).flatten()
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(8192)].contains(&0)
}

/// Every file under `root` (relative), `.gitignore` respected, `.git` and
/// hidden files included except the `.git` folder.
fn walk(root: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    ignore::WalkBuilder::new(root)
        .hidden(false)
        .require_git(false)
        .filter_entry(|e| e.file_name() != ".git")
        .build()
        .flatten()
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter_map(move |e| e.path().strip_prefix(root).ok().map(Path::to_path_buf))
}

fn git(repo: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(repo)
        .stdin(Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0");
    command
}

/// `a/b` for git, whatever the platform's separator.
fn git_path(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// The whole working tree (tracked and untracked, ignored files left out)
/// as a tree object, through a throwaway copy of the index.
fn write_tree(repo: &Path) -> Result<String, String> {
    let git_dir = crate::git::git_dir(repo).ok_or("not a git repository")?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let index =
        std::env::temp_dir().join(format!("ferrum-agent-index-{}-{stamp}", std::process::id()));
    // The real index's stat cache saves rehashing unchanged files.
    if git_dir.join("index").is_file() {
        std::fs::copy(git_dir.join("index"), &index).map_err(|e| e.to_string())?;
    }
    let run = |args: &[&str]| -> Result<String, String> {
        let output = git(repo)
            .env("GIT_INDEX_FILE", &index)
            .args(args)
            .output()
            .map_err(|e| format!("git: {e}"))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(format!(
                "git {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    };
    let result = run(&["add", "-A", "--", "."]).and_then(|_| run(&["write-tree"]));
    std::fs::remove_file(&index).ok();
    std::fs::remove_file(index.with_extension("lock")).ok();
    result
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn git_cmd(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("run git");
        assert!(output.status.success(), "git {args:?}: {output:?}");
    }

    pub(crate) fn have_git() -> bool {
        Command::new("git").arg("--version").output().is_ok()
    }

    /// A repository with one commit plus an uncommitted and an untracked
    /// file (both are part of the snapshot).
    pub(crate) fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git_cmd(root, &["init", "-q", "-b", "main"]);
        git_cmd(root, &["config", "user.name", "Ferrum Test"]);
        git_cmd(root, &["config", "user.email", "test@example.com"]);
        git_cmd(root, &["config", "commit.gpgsign", "false"]);
        std::fs::write(root.join("a.txt"), "one\ntwo\nthree\n").expect("write");
        std::fs::write(root.join(".gitignore"), "target/\n").expect("write");
        git_cmd(root, &["add", "."]);
        git_cmd(root, &["commit", "-q", "-m", "first"]);
        std::fs::write(root.join("a.txt"), "one\ntwo\nthree\nfour\n").expect("write");
        std::fs::write(root.join("notes.md"), "draft\n").expect("write");
        dir
    }

    #[test]
    fn git_snapshot_holds_uncommitted_and_untracked_files() {
        if !have_git() {
            return;
        }
        let dir = repo();
        let root = dir.path();
        let status_before = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(root)
            .output()
            .expect("status");
        let snap = Snapshot::take(root).expect("snapshot");
        assert!(snap.is_git(), "{}", snap.describe());
        let status_after = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(root)
            .output()
            .expect("status");
        assert_eq!(status_before.stdout, status_after.stdout, "index untouched");
        assert!(snap.changed_paths().expect("diff").is_empty());

        // The agent edits, adds, deletes; ignored output is not a change.
        std::fs::write(root.join("a.txt"), "one\n2\nthree\nfour\n").expect("write");
        std::fs::write(root.join("new.rs"), "fn x() {}\n").expect("write");
        std::fs::remove_file(root.join("notes.md")).expect("rm");
        std::fs::create_dir_all(root.join("target")).expect("mkdir");
        std::fs::write(root.join("target/out"), "bin").expect("write");
        let changed = snap.changed_paths().expect("diff");
        assert_eq!(
            changed,
            [
                PathBuf::from("a.txt"),
                PathBuf::from("new.rs"),
                PathBuf::from("notes.md")
            ]
        );
        assert_eq!(
            snap.base(Path::new("a.txt")).as_deref(),
            Some(&b"one\ntwo\nthree\nfour\n"[..])
        );
        assert_eq!(
            snap.base(Path::new("notes.md")).as_deref(),
            Some(&b"draft\n"[..])
        );
        assert_eq!(snap.base(Path::new("new.rs")), None);
        assert!(!snap.covers(&root.canonicalize().expect("c").join(".git/HEAD")));
    }

    #[test]
    fn subfolder_projects_see_their_own_paths() {
        if !have_git() {
            return;
        }
        let dir = repo();
        let sub = dir.path().join("web");
        std::fs::create_dir_all(&sub).expect("mkdir");
        std::fs::write(sub.join("app.js"), "a\n").expect("write");
        let snap = Snapshot::take(&sub).expect("snapshot");
        std::fs::write(sub.join("app.js"), "b\n").expect("write");
        std::fs::write(dir.path().join("a.txt"), "outside\n").expect("write");
        assert_eq!(
            snap.changed_paths().expect("diff"),
            [PathBuf::from("app.js")]
        );
        assert_eq!(snap.base(Path::new("app.js")).as_deref(), Some(&b"a\n"[..]));
    }

    #[test]
    fn folder_snapshot_without_git() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(root.join("a.txt"), "x\n").expect("write");
        std::fs::write(root.join("blob.bin"), [0u8, 1, 2]).expect("write");
        let mut snap = Snapshot::take_files(root).expect("snapshot");
        assert!(!snap.is_git());
        std::fs::write(root.join("a.txt"), "y\n").expect("write");
        std::fs::write(root.join("blob.bin"), [9u8, 0]).expect("write");
        std::fs::write(root.join("b.txt"), "new\n").expect("write");
        assert_eq!(
            snap.changed_paths().expect("scan"),
            [PathBuf::from("a.txt"), PathBuf::from("b.txt")],
            "binary files are skipped, never reported"
        );
        snap.accept(Path::new("a.txt"), Some(b"y\n".to_vec()));
        assert_eq!(
            snap.changed_paths().expect("scan"),
            [PathBuf::from("b.txt")]
        );
    }
}
