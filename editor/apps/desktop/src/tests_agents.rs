//! AI agent sessions end to end on a real git repository: a fake agent
//! (a `/bin/sh` script from `[[agents]]`, with an API key in `env`) edits
//! two places of an open file and adds a file; the review lists them, one
//! hunk is accepted and the other rejected (the open buffer follows, and
//! undo brings the agent's version back), the new file is rejected, and a
//! stale action is refused.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use ferrumeditor_core::agents::review::FileStatus;
use ferrumeditor_core::config::{AgentConfig, SecretEnv};
use gpui::{Entity, TestAppContext, VisualTestContext};

use crate::agents::{AgentSessions, ReviewAction};
use crate::settings::Settings;
use crate::terminal::test_support;
use crate::tests::{init, primary};
use crate::workspace::Workspace;

const BASE: &str = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\n";

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(output.status.success(), "git {args:?}: {output:?}");
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

fn agents(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<AgentSessions> {
    workspace.read_with(cx, |w, _| w.agents().clone())
}

#[gpui::test]
fn fake_agent_edits_are_reviewed_hunk_by_hunk(cx: &mut TestAppContext) {
    if Command::new("git").arg("--version").output().is_err() {
        eprintln!("skipping: git is not installed");
        return;
    }
    // Snapshots and scans run on background threads.
    cx.executor().allow_parking();
    init(cx);
    test_support::reset();
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.name", "Ferrum Test"]);
    git(&root, &["config", "user.email", "test@example.com"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    std::fs::write(root.join("a.txt"), BASE).expect("write");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "first"]);

    // The agent: rewrites lines 2 and 11, adds a file, and proves it got
    // its key (written outside the project).
    let tools = tempfile::tempdir().expect("tempdir");
    let key_out = tools.path().join("key.out");
    let script = tools.path().join("fake-agent");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf 'a\\nB\\nc\\nd\\ne\\nf\\ng\\nh\\ni\\nj\\nK\\nl\\n' > a.txt\nprintf 'hello\\n' > new.txt\nprintf '%s' \"$FAKE_API_KEY\" > '{}'\necho agent done\n",
            key_out.display()
        ),
    )
    .expect("write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let mut env = std::collections::BTreeMap::new();
    env.insert("FAKE_API_KEY".to_string(), "sk-test-123".to_string());
    cx.update(|cx| {
        cx.global_mut::<Settings>().config.agents = vec![AgentConfig {
            name: "fake".into(),
            command: Some(script.to_string_lossy().to_string()),
            env: SecretEnv(env),
            ..Default::default()
        }];
    });

    let (workspace, cx) = cx.add_window_view(Workspace::new);
    workspace.update_in(cx, |w, window, cx| {
        w.set_root(root.clone(), window, cx);
        w.open_file(&root.join("a.txt"), window, cx);
    });
    cx.run_until_parked();
    let editor = workspace
        .read_with(cx, |w, _| w.active_editor())
        .expect("editor");

    // ⌥⌘A shows the Agents panel.
    cx.simulate_keystrokes(&primary("cmd-alt-a"));
    assert!(workspace.read_with(cx, |w, _| w.right_panel().is_some()));

    let agent = cx
        .update(|_, cx| AgentSessions::agents(cx))
        .into_iter()
        .find(|a| a.id == "fake")
        .expect("configured agent");
    workspace.update_in(cx, |w, window, cx| w.start_agent(agent, None, window, cx));
    let sessions = agents(&workspace, cx);
    wait_until(cx, "the agent to run and its changes to be scanned", |cx| {
        sessions.read_with(cx, |a, _| {
            a.sessions
                .first()
                .is_some_and(|s| s.exit_code == Some(0) && s.changes.len() == 2)
        })
    });
    let id = sessions.read_with(cx, |a, _| a.sessions[0].id);
    assert_eq!(
        std::fs::read_to_string(&key_out).expect("key"),
        "sk-test-123"
    );
    let started = test_support::AGENTS.with(|a| a.borrow().clone());
    assert_eq!(started.len(), 1);
    assert!(started[0].contains("fake-agent"), "{started:?}");
    let terminal = workspace.read_with(cx, |w, _| w.terminal().clone());
    let title = terminal.read_with(cx, |p, cx| {
        let view = p.active().expect("agent tab");
        view.read(cx).title_with_idle(Duration::from_secs(5))
    });
    assert_eq!(title, "✦ fake · exited (0)");

    // The open (clean) buffer picks up the agent's version.
    workspace.update_in(cx, |w, window, cx| w.check_disk_changes(window, cx));
    let agent_version = "a\nB\nc\nd\ne\nf\ng\nh\ni\nj\nK\nl\n";
    assert_eq!(editor.read_with(cx, |e, _| e.state.text()), agent_version);

    // Review.
    cx.simulate_keystrokes(&primary("cmd-alt-r"));
    assert!(workspace.read_with(cx, |w, _| w.in_review()));
    assert_eq!(
        workspace.read_with(cx, |w, cx| w.agent_review().read(cx).session),
        Some(id)
    );
    let (a, new) = sessions.read_with(cx, |s, _| {
        let changes = &s.session(id).expect("session").changes;
        (changes[0].clone(), changes[1].clone())
    });
    assert_eq!(
        (a.path.as_path(), a.status),
        (Path::new("a.txt"), FileStatus::Modified)
    );
    assert_eq!((a.additions, a.deletions, a.hunks.len()), (2, 2, 2));
    assert_eq!(
        (new.path.as_path(), new.status),
        (Path::new("new.txt"), FileStatus::Added)
    );

    // A stale action (the hash of an older version) is refused.
    workspace.update_in(cx, |w, window, cx| {
        w.apply_review(
            id,
            &ReviewAction::RejectFile {
                path: PathBuf::from("a.txt"),
                hash: a.current_hash ^ 1,
            },
            window,
            cx,
        )
    });
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        agent_version
    );

    // Accept the first hunk, reject the second.
    workspace.update_in(cx, |w, window, cx| {
        w.apply_review(
            id,
            &ReviewAction::AcceptHunk {
                path: a.path.clone(),
                hunk: a.hunks[0].id,
                hash: a.current_hash,
            },
            window,
            cx,
        )
    });
    let remaining = sessions.read_with(cx, |s, _| {
        s.session(id).expect("session").changes[0].clone()
    });
    assert_eq!(remaining.hunks.len(), 1);
    workspace.update_in(cx, |w, window, cx| {
        w.apply_review(
            id,
            &ReviewAction::RejectHunk {
                path: remaining.path.clone(),
                hunk: remaining.hunks[0].id,
                hash: remaining.current_hash,
            },
            window,
            cx,
        )
    });
    let expected = "a\nB\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\n";
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        expected
    );
    assert_eq!(
        editor.read_with(cx, |e, _| e.state.text()),
        expected,
        "the buffer follows"
    );
    assert!(!editor.read_with(cx, |e, _| e.is_dirty()));

    // Reject the new file: it is deleted; nothing is left to review.
    workspace.update_in(cx, |w, window, cx| {
        w.apply_review(
            id,
            &ReviewAction::RejectFile {
                path: new.path.clone(),
                hash: new.current_hash,
            },
            window,
            cx,
        )
    });
    assert!(!root.join("new.txt").exists());
    assert!(sessions.read_with(cx, |s, _| {
        s.session(id).expect("session").changes.is_empty()
    }));

    // Undo in the editor brings back the agent's line.
    workspace.update_in(cx, |w, window, cx| w.set_mode_editor(window, cx));
    editor.update_in(cx, |e, window, cx| {
        e.focus(window, cx);
        e.run_command("editor:undo", window, cx);
    });
    assert_eq!(editor.read_with(cx, |e, _| e.state.text()), agent_version);
}
