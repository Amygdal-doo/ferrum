//! "New worktree…": a git worktree for a branch, next to the repository.

use std::path::{Path, PathBuf};
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// `feature/login` → `feature-login`.
#[must_use]
pub fn folder_suffix(branch: &str) -> String {
    branch
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

/// Creates `<repo>-<branch>` beside the repository holding `cwd`, checking
/// out `branch` (created from HEAD when it doesn't exist). Returns its path.
pub fn create(cwd: &Path, branch: &str) -> Result<PathBuf, String> {
    let suffix = folder_suffix(branch);
    if suffix.is_empty() {
        return Err("Type a branch name".into());
    }
    let top = PathBuf::from(
        git(cwd, &["rev-parse", "--show-toplevel"])
            .map_err(|_| format!("{} is not inside a git repository", cwd.display()))?,
    );
    let repo = top
        .file_name()
        .map_or_else(|| "repo".into(), |n| n.to_string_lossy().to_string());
    let target = top
        .parent()
        .unwrap_or(&top)
        .join(format!("{repo}-{suffix}"));
    if target.exists() {
        return Err(format!("{} already exists", target.display()));
    }
    let target_str = target.display().to_string();
    let exists = git(
        &top,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_ok();
    let result = if exists {
        git(&top, &["worktree", "add", &target_str, branch])
    } else {
        git(&top, &["worktree", "add", "-b", branch, &target_str])
    };
    result.map(|_| target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names() {
        assert_eq!(folder_suffix("feature/login"), "feature-login");
        assert_eq!(folder_suffix("  "), "");
        assert_eq!(folder_suffix("fix_1.2"), "fix_1.2");
    }

    #[test]
    fn creates_a_worktree_for_a_new_branch() {
        let dir = tempfile::tempdir().expect("tempdir");
        // git reports /private/var for macOS temp folders.
        let root = dir.path().canonicalize().expect("canonical");
        let repo = root.join("app");
        std::fs::create_dir(&repo).expect("mkdir");
        let ok = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .arg("-C")
                    .arg(&repo)
                    .args(args)
                    .output()
                    .expect("git")
                    .status
                    .success(),
                "{args:?}"
            );
        };
        ok(&["init", "-q"]);
        ok(&[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "init",
        ]);
        let made = create(&repo, "feature/x").expect("worktree");
        assert_eq!(made, root.join("app-feature-x"));
        assert!(made.join(".git").exists());
        assert!(
            create(&repo, "feature/x").is_err(),
            "second time: folder exists"
        );
    }
}
