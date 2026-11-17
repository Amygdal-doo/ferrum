//! The git side of parallel runs, through the `git` command (blocking: run
//! it in the background). Each run's worktrees are detached at the
//! project's HEAD in `<data>/worktrees/`, so no branches are created and
//! the user's working tree, index and stash are never touched until they
//! keep one: then its changes are applied to the (clean, or stashed) main
//! working tree as a patch, and the run's worktrees are removed.

use std::path::{Path, PathBuf};
use std::process::Command;

/// What one worktree changed against the run's base commit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    /// `git diff --binary` of everything (untracked files included).
    pub patch: String,
    /// `(path, +lines, −lines)`; binary files count 0 / 0.
    pub files: Vec<(String, usize, usize)>,
}

impl Diff {
    #[must_use]
    pub fn totals(&self) -> (usize, usize) {
        self.files
            .iter()
            .fold((0, 0), |(a, d), (_, add, del)| (a + add, d + del))
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!("git {}: {error}", args.join(" ")))
    }
}

/// `project` must be a repository's top folder with at least one commit;
/// returns HEAD's commit.
pub fn base_of(project: &Path) -> Result<String, String> {
    let top = git(project, &["rev-parse", "--show-toplevel"])
        .map_err(|_| "This project is not a git repository.".to_string())?;
    let top = PathBuf::from(top.trim());
    let same = |a: &Path, b: &Path| a.canonicalize().ok() == b.canonicalize().ok();
    if !same(&top, project) {
        return Err(format!(
            "Open the repository's top folder ({}) to run in parallel.",
            top.display()
        ));
    }
    git(project, &["rev-parse", "--verify", "HEAD^{commit}"])
        .map(|oid| oid.trim().to_string())
        .map_err(|_| "Commit something first: worktrees start from HEAD.".to_string())
}

/// Creates `count` worktrees of `project` at `base` under `dir`, named
/// `<prefix>-1`, `<prefix>-2`…. On failure the ones made are removed.
pub fn create(
    project: &Path,
    base: &str,
    dir: &Path,
    prefix: &str,
    count: usize,
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut made: Vec<PathBuf> = Vec::new();
    for i in 1..=count {
        let path = dir.join(format!("{prefix}-{i}"));
        let target = path.to_string_lossy().into_owned();
        if let Err(e) = git(project, &["worktree", "add", "--detach", &target, base]) {
            for path in &made {
                remove(project, path).ok();
            }
            return Err(e);
        }
        made.push(path.canonicalize().unwrap_or(path));
    }
    Ok(made)
}

/// Everything `worktree` changed since `base`, untracked files included
/// (they are staged in the worktree's own index to be diffed).
pub fn diff(worktree: &Path, base: &str) -> Result<Diff, String> {
    git(worktree, &["add", "-A"])?;
    let patch = git(worktree, &["diff", "--cached", "--binary", base])?;
    let numstat = git(worktree, &["diff", "--cached", "--numstat", base])?;
    let files = numstat
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let add = parts.next()?.parse().unwrap_or(0);
            let del = parts.next()?.parse().unwrap_or(0);
            Some((parts.next()?.to_string(), add, del))
        })
        .collect();
    Ok(Diff { patch, files })
}

/// Uncommitted changes in `project` (untracked files included), as
/// `git status --porcelain` lines.
pub fn dirty(project: &Path) -> Result<Vec<String>, String> {
    Ok(git(project, &["status", "--porcelain"])?
        .lines()
        .map(str::to_string)
        .collect())
}

/// Stashes everything uncommitted in `project` (untracked files included)
/// under `message`; returns the stash commit (`git stash apply <it>`
/// brings it back).
pub fn stash(project: &Path, message: &str) -> Result<String, String> {
    git(
        project,
        &["stash", "push", "--include-untracked", "-m", message],
    )?;
    let oid = git(project, &["rev-parse", "--verify", "refs/stash"])?;
    Ok(oid.trim().to_string())
}

/// Brings back the stash `oid` made by [`stash`] and drops it.
pub fn unstash(project: &Path, oid: &str) -> Result<(), String> {
    git(project, &["stash", "apply", "--index", oid])?;
    let list = git(project, &["stash", "list", "--format=%H"])?;
    if let Some(at) = list.lines().position(|line| line.trim() == oid) {
        git(project, &["stash", "drop", &format!("stash@{{{at}}}")])?;
    }
    Ok(())
}

/// Applies `patch` to `project`'s working tree (not its index). Checked
/// first: nothing is written when any part does not apply.
pub fn apply(project: &Path, patch: &str) -> Result<(), String> {
    if patch.trim().is_empty() {
        return Ok(());
    }
    let file = std::env::temp_dir().join(format!(
        "ferrum-parallel-{}-{}.patch",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::write(&file, patch).map_err(|e| e.to_string())?;
    let path = file.to_string_lossy().into_owned();
    let result = git(project, &["apply", "--check", "--binary", &path])
        .and_then(|_| git(project, &["apply", "--binary", &path]));
    std::fs::remove_file(&file).ok();
    result.map(|_| ())
}

/// Removes a worktree made by [`create`] (its changes are lost).
pub fn remove(project: &Path, worktree: &Path) -> Result<(), String> {
    let path = worktree.to_string_lossy().into_owned();
    let removed = git(project, &["worktree", "remove", "--force", &path]);
    git(project, &["worktree", "prune"]).ok();
    removed.map(|_| ())
}

/// Worktrees of `project` under `dir` (left by a run that did not finish).
pub fn leftovers(project: &Path, dir: &Path) -> Vec<PathBuf> {
    let Ok(list) = git(project, &["worktree", "list", "--porcelain"]) else {
        return Vec::new();
    };
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    list.lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .filter(|path| path.starts_with(&dir))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(["-c", "user.name=T", "-c", "user.email=t@e.x"])
            .args(args)
            .current_dir(dir)
            .status()
            .expect("git")
            .success();
        assert!(ok, "git {args:?}");
    }

    fn repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("repo");
        std::fs::create_dir_all(&root).expect("mkdir");
        let root = root.canonicalize().expect("canonical");
        std::fs::write(root.join("a.txt"), "one\ntwo\n").expect("write");
        run(&root, &["init", "-q"]);
        // `git stash` commits: it needs an author.
        run(&root, &["config", "user.name", "T"]);
        run(&root, &["config", "user.email", "t@e.x"]);
        run(&root, &["add", "."]);
        run(&root, &["commit", "-q", "-m", "one"]);
        (dir, root)
    }

    #[test]
    fn worktrees_diff_apply_and_go() {
        let (dir, root) = repo();
        let base = base_of(&root).expect("base");
        let trees = dir.path().join("worktrees");
        let made = create(&root, &base, &trees, "run", 2).expect("create");
        assert_eq!(made.len(), 2);
        assert_eq!(leftovers(&root, &trees).len(), 2);

        // Each worktree does its own thing; the main tree is untouched.
        std::fs::write(made[0].join("a.txt"), "ONE\ntwo\n").expect("write");
        std::fs::write(made[0].join("new.txt"), "fresh\n").expect("write");
        std::fs::write(made[1].join("a.txt"), "one\nTWO\nthree\n").expect("write");
        assert!(dirty(&root).expect("status").is_empty());
        let first = diff(&made[0], &base).expect("diff");
        let second = diff(&made[1], &base).expect("diff");
        assert_eq!(
            first.files,
            [("a.txt".to_string(), 1, 1), ("new.txt".to_string(), 1, 0)]
        );
        assert_eq!(second.totals(), (2, 1));

        // Keep the first.
        apply(&root, &first.patch).expect("apply");
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).expect("read"),
            "ONE\ntwo\n"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("new.txt")).expect("read"),
            "fresh\n"
        );
        for path in &made {
            remove(&root, path).expect("remove");
        }
        assert!(leftovers(&root, &trees).is_empty());
        assert!(!made[0].exists());
    }

    #[test]
    fn a_dirty_tree_is_stashed_and_given_back() {
        let (_dir, root) = repo();
        std::fs::write(root.join("a.txt"), "mine\n").expect("write");
        std::fs::write(root.join("notes.txt"), "untracked\n").expect("write");
        assert_eq!(dirty(&root).expect("status").len(), 2);
        let oid = stash(&root, "ferrum-parallel test").expect("stash");
        assert!(dirty(&root).expect("status").is_empty());

        // A patch that does not apply writes nothing.
        let bad =
            "diff --git a/zzz.txt b/zzz.txt\n--- a/zzz.txt\n+++ b/zzz.txt\n@@ -1 +1 @@\n-x\n+y\n";
        assert!(apply(&root, bad).is_err());
        assert!(dirty(&root).expect("status").is_empty());

        unstash(&root, &oid).expect("unstash");
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).expect("read"),
            "mine\n"
        );
        assert!(root.join("notes.txt").exists());
        assert!(
            Command::new("git")
                .args(["stash", "list"])
                .current_dir(&root)
                .output()
                .expect("git")
                .stdout
                .is_empty()
        );
    }

    #[test]
    fn only_a_repositorys_top_folder_with_a_commit_runs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let plain = dir.path().canonicalize().expect("canonical");
        assert!(base_of(&plain).is_err());
        run(&plain, &["init", "-q"]);
        assert!(base_of(&plain).expect_err("no commit").contains("Commit"));
        std::fs::create_dir_all(plain.join("sub")).expect("mkdir");
        std::fs::write(plain.join("sub/x"), "x").expect("write");
        run(&plain, &["add", "."]);
        run(&plain, &["commit", "-q", "-m", "x"]);
        assert!(base_of(&plain.join("sub")).is_err());
        assert!(base_of(&plain).is_ok());
    }
}
