//! Bisect from the commit menu: the session shows up in the repository
//! state (and its banner), marks move it on, Reset ends it.

use super::{git, host, init, wait_for};
use crate::components::actions::{Action, for_commit};
use crate::{GitView, GitViewOptions};
use ferrumgit_domain::{BisectMark, Oid, RepoOp};
use std::process::Command;

fn idle(v: &GitView) -> bool {
    v.state.active.is_some_and(|id| {
        let view = v.state.view(id);
        view.data.is_some() && !view.loading && !v.state.is_busy(id)
    })
}

fn remaining(v: &GitView) -> Option<usize> {
    let id = v.state.active?;
    v.state
        .view(id)
        .data?
        .snapshot
        .status
        .bisect
        .as_ref()?
        .remaining
}

#[gpui::test]
fn bisect_from_the_commit_menu(cx: &mut gpui::TestAppContext) {
    let data = tempfile::tempdir().expect("data dir");
    let repo = tempfile::tempdir().expect("repo");
    let dir = repo.path();
    git(dir, &["init", "-q", "-b", "main"]);
    for i in 1..=5 {
        std::fs::write(dir.join("n.txt"), i.to_string()).expect("write");
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", &format!("c{i}")]);
    }
    let out = Command::new("git")
        .args(["rev-list", "--max-parents=0", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("rev-list");
    let first = Oid::new(String::from_utf8_lossy(&out.stdout).trim());

    init(data.path(), cx);
    let (host, cx) = host(Some(dir.to_path_buf()), GitViewOptions::desktop(), cx);
    let view = host.read_with(cx, |h, _| h.git.clone());
    wait_for(cx, &view, idle);

    let menu = view.read_with(cx, |v, _| for_commit(&first, v.bisecting()));
    assert!(menu.contains(&Action::BisectStart(first.clone(), true)));
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.perform(Action::BisectStart(first.clone(), true), window, cx);
        });
    });
    wait_for(cx, &view, |v| idle(v) && remaining(v) == Some(4));
    assert!(view.read_with(cx, |v, _| v.bisecting()));
    let menu = view.read_with(cx, |v, _| for_commit(&first, v.bisecting()));
    assert!(menu.contains(&Action::BisectMark(BisectMark::Skip, first.clone())));

    view.update(cx, |v, cx| {
        let id = v.state.active.expect("active");
        v.op(
            id,
            RepoOp::BisectMark {
                mark: BisectMark::Good,
                rev: None,
            },
            cx,
        );
    });
    wait_for(cx, &view, |v| {
        idle(v) && remaining(v).is_some_and(|n| n < 4)
    });

    view.update(cx, |v, cx| {
        let id = v.state.active.expect("active");
        v.op(id, RepoOp::BisectReset, cx);
    });
    wait_for(cx, &view, |v| idle(v) && !v.bisecting());
}
