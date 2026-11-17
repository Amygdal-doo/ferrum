//! Headless test of Run in parallel: two worktrees with their own chats,
//! their diffs, and Keep this one on a project with uncommitted changes
//! (stashed first, never lost).

use std::sync::Arc;

use ferrumagent_ui::core::provider::mock::MockProvider;

use super::*;
use crate::parallel::Confirm;

#[gpui::test]
fn a_parallel_run_compares_worktrees_and_keeps_one(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let root = fx.path("a");
    std::fs::write(root.join("a.txt"), "alpha\nbeta\n").expect("write");
    git_in(&root, &["init", "-q"]);
    git_in(&root, &["config", "user.name", "T"]);
    git_in(&root, &["config", "user.email", "t@e.x"]);
    git_in(&root, &["add", "."]);
    git_in(&root, &["commit", "-q", "-m", "one"]);

    // Each agent just answers; the test plays their edits.
    let mock = MockProvider::new();
    mock.push_text("On it.");
    mock.push_text("On it.");
    let mut options = fx.options(Some(root.clone()));
    let home = fx.path("home");
    let provider = mock.clone();
    options.chat_options = std::rc::Rc::new(move |project: &Path| {
        let mut config = SessionConfig::isolated(project, &home);
        config.provider_override = Some(Arc::new(provider.clone()));
        ChatOptions {
            config: Some(config),
            ..Default::default()
        }
    });
    let (app, cx) = fx.open_with(cx, options);

    update(&app, cx, |a, w, cx| {
        a.parallel
            .task
            .update(cx, |input, cx| input.set_text("Make beta loud", cx));
        a.show_page(Page::Parallel, w, cx);
        a.start_parallel(w, cx);
    });
    wait(&app, cx, |a| a.parallel.run.is_some());
    let lanes: Vec<PathBuf> = read(&app, cx, |a| {
        let run = a.parallel.run.as_ref().expect("run");
        run.lanes.iter().map(|l| l.path.clone()).collect()
    });
    assert_eq!(lanes.len(), 2);
    for lane in &lanes {
        assert!(lane.join("a.txt").is_file(), "{}", lane.display());
        assert!(lane.starts_with(fx.path("data").join("worktrees")));
    }
    // Each worktree has its own chat, shown on the page.
    assert!(read(&app, cx, |a| lanes
        .iter()
        .all(|l| a.chats.contains_key(l))));
    assert!(cx.debug_bounds("page-Parallel").is_some());

    // The agents' work (played here), then their diffs.
    std::fs::write(lanes[0].join("a.txt"), "alpha\nBETA\n").expect("write");
    std::fs::write(lanes[0].join("loud.txt"), "LOUD\n").expect("write");
    std::fs::write(lanes[1].join("a.txt"), "alpha\nBeta!\n").expect("write");
    update(&app, cx, |a, _, cx| {
        a.refresh_lane(0, cx);
        a.refresh_lane(1, cx);
    });
    wait(&app, cx, |a| {
        a.parallel
            .run
            .as_ref()
            .is_some_and(|r| r.lanes.iter().all(|l| matches!(l.diff, Some(Ok(_)))))
    });
    let files = read(&app, cx, |a| {
        let run = a.parallel.run.as_ref().expect("run");
        let Some(Ok(diff)) = &run.lanes[0].diff else {
            panic!("no diff");
        };
        diff.files.clone()
    });
    assert_eq!(
        files,
        [("a.txt".to_string(), 1, 1), ("loud.txt".to_string(), 1, 0)]
    );
    // The project itself is untouched so far.
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        "alpha\nbeta\n"
    );

    // Uncommitted work in the project: Keep asks to stash it first.
    std::fs::write(root.join("mine.txt"), "my notes\n").expect("write");
    update(&app, cx, |a, _, cx| a.ask_keep(0, cx));
    wait(&app, cx, |a| a.parallel.confirm.is_some());
    let confirm = read(&app, cx, |a| a.parallel.confirm.clone()).expect("confirm");
    assert!(matches!(&confirm, Confirm::Keep { lane: 0, dirty } if dirty.len() == 1));
    update(&app, cx, |a, w, cx| a.answer_parallel(true, w, cx));
    wait(&app, cx, |a| {
        a.parallel.run.is_none() && a.parallel.busy.is_none()
    });

    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        "alpha\nBETA\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("loud.txt")).expect("read"),
        "LOUD\n"
    );
    // Stashed, not lost.
    assert!(!root.join("mine.txt").exists());
    let stashes = std::process::Command::new("git")
        .args(["stash", "list"])
        .current_dir(&root)
        .output()
        .expect("git");
    assert!(String::from_utf8_lossy(&stashes.stdout).contains("ferrum-parallel"));
    // The worktrees and their chats are gone.
    for lane in &lanes {
        assert!(!lane.exists());
    }
    assert!(read(&app, cx, |a| lanes
        .iter()
        .all(|l| !a.chats.contains_key(l))));
}
