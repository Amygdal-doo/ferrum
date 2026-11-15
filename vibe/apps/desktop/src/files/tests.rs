//! The Files pane's model: order, ignore rules, expand state, git marks
//! and the paths a drag carries.

use std::fs;
use std::path::{Path, PathBuf};

use super::tree::{Entry, sort_entries};
use super::*;

/// A project with a git folder, an ignored build folder and mixed-case
/// names.
fn sample() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    for sub in ["src/nested", "Docs", ".git", "target", "node_modules/x"] {
        fs::create_dir_all(root.join(sub)).expect("mkdir");
    }
    for (file, text) in [
        ("b.txt", ""),
        ("A.txt", ""),
        (".env", ""),
        (".DS_Store", ""),
        (".gitignore", "target/\nnode_modules\n*.log\n"),
        ("debug.log", ""),
        ("src/main.rs", ""),
        ("src/trace.log", ""),
        ("src/nested/deep.rs", ""),
        ("target/junk", ""),
    ] {
        fs::write(root.join(file), text).expect("write");
    }
    dir
}

fn names(tree: &FileTree) -> Vec<(usize, String)> {
    tree.rows().into_iter().map(|r| (r.depth, r.name)).collect()
}

fn top(tree: &FileTree) -> Vec<String> {
    tree.rows()
        .into_iter()
        .filter(|r| r.depth == 0)
        .map(|r| r.name)
        .collect()
}

#[test]
fn folders_come_first_then_names_ignoring_case() {
    let mut entries: Vec<Entry> = [("b", false), ("Z", true), ("a", true), ("A", false)]
        .into_iter()
        .map(|(name, is_dir)| Entry {
            path: PathBuf::from(name),
            name: name.into(),
            is_dir,
        })
        .collect();
    sort_entries(&mut entries);
    let order: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(order, ["a", "Z", "A", "b"]);
}

#[test]
fn gitignored_files_dot_git_and_ds_store_are_hidden() {
    let dir = sample();
    let tree = FileTree::new(dir.path());
    assert_eq!(
        top(&tree),
        ["Docs", "src", ".env", ".gitignore", "A.txt", "b.txt"]
    );
}

#[test]
fn ignore_rules_of_the_root_apply_in_subfolders() {
    let dir = sample();
    let mut tree = FileTree::new(dir.path());
    tree.expand(&dir.path().join("src"));
    let all = names(&tree);
    assert!(all.contains(&(1, "main.rs".into())));
    assert!(!all.iter().any(|(_, n)| n == "trace.log"), "{all:?}");
}

#[test]
fn folders_are_read_when_first_expanded_and_keep_their_state() {
    let dir = sample();
    let (src, nested) = (dir.path().join("src"), dir.path().join("src/nested"));
    let mut tree = FileTree::new(dir.path());
    assert!(!tree.is_loaded(&src), "lazy");
    tree.toggle(&src);
    assert!(tree.is_loaded(&src) && tree.is_expanded(&src));
    tree.expand(&nested);
    assert!(names(&tree).contains(&(2, "deep.rs".into())));
    // Collapsing the parent hides the child but remembers it is open.
    tree.toggle(&src);
    assert!(!names(&tree).iter().any(|(_, n)| n == "deep.rs"));
    tree.toggle(&src);
    assert!(names(&tree).contains(&(2, "deep.rs".into())));
    tree.collapse_all();
    assert_eq!(tree.rows().iter().filter(|r| r.depth > 0).count(), 0);
    // The root never collapses.
    tree.collapse(dir.path());
    assert!(!tree.rows().is_empty());
}

#[test]
fn refresh_reads_changes_and_forgets_removed_folders() {
    let dir = sample();
    let src = dir.path().join("src");
    let mut tree = FileTree::new(dir.path());
    tree.expand(&src);
    tree.expand(&src.join("nested"));
    fs::write(src.join("new.rs"), "").expect("write");
    fs::remove_dir_all(src.join("nested")).expect("rm");
    tree.refresh();
    let all = names(&tree);
    assert!(all.contains(&(1, "new.rs".into())));
    assert!(!all.iter().any(|(_, n)| n == "nested" || n == "deep.rs"));
    assert!(!tree.is_expanded(&src.join("nested")));
}

#[test]
fn git_status_marks_files_and_the_folders_above_them() {
    let repo = Path::new("/r");
    let output = [
        " M src/main.rs",
        "A  src/new.rs",
        "R  src/renamed.rs",
        "src/old.rs",
        "?? scratch/",
        "UU conflict.txt",
        " D gone/file.rs",
        "!! ignored.log",
        "",
    ]
    .join("\0");
    let marks = GitMarks::parse(repo, &output);
    let status = |p: &str, dir| marks.status(&repo.join(p), dir);
    assert_eq!(status("src/main.rs", false), Some(FileStatus::Modified));
    assert_eq!(status("src/new.rs", false), Some(FileStatus::Added));
    assert_eq!(status("src/renamed.rs", false), Some(FileStatus::Modified));
    assert_eq!(status("src/old.rs", false), None, "a rename's source");
    assert_eq!(status("src", true), Some(FileStatus::Modified));
    // Files inside an untracked folder.
    assert_eq!(status("scratch", true), Some(FileStatus::Untracked));
    assert_eq!(status("scratch/a/b.rs", false), Some(FileStatus::Untracked));
    assert_eq!(status("conflict.txt", false), Some(FileStatus::Conflict));
    assert_eq!(status("gone", true), Some(FileStatus::Deleted));
    assert_eq!(status("ignored.log", false), None);
    assert_eq!(status("README.md", false), None);
    // The root shows the strongest status of all.
    assert_eq!(marks.status(repo, true), Some(FileStatus::Conflict));
}

#[test]
fn git_status_is_read_from_a_real_repository() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .args(["-c", "user.name=T", "-c", "user.email=t@e.x"])
            .args(args)
            .current_dir(&root)
            .status()
            .expect("git")
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    fs::create_dir_all(root.join("src")).expect("mkdir");
    fs::write(root.join("src/a.rs"), "1").expect("write");
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "one"]);
    fs::write(root.join("src/a.rs"), "2").expect("write");
    fs::write(root.join("new.txt"), "").expect("write");
    let marks = GitMarks::read(&root);
    assert_eq!(
        marks.status(&root.join("src/a.rs"), false),
        Some(FileStatus::Modified)
    );
    assert_eq!(
        marks.status(&root.join("src"), true),
        Some(FileStatus::Modified)
    );
    assert_eq!(
        marks.status(&root.join("new.txt"), false),
        Some(FileStatus::Untracked)
    );
    // Outside a repository: nothing, no error.
    let plain = tempfile::tempdir().expect("tempdir");
    assert!(GitMarks::read(plain.path()).is_empty());
}

#[test]
fn nested_and_repeated_drops_are_merged() {
    let p = |s: &str| PathBuf::from(s);
    assert_eq!(
        without_nested(&[p("/a/src/x.rs"), p("/a/src"), p("/a/b.txt"), p("/a/b.txt")]),
        [p("/a/src"), p("/a/b.txt")]
    );
    // `/a/srcx` is not inside `/a/src`.
    assert_eq!(
        without_nested(&[p("/a/src"), p("/a/srcx")]),
        [p("/a/src"), p("/a/srcx")]
    );
    assert!(without_nested(&[]).is_empty());
}
