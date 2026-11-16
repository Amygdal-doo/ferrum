//! The three-way conflict view during a merge: open, choose per conflict
//! (rendered in the window), mark resolved.

use super::{git, host, init, wait_for};
use crate::state::Page;
use crate::{GitView, GitViewOptions};
use ferrumgit_domain::ConflictChoice;
use std::path::Path;
use std::process::Command;

fn merge_conflict(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("f.txt"), "top\nshared\nend\n").expect("write");
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "base"]);
    git(dir, &["switch", "-q", "-c", "topic"]);
    std::fs::write(dir.join("f.txt"), "top\ntheirs\nend\n").expect("write");
    git(dir, &["commit", "-q", "-am", "topic"]);
    git(dir, &["switch", "-q", "main"]);
    std::fs::write(dir.join("f.txt"), "top\nours\nend\n").expect("write");
    git(dir, &["commit", "-q", "-am", "main"]);
    let merge = Command::new("git")
        .args(["merge", "-q", "topic"])
        .current_dir(dir)
        .output()
        .expect("merge");
    assert!(!merge.status.success(), "the merge conflicts");
}

fn conflicted(v: &GitView) -> usize {
    v.state
        .active
        .and_then(|id| v.state.view(id).data)
        .map_or(usize::MAX, |d| d.snapshot.status.conflicted().len())
}

#[gpui::test]
fn resolves_a_merge_conflict_in_three_columns(cx: &mut gpui::TestAppContext) {
    let data = tempfile::tempdir().expect("data dir");
    let repo = tempfile::tempdir().expect("repo");
    let dir = repo.path();
    merge_conflict(dir);

    init(data.path(), cx);
    let (host, cx) = host(Some(dir.to_path_buf()), GitViewOptions::desktop(), cx);
    let view = host.read_with(cx, |h, _| h.git.clone());
    wait_for(cx, &view, |v| conflicted(v) == 1);

    view.update(cx, |v, cx| {
        let id = v.state.active.expect("active");
        v.set_page(id, Page::WorkingCopy, cx);
        v.open_conflict(id, "f.txt".into(), cx);
    });
    wait_for(cx, &view, |v| {
        v.state
            .active
            .is_some_and(|id| v.state.view(id).conflict.is_some())
    });
    // Drawn with the choice in place.
    view.update(cx, |v, cx| {
        let id = v.state.active.expect("active");
        v.choose(id, 0, ConflictChoice::Both, cx);
    });
    cx.run_until_parked();
    view.update(cx, |v, cx| {
        let id = v.state.active.expect("active");
        assert_eq!(v.state.view(id).conflict.expect("open").resolved(), 1);
        v.mark_conflict_resolved(id, cx);
    });
    wait_for(cx, &view, |v| conflicted(v) == 0);
    assert_eq!(
        std::fs::read_to_string(dir.join("f.txt")).expect("read"),
        "top\nours\ntheirs\nend\n"
    );
    view.read_with(cx, |v, _| {
        let id = v.state.active.expect("active");
        assert!(v.state.view(id).conflict.is_none());
    });
}
