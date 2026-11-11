//! Git integration for the gutter and status bar: line marks against HEAD,
//! repository discovery and the current branch. Uses the `git` CLI only to
//! read HEAD's copy of a file (like FerrumGIT, no libgit2).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use similar::{Algorithm, DiffOp, capture_diff_slices};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineMark {
    Added,
    Modified,
    /// Lines were deleted just above this line (at end of file: the last line).
    Deleted,
}

/// Per-line marks of `current` compared with `base`, sorted by 0-based line.
/// Lines are compared without their terminators (`\n` / `\r\n`).
pub fn diff_marks(base: &str, current: &str) -> Vec<(usize, LineMark)> {
    let old = split_lines(base);
    let new = split_lines(current);
    let new_count = new.len();
    let mut marks: Vec<(usize, LineMark)> = Vec::new();
    let mut pending_delete: Option<(usize, usize)> = None;

    let flush_delete = |pending: &mut Option<(usize, usize)>,
                        marks: &mut Vec<(usize, LineMark)>| {
        if let Some((at, _)) = pending.take()
            && new_count > 0
        {
            marks.push((at.min(new_count - 1), LineMark::Deleted));
        }
    };

    for op in capture_diff_slices(Algorithm::Myers, &old, &new) {
        match op {
            DiffOp::Equal { .. } => flush_delete(&mut pending_delete, &mut marks),
            DiffOp::Delete {
                old_len, new_index, ..
            } => {
                flush_delete(&mut pending_delete, &mut marks);
                pending_delete = Some((new_index, old_len));
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                let deleted = match pending_delete.take() {
                    Some((at, len)) if at == new_index => len,
                    Some(other) => {
                        let mut stale = Some(other);
                        flush_delete(&mut stale, &mut marks);
                        0
                    }
                    None => 0,
                };
                push_replace(&mut marks, new_index, new_len, deleted);
            }
            DiffOp::Replace {
                old_len,
                new_index,
                new_len,
                ..
            } => {
                flush_delete(&mut pending_delete, &mut marks);
                push_replace(&mut marks, new_index, new_len, old_len);
            }
        }
    }
    flush_delete(&mut pending_delete, &mut marks);
    marks.sort_by_key(|(line, _)| *line);
    marks.dedup_by_key(|(line, _)| *line);
    marks
}

fn push_replace(marks: &mut Vec<(usize, LineMark)>, start: usize, new_len: usize, old_len: usize) {
    for i in 0..new_len {
        let mark = if i < old_len {
            LineMark::Modified
        } else {
            LineMark::Added
        };
        marks.push((start + i, mark));
    }
}

fn split_lines(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

/// The working-tree root containing `path` (a file or folder), found by
/// walking up to the first `.git` directory or file.
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
    let target = contents.trim().strip_prefix("gitdir:")?.trim();
    let target = PathBuf::from(target);
    Some(if target.is_absolute() {
        target
    } else {
        repo_root.join(target)
    })
}

/// Branch name from HEAD, or the short commit id when detached.
pub fn current_branch(repo_root: &Path) -> Option<String> {
    let head = std::fs::read_to_string(git_dir(repo_root)?.join("HEAD")).ok()?;
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref:") {
        let reference = reference.trim();
        return Some(
            reference
                .strip_prefix("refs/heads/")
                .unwrap_or(reference)
                .to_string(),
        );
    }
    (head.len() >= 7 && head.chars().all(|c| c.is_ascii_hexdigit())).then(|| head[..7].to_string())
}

/// HEAD's version of `file`; `None` when git is missing, the file is not in
/// HEAD, or its contents are not UTF-8.
pub fn head_contents(repo_root: &Path, file: &Path) -> Option<String> {
    let relative = relative_path(repo_root, file)?;
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["--no-optional-locks", "-c", "core.quotepath=off", "show"])
        .arg(format!("HEAD:{relative}"))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// `file` relative to `repo_root` with `/` separators.
fn relative_path(repo_root: &Path, file: &Path) -> Option<String> {
    let relative = match file.strip_prefix(repo_root) {
        Ok(relative) => relative.to_path_buf(),
        Err(_) => {
            let root = repo_root.canonicalize().ok()?;
            let file = file.canonicalize().ok()?;
            file.strip_prefix(root).ok()?.to_path_buf()
        }
    };
    let parts: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use LineMark::*;

    #[test]
    fn identical_text_has_no_marks() {
        assert!(diff_marks("a\nb\n", "a\nb\n").is_empty());
        assert!(diff_marks("a\r\nb", "a\nb").is_empty());
    }

    #[test]
    fn added_lines() {
        assert_eq!(diff_marks("a\nc", "a\nb\nc"), vec![(1, Added)]);
        assert_eq!(diff_marks("", "x\ny"), vec![(0, Modified), (1, Added)]);
    }

    #[test]
    fn modified_lines() {
        assert_eq!(diff_marks("a\nb\nc", "a\nB\nc"), vec![(1, Modified)]);
        assert_eq!(
            diff_marks("a\nb\nc", "a\nB\nB2\nc"),
            vec![(1, Modified), (2, Added)]
        );
    }

    #[test]
    fn deleted_lines_mark_the_line_below() {
        assert_eq!(diff_marks("a\nb\nc", "a\nc"), vec![(1, Deleted)]);
        assert_eq!(diff_marks("a\nb\nc\nd", "a\nd"), vec![(1, Deleted)]);
    }

    #[test]
    fn deletion_at_end_marks_last_line() {
        assert_eq!(diff_marks("a\nb\nc", "a\nb"), vec![(1, Deleted)]);
    }

    #[test]
    fn deletion_at_start() {
        assert_eq!(diff_marks("x\na\nb", "a\nb"), vec![(0, Deleted)]);
    }

    #[test]
    fn mixed_changes_are_sorted() {
        let marks = diff_marks("1\n2\n3\n4\n5", "0\n1\n2x\n3\n5");
        assert_eq!(marks, vec![(0, Added), (2, Modified), (4, Deleted)]);
    }

    #[test]
    fn repo_root_and_branch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join(".git")).expect("mkdir");
        std::fs::create_dir_all(root.join("src/deep")).expect("mkdir");
        std::fs::write(root.join(".git/HEAD"), "ref: refs/heads/feature/x\n").expect("write");
        assert_eq!(repo_root(&root.join("src/deep")), Some(root.to_path_buf()));
        assert_eq!(current_branch(root), Some("feature/x".to_string()));

        std::fs::write(
            root.join(".git/HEAD"),
            "0123456789abcdef0123456789abcdef01234567\n",
        )
        .expect("write");
        assert_eq!(current_branch(root), Some("0123456".to_string()));
    }

    #[test]
    fn gitdir_file_is_followed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let real = dir.path().join("real-git");
        let tree = dir.path().join("tree");
        std::fs::create_dir_all(&real).expect("mkdir");
        std::fs::create_dir_all(&tree).expect("mkdir");
        std::fs::write(real.join("HEAD"), "ref: refs/heads/main").expect("write");
        std::fs::write(tree.join(".git"), "gitdir: ../real-git\n").expect("write");
        assert_eq!(repo_root(&tree), Some(tree.clone()));
        assert_eq!(current_branch(&tree), Some("main".to_string()));
    }

    #[test]
    fn no_repo() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(current_branch(dir.path()), None);
        assert_eq!(
            relative_path(dir.path(), &dir.path().join("a/b.rs")).as_deref(),
            Some("a/b.rs")
        );
    }
}
