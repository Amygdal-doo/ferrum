//! The folder pane's file-system side: what a snapshot reads and skips,
//! the changes and diffs against it, and repository discovery.

use super::diff::{Change, changes, file_diff};
use super::scan::{Limits, TooBig, find_repos, is_watched, skip_dir, snapshot, too_broad};
use ferrumgit_domain::{ChangeKind, LineKind};
use std::fs;
use std::path::Path;

fn write(root: &Path, path: &str, text: impl AsRef<[u8]>) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, text).expect("write");
}

fn keys(root: &Path) -> Vec<String> {
    let snap = snapshot(root, Limits::default(), None, None).expect("snapshot");
    snap.files.keys().cloned().collect()
}

#[test]
fn skips_heavy_and_hidden_folders() {
    for name in ["node_modules", "target", "dist", "build", ".git", ".cache"] {
        assert!(skip_dir(name), "{name}");
    }
    assert!(!skip_dir("src"));
    assert!(!skip_dir("targets"));
}

#[test]
fn a_snapshot_reads_text_files_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(root, "notes.md", "hello\n");
    write(root, "src/main.rs", "fn main() {}\n");
    write(root, "node_modules/x/index.js", "skip\n");
    write(root, "target/debug/out.txt", "skip\n");
    write(root, ".hidden/secret.txt", "skip\n");
    write(root, ".env", "KEY=1\n");
    write(root, "image.png", [0x89, b'P', b'N', b'G', 0, 0, 1]);
    write(root, "latin1.txt", [0xe9, b'\n']);
    let big = "x".repeat(2048);
    write(root, "big.log", &big);
    let limits = Limits {
        max_file: 1024,
        ..Limits::default()
    };
    let snap = snapshot(root, limits, None, None).expect("snapshot");
    let found: Vec<_> = snap.files.keys().map(String::as_str).collect();
    // Hidden files (not only folders) are read: `.env` is a file.
    assert_eq!(found, [".env", "notes.md", "src/main.rs"]);
    assert_eq!(&*snap.files["notes.md"].text, "hello\n");
}

#[test]
fn library_in_home_is_skipped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    write(&home, "Library/Caches/a.txt", "cache\n");
    write(&home, "project/a.txt", "code\n");
    let root = home.join("project");
    assert!(is_watched(&root, &root.join("a.txt"), Some(&home)));
    // Watching the home folder itself is refused, but a change report under
    // `~/Library` is never relevant.
    assert!(!is_watched(
        &home,
        &home.join("Library/Caches/a.txt"),
        Some(&home)
    ));
    assert!(too_broad(&home, Some(&home)));
    assert!(too_broad(Path::new("/"), None));
    assert!(!too_broad(&root, Some(&home)));
    assert_eq!(
        snapshot(&home, Limits::default(), Some(&home), None),
        Err(TooBig::Home)
    );
}

#[test]
fn watched_paths_skip_the_same_folders() {
    let root = Path::new("/work/site");
    assert!(is_watched(root, &root.join("index.html"), None));
    assert!(is_watched(root, &root.join("src/app/main.ts"), None));
    assert!(is_watched(root, &root.join(".env"), None));
    assert!(!is_watched(root, &root.join("node_modules/a/b.js"), None));
    assert!(!is_watched(root, &root.join("src/target/x"), None));
    assert!(!is_watched(root, &root.join(".git/index"), None));
    assert!(!is_watched(root, Path::new("/elsewhere/a.txt"), None));
}

#[test]
fn a_folder_over_the_limits_is_too_big() {
    let dir = tempfile::tempdir().expect("tempdir");
    for i in 0..5 {
        write(dir.path(), &format!("f{i}.txt"), "line\n");
    }
    let few = Limits {
        max_files: 3,
        ..Limits::default()
    };
    assert!(matches!(
        snapshot(dir.path(), few, None, None),
        Err(TooBig::Over { files: 4, .. })
    ));
    let small = Limits {
        max_bytes: 12,
        ..Limits::default()
    };
    assert!(matches!(
        snapshot(dir.path(), small, None, None),
        Err(TooBig::Over { .. })
    ));
    assert_eq!(keys(dir.path()).len(), 5);
}

#[test]
fn changes_since_the_baseline() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(root, "keep.txt", "same\n");
    write(root, "edit.txt", "one\ntwo\n");
    write(root, "gone.txt", "bye\n");
    let base = snapshot(root, Limits::default(), None, None).expect("baseline");
    write(root, "edit.txt", "one\n2\n");
    fs::remove_file(root.join("gone.txt")).expect("rm");
    write(root, "src/new.rs", "fn new() {}\n");
    write(root, "node_modules/ignored.js", "x\n");
    let now = snapshot(root, Limits::default(), None, Some(&base)).expect("rescan");
    let change = |path: &str, kind| Change {
        path: path.into(),
        kind,
    };
    assert_eq!(
        changes(&base, &now),
        [
            change("edit.txt", ChangeKind::Modified),
            change("gone.txt", ChangeKind::Deleted),
            change("src/new.rs", ChangeKind::Added),
        ]
    );
    // Unchanged files are reused, not read again.
    assert!(std::sync::Arc::ptr_eq(
        &base.files["keep.txt"].text,
        &now.files["keep.txt"].text
    ));
    assert!(changes(&now, &now).is_empty());
}

#[test]
fn a_diff_against_the_baseline() {
    let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\n";
    let new = "a\nb\nc\nd\nE\nf\ng\nh\ni\nj\n";
    let diff = file_diff("x.txt", Some(old), Some(new));
    assert_eq!(diff.change, ChangeKind::Modified);
    assert_eq!((diff.additions, diff.deletions), (2, 1));
    assert_eq!(diff.hunks.len(), 1);
    let hunk = &diff.hunks[0];
    assert_eq!((hunk.old_start, hunk.new_start), (2, 2));
    assert_eq!(hunk.header, "@@ -2,8 +2,9 @@");
    let removed = hunk
        .lines
        .iter()
        .find(|l| l.kind == LineKind::Removed)
        .expect("removed line");
    assert_eq!((removed.content.as_str(), removed.old_no), ("e", Some(5)));
    let added: Vec<_> = hunk
        .lines
        .iter()
        .filter(|l| l.kind == LineKind::Added)
        .map(|l| (l.content.as_str(), l.new_no))
        .collect();
    assert_eq!(added, [("E", Some(5)), ("j", Some(10))]);

    let created = file_diff("n.txt", None, Some("one\r\ntwo\r\n"));
    assert_eq!(created.change, ChangeKind::Added);
    assert_eq!(created.additions, 2);
    assert_eq!(created.hunks[0].lines[0].content, "one");
    let deleted = file_diff("d.txt", Some("x\n"), None);
    assert_eq!(
        (deleted.change, deleted.deletions),
        (ChangeKind::Deleted, 1)
    );
    assert!(
        file_diff("s.txt", Some("x\n"), Some("x\n"))
            .hunks
            .is_empty()
    );
}

#[test]
fn finds_repositories_below_a_folder() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    for repo in [
        "app",
        "code/lib",
        "code/deep/er/too-deep",
        "node_modules/pkg",
        ".config/x",
    ] {
        fs::create_dir_all(root.join(repo).join(".git")).expect("mkdir");
    }
    // A worktree or submodule: `.git` is a file.
    write(root, "code/worktree/.git", "gitdir: /elsewhere\n");
    // Nested inside a repository: not searched.
    fs::create_dir_all(root.join("app/vendor/inner/.git")).expect("mkdir");
    let found: Vec<_> = find_repos(root, 3, None)
        .into_iter()
        .map(|p| p.strip_prefix(root).expect("below").display().to_string())
        .collect();
    assert_eq!(found, ["app", "code/lib", "code/worktree"]);
    assert_eq!(find_repos(root, 1, None).len(), 1);
    assert!(find_repos(&root.join("missing"), 3, None).is_empty());
}

/// The embedded view on a folder without git, end to end: the baseline,
/// a change and its diff, opening a file, the repositories below and
/// picking one, and back.
#[gpui::test]
fn an_embedded_view_on_a_folder_without_git(cx: &mut gpui::TestAppContext) {
    use crate::tests::{canonical, git, host, init, wait_for};
    use crate::{GitViewEvent, GitViewOptions};

    let data = tempfile::tempdir().expect("data dir");
    let dir = tempfile::tempdir().expect("folder");
    let root = canonical(dir.path());
    write(&root, "notes.txt", "one\n");
    let repo = root.join("proj");
    fs::create_dir_all(&repo).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    init(data.path(), cx);
    let (host, cx) = host(Some(root.clone()), GitViewOptions::embedded(), cx);
    let view = host.read_with(cx, |h, _| h.git.clone());
    wait_for(cx, &view, |v| {
        v.folder.as_ref().is_some_and(|f| {
            matches!(f.baseline, super::Baseline::Ready { .. }) && f.repos.is_some()
        })
    });
    view.read_with(cx, |v, _| {
        assert!(v.in_folder());
        let folder = v.folder.as_ref().expect("folder");
        assert_eq!(folder.page, super::FolderPage::Changes);
        assert_eq!(folder.repos.as_deref(), Some(&[repo.clone()][..]));
        assert!(folder.changes.is_empty());
    });

    // A change shows after the next rescan, with its diff.
    write(&root, "notes.txt", "one\ntwo\n");
    write(&root, "new.txt", "new\n");
    view.update(cx, |v, cx| {
        v.reload_folder(cx);
        let generation = v.folder.as_ref().expect("folder").generation;
        assert!(v.folder_tick(generation, cx));
    });
    wait_for(cx, &view, |v| {
        v.folder.as_ref().is_some_and(|f| f.changes.len() == 2)
    });
    view.update(cx, |v, cx| {
        v.select_folder_file("notes.txt".into(), cx);
        let diff = v
            .folder
            .as_ref()
            .and_then(|f| f.diff.clone())
            .expect("diff");
        assert_eq!((diff.additions, diff.deletions), (1, 0));
        v.open_folder_file("notes.txt", Some(2), cx);
        v.show_history(cx);
        let page = v.folder.as_ref().expect("folder").page;
        assert_eq!(page, super::FolderPage::Repositories);
    });
    cx.run_until_parked();
    host.read_with(cx, |h, _| {
        assert!(h.events.contains(&GitViewEvent::OpenFile {
            path: root.join("notes.txt"),
            line: Some(2),
        }));
    });

    // Picking a repository below shows it; back returns to the folder.
    view.update(cx, |v, cx| v.pick_repo(repo.clone(), cx));
    wait_for(cx, &view, |v| v.state.active.is_some());
    view.read_with(cx, |v, _| {
        assert!(!v.in_folder());
        assert_eq!(v.repo_path().map(|p| canonical(&p)), Some(repo.clone()));
    });
    view.update(cx, |v, cx| v.back_to_folder(cx));
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        assert!(v.in_folder() && v.state.open.is_empty());
    });

    // A repository given by the host leaves the folder.
    view.update(cx, |v, cx| v.set_repo(&repo, cx));
    wait_for(cx, &view, |v| v.state.active.is_some());
    view.read_with(cx, |v, _| assert!(v.folder.is_none()));
}
