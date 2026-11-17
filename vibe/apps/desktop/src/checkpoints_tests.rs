//! Headless test of the agent's checkpoints in the git column: a turn
//! that edits a file becomes a marker above the history; "Restore to
//! here…" asks, then puts the file back.

use std::sync::Arc;

use ferrumagent_ui::Approval;
use ferrumagent_ui::core::provider::mock::MockProvider;
use ferrumgit_ui::RestoreMarker;
use serde_json::json;

use super::*;

#[gpui::test]
fn agent_turns_are_checkpoints_in_the_git_history(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let root = fx.path("a");
    std::fs::write(root.join("a.txt"), "alpha\nbeta\n").expect("write");
    git_in(&root, &["init", "-q"]);
    git_in(&root, &["add", "."]);
    git_in(&root, &["commit", "-q", "-m", "one"]);

    let mock = MockProvider::new();
    mock.push_tool(
        "t1",
        "edit_file",
        json!({"path": "a.txt", "old_string": "beta", "new_string": "BETA"}),
    );
    mock.push_text("Done.");
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
    let chat = read(&app, cx, |a| a.chat().expect("chat"));
    let git = read(&app, cx, |a| a.git.clone().expect("git"));
    wait_for_session(&chat, cx);
    let session = chat.read_with(cx, |c, _| c.session().cloned().expect("session"));
    session.set_approval(Approval::AutoEdits).expect("approval");

    // The git column shows its history, where the markers are drawn.
    update(&app, cx, |a, w, cx| a.set_visible(Column::Git, true, w, cx));
    git.update(cx, |g, cx| g.show_history(cx));
    update(&app, cx, |a, w, cx| {
        a.start_task(Mode::Agent, Some("Make beta loud".into()), w, cx);
    });
    let mut markers = Vec::new();
    for _ in 0..400 {
        markers = git.read_with(cx, |g, _| g.history_markers().to_vec());
        if !markers.is_empty() {
            break;
        }
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        draw(cx);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(markers.len(), 1, "{markers:?}");
    assert_eq!(markers[0].title, "Before: Make beta loud");
    assert!(markers[0].detail.starts_with("1 file · "));
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        "alpha\nBETA\n"
    );

    // "Restore to here…" asks first; Cancel changes nothing.
    let id = markers[0].id;
    git.update(cx, |_, cx| cx.emit(RestoreMarker { id }));
    draw(cx);
    let prompt = read(&app, cx, |a| a.restore_prompt.clone()).expect("prompt");
    assert_eq!(prompt.files, [PathBuf::from("a.txt")]);
    update(&app, cx, |a, _, cx| a.answer_restore(false, cx));
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        "alpha\nBETA\n"
    );

    // Restore puts the file back and the marker goes.
    git.update(cx, |_, cx| cx.emit(RestoreMarker { id }));
    update(&app, cx, |a, _, cx| a.answer_restore(true, cx));
    wait(&app, cx, |_| {
        std::fs::read_to_string(root.join("a.txt")).is_ok_and(|t| t == "alpha\nbeta\n")
    });
    for _ in 0..400 {
        if git.read_with(cx, |g, _| g.history_markers().is_empty()) {
            return;
        }
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        draw(cx);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the marker stayed after the restore");
}
