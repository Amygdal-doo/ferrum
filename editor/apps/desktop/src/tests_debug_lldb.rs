//! The debugger end to end with the real `lldb-dap` on a small C program,
//! driven like a user would: F9 in the gutter, start, the stop marked in
//! the editor, locals, F11 into a function (two frames on the stack),
//! picking the caller's frame, ⇧F11 out, the console evaluating, F10, F5
//! to the exit code. Needs `lldb-dap` (Xcode or LLVM) and `cc`:
//! `cargo test -p ferrumeditor-desktop -- --ignored`.

use std::time::Duration;

use gpui::{Entity, TestAppContext, VisualTestContext};

use crate::debug::Debugger;
use crate::tests::init;
use crate::workspace::Workspace;

const PROGRAM: &str = "#include <stdio.h>
static int twice(int n) {
    int doubled = n * 2;
    return doubled;
}
int main(void) {
    int answer = 21;
    answer = twice(answer);
    printf(\"%d\\n\", answer);
    return 3;
}
";

fn wait_until(
    cx: &mut VisualTestContext,
    what: &str,
    mut done: impl FnMut(&mut VisualTestContext) -> bool,
) {
    // Real time: lldb-dap can take a while to launch.
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    while std::time::Instant::now() < deadline {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        cx.executor().advance_clock(Duration::from_millis(50));
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {what}");
}

fn local(debugger: &Entity<Debugger>, name: &str, cx: &mut VisualTestContext) -> Option<String> {
    debugger.read_with(cx, |d, _| {
        let session = d.session.as_ref()?;
        session
            .state
            .variables
            .values()
            .flatten()
            .find(|v| v.name == name)
            .map(|v| v.value.clone())
    })
}

#[gpui::test]
#[ignore = "needs lldb-dap and a C compiler"]
fn lldb_dap_session_through_the_editor(cx: &mut TestAppContext) {
    let path_env = std::env::var("PATH").unwrap_or_default();
    if ferrumeditor_core::dap::adapters::find_lldb_dap(&path_env).is_none() {
        eprintln!("skipping: no lldb-dap");
        return;
    }
    cx.executor().allow_parking();
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    let source = root.join("main.c");
    std::fs::write(&source, PROGRAM).expect("write");
    let built = std::process::Command::new("cc")
        .args(["-g", "-O0", "-o"])
        .arg(root.join("main"))
        .arg(&source)
        .status()
        .expect("cc");
    assert!(built.success());
    std::fs::create_dir_all(root.join(".vscode")).expect("mkdir");
    std::fs::write(
        root.join(".vscode/launch.json"),
        r#"{ "configurations": [ { "name": "main", "type": "lldb-dap", "request": "launch",
             "program": "${workspaceFolder}/main", "cwd": "${workspaceFolder}" } ] }"#,
    )
    .expect("write");

    let (workspace, cx) = cx.add_window_view(Workspace::new);
    workspace.update_in(cx, |w, window, cx| {
        w.set_root(root.clone(), window, cx);
        // Line 8 (`answer = twice(answer);`), 0-based 7.
        w.open_file_at(&source, Some(7), None, window, cx);
    });
    cx.run_until_parked();
    let editor = workspace
        .read_with(cx, |w, _| w.active_editor())
        .expect("editor");
    cx.simulate_keystrokes("f9");
    let debugger = workspace.read_with(cx, |w, _| w.debugger().clone());
    assert_eq!(
        debugger.read_with(cx, |d, _| d.breakpoints.lines(&source)),
        [8]
    );
    let config = crate::debug::configurations(&root, Some(&source))
        .0
        .into_iter()
        .next()
        .expect("config");
    workspace.update_in(cx, |w, _, cx| w.start_debugging(config, cx));

    wait_until(cx, "the stop at the breakpoint", |cx| {
        editor.read_with(cx, |e, _| e.debug_marks.exec_line) == Some(7)
    });
    assert_eq!(
        editor.read_with(cx, |e, _| e.debug_marks.breakpoints.clone()),
        [(7, Some(true))],
        "verified by lldb-dap"
    );
    wait_until(cx, "locals", |cx| {
        local(&debugger, "answer", cx).as_deref() == Some("21")
    });

    // F11 steps into `twice`: two frames, the editor on line 3.
    cx.simulate_keystrokes("f11");
    wait_until(cx, "the step into", |cx| {
        editor.read_with(cx, |e, _| e.debug_marks.exec_line) == Some(2)
    });
    let frames: Vec<String> = debugger.read_with(cx, |d, _| {
        d.session
            .as_ref()
            .map(|s| s.state.frames.iter().map(|f| f.name.clone()).collect())
            .unwrap_or_default()
    });
    assert!(
        frames.len() >= 2 && frames[0].contains("twice") && frames[1].contains("main"),
        "{frames:?}"
    );
    wait_until(cx, "the argument", |cx| {
        local(&debugger, "n", cx).as_deref() == Some("21")
    });
    // Picking the caller's frame shows its locals.
    debugger.update(cx, |d, cx| d.select_frame(1, cx));
    wait_until(cx, "the caller's locals", |cx| {
        local(&debugger, "answer", cx).as_deref() == Some("21")
    });

    // ⇧F11 returns to main; the console evaluates there.
    cx.simulate_keystrokes("shift-f11");
    wait_until(cx, "the step out", |cx| {
        debugger.read_with(cx, |d, _| {
            d.session.as_ref().is_some_and(|s| {
                s.state
                    .frames
                    .first()
                    .is_some_and(|f| f.name.contains("main"))
                    && d.is_stopped()
            })
        })
    });
    debugger.update(cx, |d, cx| d.evaluate("answer + 1".into(), cx));
    wait_until(cx, "the evaluate result", |cx| {
        debugger.read_with(cx, |d, _| d.console.iter().any(|l| l.text.contains("22")))
    });

    // F10 to the printf line, then F5 to the end.
    cx.simulate_keystrokes("f10");
    wait_until(cx, "the step over", |cx| {
        local(&debugger, "answer", cx).as_deref() == Some("42")
    });
    cx.simulate_keystrokes("f5");
    wait_until(cx, "the end of the session", |cx| {
        debugger.read_with(cx, |d, _| d.session.is_none())
    });
    assert_eq!(editor.read_with(cx, |e, _| e.debug_marks.exec_line), None);
    assert!(
        debugger.read_with(cx, |d, _| {
            d.console
                .iter()
                .any(|l| l.text.contains("exited with code 3"))
        }),
        "{:?}",
        debugger.read_with(cx, |d, _| d
            .console
            .iter()
            .map(|l| l.text.clone())
            .collect::<Vec<_>>())
    );
}
