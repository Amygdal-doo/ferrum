//! The Logs page, headless: the window is never shown. Sources run for
//! real (`/bin/sh` commands, temp files), so the executor may park while
//! runner threads produce output.

use crate::logs::{LogsEvent, LogsView, Options};
use crate::tests::{init, primary};
use crate::workspace::Workspace;
use ferrum_ui::gpui::Focusable as _;
use ferrum_ui::gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use ferrumlogs_core::{Level, Levels, SourceConfig, SourceKind, Store};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn options(store_file: PathBuf) -> Options {
    Options {
        store_file: Some(store_file),
        recent_file: None,
        shell: PathBuf::from("/bin/sh"),
        login: false,
        home: None,
    }
}

fn open<'a>(
    project: &Path,
    store_file: PathBuf,
    cx: &'a mut TestAppContext,
) -> (Entity<LogsView>, &'a mut VisualTestContext) {
    cx.executor().allow_parking();
    // The editor's keymap, which includes the Logs page's shortcuts.
    init(cx);
    let project = project.to_path_buf();
    let (app, cx) = cx.add_window_view(move |window, cx| {
        LogsView::new(options(store_file), Some(project), window, cx)
    });
    cx.run_until_parked();
    (app, cx)
}

/// Lets runner threads and the batching timer run until `done`.
fn wait_until(
    app: &Entity<LogsView>,
    cx: &mut VisualTestContext,
    done: impl Fn(&LogsView) -> bool,
) {
    for _ in 0..300 {
        cx.executor().advance_clock(Duration::from_millis(60));
        cx.run_until_parked();
        if app.read_with(cx, |app, _| done(app)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for log output");
}

fn visible(app: &LogsView) -> Vec<String> {
    let Some(session) = app.session() else {
        return Vec::new();
    };
    (0..session.view.len())
        .filter_map(|row| session.row(row).map(|l| l.text.clone()))
        .collect()
}

fn has_line(app: &LogsView, text: &str) -> bool {
    visible(app).iter().any(|l| l == text)
}

#[gpui::test]
fn suggested_log_folder_is_tailed_and_saved(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let project = dir.path().join("proj");
    std::fs::create_dir_all(project.join("logs")).expect("mkdir");
    std::fs::write(project.join("logs/app.log"), "INFO booted\nERROR db down\n").expect("log");
    let store_file = dir.path().join("store.json");
    let (app, cx) = open(&project, store_file.clone(), cx);

    let suggestion = app.read_with(cx, |app, _| app.suggestions.clone());
    assert_eq!(suggestion.len(), 1);
    assert_eq!(suggestion[0].kind.target(), "logs/*.log");
    // One click: add and start.
    let button = cx
        .debug_bounds("add-suggestion-0")
        .expect("suggestion button");
    cx.simulate_click(button.center(), Modifiers::none());
    wait_until(&app, cx, |app| has_line(app, "ERROR db down"));

    app.read_with(cx, |app, _| {
        assert!(
            app.suggestions.is_empty(),
            "suggestions hide once a source exists"
        );
        let session = app.session().expect("session");
        assert_eq!(session.sources[0].errors, 1);
        assert_eq!(session.running_count(), 1);
    });
    let saved = Store::load(&store_file).sources(&project);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].kind, suggestion[0].kind);

    // Appended lines show up live.
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(project.join("logs/app.log"))
        .expect("open");
    std::io::Write::write_all(&mut file, b"WARN later\n").expect("append");
    wait_until(&app, cx, |app| has_line(app, "WARN later"));
    app.update(cx, |app, cx| app.stop_all(cx));
    wait_until(&app, cx, |app| {
        app.session().is_some_and(|s| s.running_count() == 0)
    });
}

#[gpui::test]
fn commands_filters_selection_and_copy(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let (app, cx) = open(dir.path(), dir.path().join("store.json"), cx);

    // Add a command through the dialog.
    app.update_in(cx, |app, window, cx| app.open_dialog(None, window, cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let dialog = app.dialog.as_ref().expect("dialog");
        dialog.target.update(cx, |input, cx| {
            input.set_text(
                r#"echo 'INFO ready'; sleep 0.2; echo 'ERROR boom' >&2; sleep 0.2; echo '{"level":"warn","msg":"slow","ms":9}'; exit 4"#,
                cx,
            );
        });
        dialog.name.update(cx, |input, cx| input.set_text("api", cx));
        app.submit_dialog(true, window, cx);
    });
    assert!(app.read_with(cx, |app, _| app.dialog.is_none()));
    wait_until(&app, cx, |app| {
        app.session().is_some_and(|s| s.running_count() == 0) && has_line(app, "slow")
    });
    app.read_with(cx, |app, _| {
        let session = app.session().expect("session");
        let source = &session.sources[0];
        assert_eq!(source.config.name, "api");
        assert_eq!(source.status_text(), "exited 4");
        assert_eq!((source.errors, source.warns), (2, 1));
        let json = (0..session.view.len())
            .filter_map(|r| session.row(r))
            .find(|l| l.text == "slow")
            .expect("json line");
        assert_eq!(json.level, Some(Level::Warn));
        assert_eq!(json.fields, [("ms".to_owned(), "9".to_owned())]);
    });

    // ⌘E: errors only, and back.
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.focus_handle(cx), cx);
    });
    cx.simulate_keystrokes("cmd-e");
    app.read_with(cx, |app, _| {
        assert_eq!(visible(app), ["ERROR boom", "exited with code 4"]);
    });
    cx.simulate_keystrokes("cmd-e");
    assert_eq!(
        app.read_with(cx, |app, _| app.session().map(|s| s.filter().levels)),
        Some(Levels::default())
    );

    // ⌘F, then type into the toolbar search.
    cx.simulate_keystrokes("cmd-f");
    cx.simulate_input("ready|boom");
    cx.run_until_parked();
    assert!(
        app.read_with(cx, |app, _| visible(app).is_empty()),
        "plain text by default"
    );
    app.update(cx, |app, cx| app.update_filter(cx, |f| f.regex = true));
    let shown = app.read_with(cx, |app, _| visible(app));
    // The `$ command` line written at start matches too.
    assert_eq!(shown.len(), 3);
    assert!(shown[0].starts_with("$ echo"));
    assert_eq!(shown[1..], ["INFO ready", "ERROR boom"]);

    // Select both lines (click, shift-click) and copy them.
    app.update(cx, |app, cx| {
        app.select_row(1, false, cx);
        app.select_row(2, true, cx);
    });
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some("[api] INFO ready\n[api] ERROR boom\n")
    );

    // Hiding the source hides its lines; clear empties the view.
    app.update(cx, |app, cx| {
        app.update_filter(cx, |f| {
            f.query.clear();
            f.hidden.insert(0);
        })
    });
    assert!(app.read_with(cx, |app, _| visible(app).is_empty()));
    app.update(cx, |app, cx| app.update_filter(cx, |f| f.hidden.clear()));
    // ⌘K clears, even though the editor also has ⌘K chords.
    app.update_in(cx, |app, window, cx| {
        window.focus(&app.focus_handle(cx), cx);
    });
    cx.simulate_keystrokes("cmd-k");
    app.read_with(cx, |app, _| {
        let session = app.session().expect("session");
        assert!(session.view.is_empty());
        assert_eq!(session.sources[0].errors, 0);
    });
}

#[gpui::test]
fn scrolling_up_pauses_and_jump_resumes(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let (app, cx) = open(dir.path(), dir.path().join("store.json"), cx);
    let source = SourceConfig::new(
        "ticker",
        SourceKind::Command {
            command: "i=0; while [ $i -lt 400 ]; do echo line $i; i=$((i+1)); done; sleep 30"
                .into(),
        },
    );
    let id = source.id.clone();
    app.update(cx, |app, cx| app.save_source(source, true, cx));
    wait_until(&app, cx, |app| has_line(app, "line 399"));
    assert!(app.read_with(cx, |app, _| app.follow));

    // Wheel up over the list pauses following.
    app.update(cx, |app, cx| app.on_wheel(true, false, cx));
    assert!(!app.read_with(cx, |app, _| app.follow));
    cx.run_until_parked();
    assert!(cx.debug_bounds("jump-to-latest").is_some());

    // New lines while paused are counted.
    app.update(cx, |app, cx| {
        let session = app.session_mut().expect("session");
        let slot = session.sources[0].slot;
        let run = (0..100).find(|r| session.owns_run(*r)).expect("run");
        let lines = vec![ferrumlogs_core::LogLine::system(slot, "extra", None)];
        app.apply_events(vec![ferrumlogs_core::Event::Lines { run, lines }], cx);
    });
    assert_eq!(app.read_with(cx, |app, _| app.unseen), 1);

    let button = cx.debug_bounds("jump-to-latest").expect("jump button");
    cx.simulate_click(button.center(), Modifiers::none());
    app.read_with(cx, |app, _| {
        assert!(app.follow);
        assert_eq!(app.unseen, 0);
    });

    // Stop from the sidebar.
    app.update(cx, |app, cx| app.stop_source(&id, cx));
    wait_until(&app, cx, |app| {
        app.session().is_some_and(|s| s.running_count() == 0)
    });
    assert!(app.read_with(cx, |app, _| has_line(app, "stopped")));
}

#[gpui::test]
fn switching_projects_keeps_sessions(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    std::fs::create_dir_all(&a).expect("a");
    std::fs::create_dir_all(&b).expect("b");
    let store_file = dir.path().join("store.json");
    let mut store = Store::default();
    let mut auto = SourceConfig::new(
        "hello",
        SourceKind::Command {
            command: "echo hello from b".into(),
        },
    );
    auto.autostart = true;
    store.set_sources(&b, vec![auto]);
    store.save(&store_file).expect("save");

    let (app, cx) = open(&a, store_file, cx);
    app.update(cx, |app, cx| app.open_project(b.clone(), cx));
    wait_until(&app, cx, |app| has_line(app, "hello from b"));
    app.update(cx, |app, cx| app.open_project(a.clone(), cx));
    app.read_with(cx, |app, _| {
        assert_eq!(app.current.as_deref(), Some(a.as_path()));
        assert_eq!(app.sessions.len(), 2);
    });
    app.update(cx, |app, cx| app.open_project(b.clone(), cx));
    assert!(app.read_with(cx, |app, _| has_line(app, "hello from b")));
}

#[gpui::test]
fn the_logs_page_opens_from_the_workspace_on_the_project(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    let (a, b) = (root.join("a"), root.join("b"));
    std::fs::create_dir_all(a.join("logs")).expect("mkdir");
    std::fs::create_dir_all(&b).expect("mkdir");
    std::fs::write(a.join("logs/app.log"), "INFO booted\n").expect("log");
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    workspace.update_in(cx, |w, window, cx| {
        w.set_logs_options(options(root.join("store.json")));
        w.set_root(a.clone(), window, cx);
    });
    assert!(
        workspace.read_with(cx, |w, _| w.logs().is_none()),
        "the page starts on first use"
    );

    // ⌥⌘L opens the Logs page on the editor's project.
    cx.simulate_keystrokes(&primary("cmd-alt-l"));
    assert!(workspace.read_with(cx, |w, _| w.in_logs_view()));
    let logs = workspace.read_with(cx, |w, _| w.logs().cloned().expect("logs page"));
    logs.read_with(cx, |l, _| {
        assert_eq!(l.current.as_deref(), Some(a.as_path()));
        assert_eq!(l.suggestions.len(), 1, "logs/*.log is suggested");
    });

    // A project picked in the page's sidebar opens in the editor, and the
    // page follows.
    logs.update(cx, |_, cx| cx.emit(LogsEvent::OpenProject(b.clone())));
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |w, _| w.root.clone()),
        Some(b.clone())
    );
    logs.read_with(cx, |l, _| {
        assert_eq!(l.current.as_deref(), Some(b.as_path()));
        assert_eq!(l.sessions.len(), 2, "a's session stays open");
    });

    // The palette command toggles back to the editor, and ⌥⌘G to Git.
    workspace.update_in(cx, |w, window, cx| {
        w.run_command("logs:toggle-view", window, cx)
    });
    assert!(!workspace.read_with(cx, |w, _| w.in_logs_view()));
    cx.simulate_keystrokes(&primary("cmd-alt-l"));
    assert!(workspace.read_with(cx, |w, _| w.in_logs_view()));
    cx.simulate_keystrokes(&primary("cmd-alt-g"));
    assert!(workspace.read_with(cx, |w, _| w.in_git_view()));
}
