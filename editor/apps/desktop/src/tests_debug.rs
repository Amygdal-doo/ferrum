//! The debugger end to end with a scripted adapter (Python, named by
//! `debugAdapterPath` in `.vscode/launch.json`): F9 sets a breakpoint, the
//! session stops there (the line is marked, variables load), F10 steps,
//! the adapter's `runInTerminal` lands in a terminal tab, evaluate works,
//! F5 continues to the end.

use std::path::Path;
use std::time::Duration;

use gpui::{Entity, TestAppContext, VisualTestContext};

use crate::debug::Debugger;
use crate::terminal::test_support;
use crate::tests::init;
use crate::workspace::Workspace;

const FAKE_ADAPTER: &str = r#"#!/usr/bin/env python3
import json, sys
inp, out = sys.stdin.buffer, sys.stdout.buffer
seq = 0
def send(msg):
    global seq
    seq += 1
    msg["seq"] = seq
    body = json.dumps(msg).encode()
    out.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
    out.flush()
def event(name, body):
    send({"type": "event", "event": name, "body": body})
line, bps, program = 0, [], None
while True:
    length = None
    while True:
        header = inp.readline()
        if not header:
            sys.exit(0)
        header = header.strip()
        if not header:
            break
        if header.lower().startswith(b"content-length:"):
            length = int(header.split(b":")[1])
    msg = json.loads(inp.read(length))
    cmd, args = msg.get("command"), msg.get("arguments") or {}
    if msg.get("type") == "response":
        continue
    def reply(body=None):
        send({"type": "response", "request_seq": msg["seq"], "command": cmd, "success": True, "body": body or {}})
    if cmd == "initialize":
        reply({"supportsConfigurationDoneRequest": True})
    elif cmd == "launch":
        program = args["program"]
        reply()
        event("initialized", {})
    elif cmd == "setBreakpoints":
        bps = [b["line"] for b in args["breakpoints"]]
        reply({"breakpoints": [{"id": i + 1, "verified": True, "line": l} for i, l in enumerate(bps)]})
    elif cmd == "configurationDone":
        reply()
        send({"type": "request", "command": "runInTerminal",
              "arguments": {"kind": "integrated", "title": "fake program", "args": ["echo", "hi"]}})
        line = bps[0] if bps else 1
        event("stopped", {"reason": "breakpoint", "threadId": 1})
    elif cmd == "threads":
        reply({"threads": [{"id": 1, "name": "main"}]})
    elif cmd == "stackTrace":
        reply({"stackFrames": [{"id": 1, "name": "main", "line": line, "column": 1, "source": {"path": program}}]})
    elif cmd == "scopes":
        reply({"scopes": [{"name": "Locals", "variablesReference": 7}]})
    elif cmd == "variables":
        reply({"variables": [{"name": "count", "value": str(line), "variablesReference": 0}]})
    elif cmd == "evaluate":
        reply({"result": "answer=" + args["expression"], "variablesReference": 0})
    elif cmd == "next":
        reply()
        line += 1
        event("stopped", {"reason": "step", "threadId": 1})
    elif cmd == "continue":
        reply()
        event("output", {"category": "stdout", "output": "bye\n"})
        event("exited", {"exitCode": 0})
        event("terminated", {})
    elif cmd == "disconnect":
        reply()
        sys.exit(0)
    else:
        reply()
"#;

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

fn debugger(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<Debugger> {
    workspace.read_with(cx, |w, _| w.debugger().clone())
}

#[gpui::test]
fn breakpoints_stepping_and_console_with_a_fake_adapter(cx: &mut TestAppContext) {
    if std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: python3 is not installed");
        return;
    }
    cx.executor().allow_parking();
    init(cx);
    test_support::reset();
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    let source = root.join("main.rs");
    std::fs::write(
        &source,
        "fn main() {\n    let a = 1;\n    let b = 2;\n    println!(\"{}\", a + b);\n}\n",
    )
    .expect("write");
    let adapter = root.join("fake-adapter.py");
    std::fs::write(&adapter, FAKE_ADAPTER).expect("write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&adapter, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    std::fs::create_dir_all(root.join(".vscode")).expect("mkdir");
    std::fs::write(
        root.join(".vscode/launch.json"),
        format!(
            r#"{{ "configurations": [ {{ "name": "Fake", "type": "lldb", "request": "launch",
                 "program": "${{workspaceFolder}}/main.rs", "debugAdapterPath": "{}" }} ] }}"#,
            adapter.display()
        ),
    )
    .expect("write");

    let (workspace, cx) = cx.add_window_view(Workspace::new);
    workspace.update_in(cx, |w, window, cx| {
        w.set_root(root.clone(), window, cx);
        w.open_file_at(&source, Some(1), None, window, cx);
    });
    cx.run_until_parked();
    let editor = workspace
        .read_with(cx, |w, _| w.active_editor())
        .expect("editor");

    // F9 on line 2 sets a breakpoint (no session: not yet verified).
    cx.simulate_keystrokes("f9");
    assert_eq!(
        editor.read_with(cx, |e, _| e.debug_marks.breakpoints.clone()),
        [(1, None)]
    );
    let debugger = debugger(&workspace, cx);
    assert_eq!(
        debugger.read_with(cx, |d, _| d.breakpoints.lines(&source)),
        [2]
    );

    // The picker lists launch.json first; start it.
    let config = crate::debug::configurations(&root, Some(&source))
        .0
        .into_iter()
        .next()
        .expect("config");
    assert_eq!(
        (config.name.as_str(), config.source),
        ("Fake", "launch.json")
    );
    workspace.update_in(cx, |w, _, cx| w.start_debugging(config, cx));

    wait_until(cx, "the stop at the breakpoint", |cx| {
        editor.read_with(cx, |e, _| e.debug_marks.exec_line) == Some(1)
    });
    assert_eq!(
        editor.read_with(cx, |e, _| e.debug_marks.breakpoints.clone()),
        [(1, Some(true))],
        "verified by the adapter"
    );
    wait_until(cx, "variables", |cx| {
        debugger.read_with(cx, |d, _| {
            d.session.as_ref().is_some_and(|s| {
                s.state
                    .variables
                    .values()
                    .flatten()
                    .any(|v| v.name == "count" && v.value == "2")
            })
        })
    });
    // runInTerminal ran the program in a terminal tab.
    wait_until(cx, "runInTerminal", |_| {
        test_support::TASKS.with(|t| t.borrow().iter().any(|c| c == "echo hi"))
    });

    // F10 steps to the next line.
    cx.simulate_keystrokes("f10");
    wait_until(cx, "the step", |cx| {
        editor.read_with(cx, |e, _| e.debug_marks.exec_line) == Some(2)
    });

    // The console evaluates in the paused frame.
    debugger.update(cx, |d, cx| d.evaluate("a + b".into(), cx));
    wait_until(cx, "the evaluate result", |cx| {
        debugger.read_with(cx, |d, _| {
            d.console.iter().any(|l| l.text.contains("answer=a + b"))
        })
    });

    // F5 continues to the end; the marks go away.
    cx.simulate_keystrokes("f5");
    wait_until(cx, "the end of the session", |cx| {
        debugger.read_with(cx, |d, _| d.session.is_none())
    });
    assert_eq!(editor.read_with(cx, |e, _| e.debug_marks.exec_line), None);
    assert!(debugger.read_with(cx, |d, _| d.console.iter().any(|l| l.text.contains("bye"))));
    assert!(debugger.read_with(cx, |d, _| {
        d.console
            .iter()
            .any(|l| l.text.contains("exited with code 0"))
    }));
    assert!(!Path::new(&root).join(".ferrum").exists());
}
