//! The stand-alone app's file editor: opening a changed file, saving with
//! the keyboard (the Changes list and the diff follow), and never dropping
//! unsaved edits; hosts still get `OpenFile`.

use super::{host, init, repo, wait_for};
use crate::components::editor::pane::{Answer, Pending};
use crate::state::Page;
use crate::{GitView, GitViewEvent, GitViewOptions};
use gpui::{Entity, VisualTestContext};
use std::path::Path;

/// `cmd` on macOS, `ctrl` elsewhere (FerrumIDE's default keymap).
fn primary(key: &str) -> String {
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    format!("{modifier}-{key}")
}

fn loaded(v: &GitView) -> bool {
    v.state.active.is_some_and(|id| {
        let view = v.state.view(id);
        view.data.is_some() && !view.loading && !v.state.is_busy(id)
    })
}

fn changed_files(v: &GitView) -> Option<usize> {
    let id = v.state.active?;
    Some(v.state.view(id).data?.snapshot.status.files.len())
}

fn editor_text(git: &Entity<GitView>, cx: &mut VisualTestContext) -> Option<String> {
    git.read_with(cx, |v, cx| {
        v.editor.as_ref().map(|o| o.view.read(cx).text())
    })
}

fn edit(git: &Entity<GitView>, path: &str, line: Option<u32>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        git.update(cx, |view, cx| {
            let id = view.state.active.expect("active");
            view.edit_file(id, path, line, window, cx);
        });
    });
    cx.run_until_parked();
}

fn answer(git: &Entity<GitView>, answer: Answer, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        git.update(cx, |view, cx| view.answer_unsaved(answer, window, cx));
    });
    cx.run_until_parked();
}

fn pending(git: &Entity<GitView>, cx: &mut VisualTestContext) -> Option<Pending> {
    git.read_with(cx, |v, _| v.editor.as_ref().and_then(|o| o.pending.clone()))
}

#[gpui::test]
fn the_app_edits_a_changed_file_and_saving_refreshes_the_changes(cx: &mut gpui::TestAppContext) {
    let data = tempfile::tempdir().expect("data dir");
    let repo = repo();
    init(data.path(), cx);
    let (host, cx) = host(
        Some(repo.path().to_path_buf()),
        GitViewOptions::desktop(),
        cx,
    );
    let git = host.read_with(cx, |h, _| h.git.clone());
    wait_for(cx, &git, loaded);

    // The Changes page with the file's diff open (one added line).
    git.update(cx, |view, cx| {
        let id = view.state.active.expect("active");
        view.set_page(id, Page::WorkingCopy, cx);
        let file = view
            .state
            .view(id)
            .data
            .expect("data")
            .snapshot
            .status
            .files[0]
            .clone();
        view.open_file(id, &file, false, cx);
    });
    wait_for(cx, &git, |v| {
        v.state
            .active
            .and_then(|id| v.state.view(id).diff)
            .is_some_and(|d| d.file.additions == 1)
    });

    // Edit opens the file in place, at the changed line, focused.
    edit(&git, "a.txt", Some(2), cx);
    git.read_with(cx, |view, cx| {
        assert!(view.editor_shown());
        let editor = view.editor.as_ref().expect("editor").view.read(cx);
        assert_eq!(editor.text(), "one\ntwo\n");
        assert_eq!(editor.cursor_line(), 2);
        assert!(!editor.is_dirty());
        assert!(!view.has_unsaved_edits(cx));
    });

    // Typing edits the buffer (and marks it unsaved); nothing is written yet.
    cx.simulate_input("zero\n");
    assert_eq!(editor_text(&git, cx).as_deref(), Some("one\nzero\ntwo\n"));
    git.read_with(cx, |view, cx| assert!(view.has_unsaved_edits(cx)));
    let file = repo.path().join("a.txt");
    assert_eq!(read(&file), "one\ntwo\n");

    // ⌘S saves; the reload refreshes the diff (two added lines now).
    cx.simulate_keystrokes(&primary("s"));
    assert_eq!(read(&file), "one\nzero\ntwo\n");
    git.read_with(cx, |view, cx| assert!(!view.has_unsaved_edits(cx)));
    wait_for(cx, &git, |v| {
        v.state
            .active
            .and_then(|id| v.state.view(id).diff)
            .is_some_and(|d| d.file.additions == 2)
    });

    // Back to the committed text and save: the file leaves the
    // Changes list and its diff closes.
    cx.simulate_keystrokes(&primary("a"));
    cx.simulate_input("one\n");
    assert_eq!(editor_text(&git, cx).as_deref(), Some("one\n"));
    cx.simulate_keystrokes(&primary("s"));
    assert_eq!(read(&file), "one\n");
    wait_for(cx, &git, |v| {
        loaded(v)
            && changed_files(v) == Some(0)
            && v.state
                .active
                .is_some_and(|id| v.state.view(id).diff.is_none())
    });
}

#[gpui::test]
fn unsaved_edits_are_never_dropped_silently(cx: &mut gpui::TestAppContext) {
    let data = tempfile::tempdir().expect("data dir");
    let repo = repo();
    std::fs::write(repo.path().join("b.txt"), "bee\n").expect("write");
    init(data.path(), cx);
    let (host, cx) = host(
        Some(repo.path().to_path_buf()),
        GitViewOptions::desktop(),
        cx,
    );
    let git = host.read_with(cx, |h, _| h.git.clone());
    wait_for(cx, &git, loaded);
    edit(&git, "a.txt", None, cx);
    cx.simulate_input("x");
    assert_eq!(editor_text(&git, cx).as_deref(), Some("xone\ntwo\n"));

    // Closing asks; Cancel keeps the edits.
    cx.update(|window, cx| git.update(cx, |view, cx| view.close_editor(window, cx)));
    assert_eq!(pending(&git, cx), Some(Pending::Close));
    answer(&git, Answer::Cancel, cx);
    assert_eq!(pending(&git, cx), None);
    assert_eq!(editor_text(&git, cx).as_deref(), Some("xone\ntwo\n"));

    // Quitting asks too.
    assert!(!git.update(cx, |view, cx| view.confirm_quit(cx)));
    assert_eq!(pending(&git, cx), Some(Pending::Quit));
    answer(&git, Answer::Cancel, cx);

    // So does closing the repository's tab, which stays open meanwhile.
    git.update(cx, |view, cx| {
        let id = view.state.active.expect("active");
        view.close_tab(id, cx);
        assert_eq!(view.state.open, vec![id]);
    });
    assert_eq!(pending(&git, cx), Some(Pending::Close));
    answer(&git, Answer::Cancel, cx);

    // Opening another file asks; Save writes this one, then opens it.
    edit(&git, "b.txt", Some(1), cx);
    assert_eq!(
        pending(&git, cx),
        Some(Pending::Open {
            path: "b.txt".into(),
            line: Some(1)
        })
    );
    assert_eq!(editor_text(&git, cx).as_deref(), Some("xone\ntwo\n"));
    answer(&git, Answer::Save, cx);
    assert_eq!(read(&repo.path().join("a.txt")), "xone\ntwo\n");
    assert_eq!(editor_text(&git, cx).as_deref(), Some("bee\n"));

    // Discard drops the edits and leaves the file as it was.
    cx.simulate_input("y");
    cx.update(|window, cx| git.update(cx, |view, cx| view.close_editor(window, cx)));
    assert_eq!(editor_text(&git, cx).as_deref(), Some("ybee\n"));
    answer(&git, Answer::Discard, cx);
    assert_eq!(editor_text(&git, cx), None);
    assert_eq!(read(&repo.path().join("b.txt")), "bee\n");
    assert!(git.update(cx, |view, cx| view.confirm_quit(cx)));
}

#[gpui::test]
fn a_clean_buffer_follows_the_file_on_disk(cx: &mut gpui::TestAppContext) {
    let data = tempfile::tempdir().expect("data dir");
    let repo = repo();
    init(data.path(), cx);
    let (host, cx) = host(
        Some(repo.path().to_path_buf()),
        GitViewOptions::desktop(),
        cx,
    );
    let git = host.read_with(cx, |h, _| h.git.clone());
    wait_for(cx, &git, loaded);
    edit(&git, "a.txt", None, cx);

    // A discard (here: an outside write) shows up after the reload.
    std::fs::write(repo.path().join("a.txt"), "one\nchanged elsewhere\n").expect("write");
    git.update(cx, |view, cx| {
        let id = view.state.active.expect("active");
        view.load(id, cx);
    });
    wait_for(cx, &git, loaded);
    assert_eq!(
        editor_text(&git, cx).as_deref(),
        Some("one\nchanged elsewhere\n")
    );

    // Edits of its own are kept.
    cx.simulate_input("mine ");
    std::fs::write(repo.path().join("a.txt"), "one\n").expect("write");
    git.update(cx, |view, cx| {
        let id = view.state.active.expect("active");
        view.load(id, cx);
    });
    wait_for(cx, &git, loaded);
    assert_eq!(
        editor_text(&git, cx).as_deref(),
        Some("mine one\nchanged elsewhere\n")
    );
}

#[gpui::test]
fn embedded_hosts_still_open_files_themselves(cx: &mut gpui::TestAppContext) {
    let data = tempfile::tempdir().expect("data dir");
    let repo = repo();
    init(data.path(), cx);
    let (host, cx) = host(
        Some(repo.path().to_path_buf()),
        GitViewOptions::embedded(),
        cx,
    );
    let git = host.read_with(cx, |h, _| h.git.clone());
    wait_for(cx, &git, loaded);
    edit(&git, "a.txt", Some(2), cx);
    assert_eq!(editor_text(&git, cx), None);
    host.read_with(cx, |host, _| {
        assert!(host.events.iter().any(|e| matches!(
            e,
            GitViewEvent::OpenFile { path, line: Some(2) } if path.ends_with("a.txt")
        )));
    });
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("read")
}
