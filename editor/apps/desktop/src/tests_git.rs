//! The Git panel and view against a real repository in a temp folder:
//! status, staging, the diff, commit, and the Editor | Git switch.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use gpui::{Entity, TestAppContext, VisualTestContext};

use crate::tests::{init, primary};
use crate::workspace::Workspace;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(status.status.success(), "git {args:?}: {status:?}");
}

fn wait_until(
    cx: &mut VisualTestContext,
    what: &str,
    mut done: impl FnMut(&mut VisualTestContext) -> bool,
) {
    for _ in 0..500 {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        cx.executor().advance_clock(Duration::from_millis(50));
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {what}");
}

fn changes(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> usize {
    workspace.read_with(cx, |w, cx| w.git_state().read(cx).change_count())
}

#[gpui::test]
fn git_panel_stages_commits_and_shows_diffs(cx: &mut TestAppContext) {
    if Command::new("git").arg("--version").output().is_err() {
        eprintln!("skipping: git is not installed");
        return;
    }
    // git runs on its own runtime threads.
    cx.executor().allow_parking();
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.name", "Ferrum Test"]);
    git(&root, &["config", "user.email", "test@example.com"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    std::fs::write(root.join("a.txt"), "one\ntwo\n").expect("write");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "first"]);
    std::fs::write(root.join("a.txt"), "one\n2\n").expect("write");
    std::fs::write(root.join("new.txt"), "hello\n").expect("write");

    let (workspace, cx) = cx.add_window_view(Workspace::new);
    workspace.update_in(cx, |w, window, cx| w.set_root(root.clone(), window, cx));
    wait_until(cx, "status", |cx| changes(&workspace, cx) == 2);
    workspace.read_with(cx, |w, cx| {
        let git = w.git_state().read(cx);
        assert_eq!(git.branch_label().as_deref(), Some("main"));
        assert_eq!(git.commits.len(), 1);
    });

    // Ctrl+Shift+G shows the panel; Cmd+Alt+G switches to the Git view.
    cx.simulate_keystrokes("ctrl-shift-g");
    assert!(workspace.read_with(cx, |w, _| w.sidebar_shows_git()));
    cx.simulate_keystrokes(&primary("cmd-alt-g"));
    assert!(workspace.read_with(cx, |w, _| w.in_git_view()));

    // Selecting a file shows its diff.
    let git_state = workspace.read_with(cx, |w, _| w.git_state().clone());
    git_state.update(cx, |g, cx| g.select_file("a.txt", false, cx));
    wait_until(cx, "diff", |cx| {
        git_state.read_with(cx, |g, _| !g.diff.is_empty())
    });
    git_state.read_with(cx, |g, _| {
        let texts: Vec<&str> = g.diff.iter().map(|r| r.text.as_ref()).collect();
        assert!(texts.contains(&"two") && texts.contains(&"2"), "{texts:?}");
    });

    // Stage one file and commit it.
    git_state.update(cx, |g, cx| g.stage("a.txt", cx));
    wait_until(cx, "staged", |cx| {
        git_state.read_with(cx, |g, _| g.status.staged().len() == 1 && g.busy.is_none())
    });
    git_state.update(cx, |g, cx| {
        g.message.update(cx, |m, cx| m.set_text("Change a", cx));
        g.commit(cx);
    });
    wait_until(cx, "commit", |cx| {
        git_state.read_with(cx, |g, _| g.commits.len() == 2 && g.busy.is_none())
    });
    git_state.read_with(cx, |g, cx| {
        assert_eq!(g.commits[0].summary, "Change a");
        assert_eq!(g.change_count(), 1, "new.txt is still untracked");
        assert!(g.message.read(cx).text().is_empty());
    });

    // Nothing staged: Commit commits everything.
    git_state.update(cx, |g, cx| {
        g.message.update(cx, |m, cx| m.set_text("Add new", cx));
        g.commit(cx);
    });
    wait_until(cx, "commit all", |cx| {
        git_state.read_with(cx, |g, _| g.commits.len() == 3 && g.change_count() == 0)
    });

    // Back to the editor.
    cx.simulate_keystrokes(&primary("cmd-alt-g"));
    assert!(!workspace.read_with(cx, |w, _| w.in_git_view()));
}

#[gpui::test]
fn a_folder_without_git_says_so(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    workspace.update_in(cx, |w, window, cx| w.set_root(root.clone(), window, cx));
    wait_until(cx, "not a repo", |cx| {
        workspace.read_with(cx, |w, cx| w.git_state().read(cx).not_a_repo)
    });
}
