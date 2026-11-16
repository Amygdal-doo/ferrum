//! The interactive rebase dialog: opened from a commit, steered with the
//! keyboard, then run through git.

use super::{git, host, init, wait_for};
use crate::components::dialogs::DialogState;
use crate::{GitView, GitViewOptions};
use ferrumgit_domain::{Oid, RebaseAction};
use std::path::Path;
use std::process::Command;

fn log(dir: &Path, format: &str) -> Vec<String> {
    let out = Command::new("git")
        .args(["log", &format!("--format={format}")])
        .current_dir(dir)
        .output()
        .expect("git log");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

fn idle(v: &GitView) -> bool {
    v.state.active.is_some_and(|id| {
        let view = v.state.view(id);
        view.data.is_some() && !view.loading && !v.state.is_busy(id)
    })
}

fn steps(v: &GitView) -> Option<Vec<(String, RebaseAction)>> {
    match &v.dialog {
        Some(DialogState::Rebase(f)) => Some(
            f.plan
                .steps
                .iter()
                .map(|s| (s.summary().to_owned(), s.action))
                .collect(),
        ),
        _ => None,
    }
}

#[gpui::test]
fn keyboard_reorder_and_squash_then_rebase(cx: &mut gpui::TestAppContext) {
    let data = tempfile::tempdir().expect("data dir");
    let repo = tempfile::tempdir().expect("repo");
    let dir = repo.path();
    git(dir, &["init", "-q", "-b", "main"]);
    for name in ["base", "A", "B", "C"] {
        std::fs::write(dir.join(format!("{name}.txt")), name).expect("write");
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", name]);
    }
    let base = Oid::new(log(dir, "%H")[3].clone());

    init(data.path(), cx);
    let (host, cx) = host(Some(dir.to_path_buf()), GitViewOptions::desktop(), cx);
    let view = host.read_with(cx, |h, _| h.git.clone());
    wait_for(cx, &view, idle);

    cx.update(|window, cx| {
        view.update(cx, |v, cx| v.open_interactive_rebase(base, window, cx));
    });
    wait_for(cx, &view, |v| steps(v).is_some());
    assert_eq!(
        view.read_with(cx, |v, _| steps(v)).expect("steps"),
        [
            ("A".to_owned(), RebaseAction::Pick),
            ("B".to_owned(), RebaseAction::Pick),
            ("C".to_owned(), RebaseAction::Pick),
        ]
    );

    // A moves below B and is squashed into it.
    cx.simulate_keystrokes("alt-down s");
    assert_eq!(
        view.read_with(cx, |v, _| steps(v)).expect("steps"),
        [
            ("B".to_owned(), RebaseAction::Pick),
            ("A".to_owned(), RebaseAction::Squash),
            ("C".to_owned(), RebaseAction::Pick),
        ]
    );

    cx.update(|window, cx| view.update(cx, |v, cx| v.submit_dialog(window, cx)));
    wait_for(cx, &view, |v| v.dialog.is_none() && idle(v));
    wait_for(cx, &view, |_| log(dir, "%s").len() == 3);
    assert_eq!(log(dir, "%s"), ["C", "B", "base"]);
    assert!(
        log(dir, "%B").iter().any(|l| l == "A"),
        "A's message is kept"
    );
}
