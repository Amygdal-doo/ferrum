//! End to end with a scripted language server (a small Python program):
//! problems, completion while typing, hover, go to definition and back,
//! and the problems list.

use std::path::{Path, PathBuf};
use std::time::Duration;

use ferrumeditor_core::buffer::Point;
use ferrumeditor_core::config::LspServerConfig;
use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};

use crate::editor::EditorView;
use crate::lsp_store::{GlobalLsp, LspStore};
use crate::settings::Settings;
use crate::tests::init;
use crate::workspace::Workspace;

const FAKE_SERVER: &str = r#"#!/usr/bin/env python3
import json, re, sys

inp, out = sys.stdin.buffer, sys.stdout.buffer
docs = {}

def send(msg):
    body = json.dumps(msg).encode()
    out.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
    out.flush()

def publish(uri):
    diags = []
    for line, text in enumerate(docs[uri].split("\n")):
        for m in re.finditer(r"\bbad\b", text):
            diags.append({"range": {"start": {"line": line, "character": m.start()},
                                    "end": {"line": line, "character": m.end()}},
                          "severity": 1, "message": "unknown name `bad`", "source": "fake"})
    send({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
          "params": {"uri": uri, "diagnostics": diags}})

while True:
    length = None
    while True:
        line = inp.readline()
        if not line:
            sys.exit(0)
        line = line.strip()
        if not line:
            break
        if line.lower().startswith(b"content-length:"):
            length = int(line.split(b":")[1])
    msg = json.loads(inp.read(length))
    method, params, mid = msg.get("method"), msg.get("params") or {}, msg.get("id")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": mid, "result": {"capabilities": {
            "textDocumentSync": 1, "hoverProvider": True, "definitionProvider": True,
            "completionProvider": {"triggerCharacters": ["."]}}}})
    elif method == "textDocument/didOpen":
        docs[params["textDocument"]["uri"]] = params["textDocument"]["text"]
        publish(params["textDocument"]["uri"])
    elif method == "textDocument/didChange":
        docs[params["textDocument"]["uri"]] = params["contentChanges"][-1]["text"]
        publish(params["textDocument"]["uri"])
    elif method == "textDocument/completion":
        send({"jsonrpc": "2.0", "id": mid, "result": {"isIncomplete": False, "items": [
            {"label": "helper", "kind": 3, "detail": "fn helper()", "insertText": "helper()"},
            {"label": "hello_world", "kind": 3, "insertText": "hello_world($1)",
             "insertTextFormat": 2}]}})
    elif method == "textDocument/definition":
        send({"jsonrpc": "2.0", "id": mid, "result": {"uri": params["textDocument"]["uri"],
            "range": {"start": {"line": 0, "character": 3}, "end": {"line": 0, "character": 9}}}})
    elif method == "textDocument/hover":
        send({"jsonrpc": "2.0", "id": mid, "result": {"contents": {"kind": "markdown",
            "value": "```rust\nfn helper()\n```"}}})
    elif method == "shutdown":
        send({"jsonrpc": "2.0", "id": mid, "result": None})
    elif method == "exit":
        sys.exit(0)
"#;

fn has_python() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn write_server(dir: &Path) -> PathBuf {
    let path = dir.join("fake-lsp.py");
    std::fs::write(&path, FAKE_SERVER).expect("write server");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    path
}

/// Runs the app (and its timers) until `done` or a few seconds pass; the
/// server answers from real threads.
fn wait_until(
    cx: &mut VisualTestContext,
    what: &str,
    mut done: impl FnMut(&mut VisualTestContext) -> bool,
) {
    for _ in 0..400 {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        cx.executor().advance_clock(Duration::from_millis(50));
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {what}");
}

fn active(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    workspace
        .read_with(cx, |w, _| w.active_editor())
        .expect("an editor")
}

fn cursor(editor: &Entity<EditorView>, cx: &mut VisualTestContext) -> Point {
    editor.read_with(cx, |e, _| e.cursor_point())
}

#[gpui::test]
fn language_server_problems_completion_and_definition(cx: &mut TestAppContext) {
    if !has_python() {
        eprintln!("skipping: python3 is not installed");
        return;
    }
    // The server answers from real threads.
    cx.executor().allow_parking();
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    let server = write_server(&root);
    let file = root.join("main.rs");
    std::fs::write(&file, "fn helper() {}\nfn main() {\n    bad;\n    hel\n}\n").expect("write");
    cx.update(|cx| {
        let settings = cx.global_mut::<Settings>();
        settings.config.lsp.servers.insert(
            "rust".into(),
            LspServerConfig {
                command: Some(server.to_string_lossy().to_string()),
                ..Default::default()
            },
        );
        let path = std::env::var("PATH").unwrap_or_default();
        let store = cx.new(|_| LspStore::with_path(path));
        cx.set_global(GlobalLsp(store));
    });
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    workspace.update_in(cx, |w, window, cx| {
        w.set_root(root.clone(), window, cx);
        w.open_file(&file, window, cx);
    });
    let editor = active(&workspace, cx);

    // Problems arrive and are mapped onto the buffer.
    wait_until(cx, "diagnostics", |cx| {
        editor.read_with(cx, |e, _| !e.lsp.diagnostics.is_empty())
    });
    editor.read_with(cx, |e, _| {
        let d = &e.lsp.diagnostics[0];
        assert_eq!(e.state.buffer().slice(d.range.clone()), "bad");
        assert_eq!(d.message, "unknown name `bad` (fake)");
    });
    cx.update(|_, cx| {
        let store = LspStore::global(cx).expect("store");
        assert_eq!(store.read(cx).counts(), (1, 0));
    });

    // Typing asks for completions; the menu filters as the word grows.
    editor.update_in(cx, |e, window, cx| {
        e.state.go_to_point(3, 7);
        e.focus(window, cx);
    });
    cx.simulate_input("p");
    wait_until(cx, "completions", |cx| {
        editor.read_with(cx, |e, _| e.completion_labels().is_some())
    });
    editor.read_with(cx, |e, _| {
        assert_eq!(e.completion_labels(), Some(vec!["helper".to_string()]));
    });
    cx.simulate_keystrokes("enter");
    editor.read_with(cx, |e, _| {
        assert_eq!(e.state.buffer().line(3), "    helper()");
        assert!(e.completion_labels().is_none());
    });

    // Escape closes the menu without inserting anything.
    cx.simulate_keystrokes("ctrl-space");
    wait_until(cx, "manual completions", |cx| {
        editor.read_with(cx, |e, _| e.completion_labels().is_some())
    });
    cx.simulate_keystrokes("escape");
    editor.read_with(cx, |e, _| {
        assert!(e.completion_labels().is_none());
        assert_eq!(e.state.buffer().line(3), "    helper()");
    });

    // Hover over the problem: the message at once, the type when it comes.
    editor.update_in(cx, |e, window, cx| {
        e.state.go_to_point(2, 5);
        e.run_command("editor:show-hover", window, cx);
    });
    editor.read_with(cx, |e, _| {
        let hover = e.hover_popup().expect("hover");
        assert_eq!(hover.diagnostics.len(), 1);
    });

    // F12 jumps to the definition; navigation:back returns.
    editor.update_in(cx, |e, _, _| e.state.go_to_point(3, 6));
    cx.simulate_keystrokes("f12");
    wait_until(cx, "go to definition", |cx| {
        cursor(&editor, cx) == Point::new(0, 3)
    });
    cx.simulate_keystrokes("ctrl--");
    cx.run_until_parked();
    assert_eq!(cursor(&editor, cx), Point::new(3, 6));
    cx.simulate_keystrokes("ctrl-shift--");
    cx.run_until_parked();
    assert_eq!(cursor(&editor, cx), Point::new(0, 3));

    // The problems list opens the problem.
    workspace.update_in(cx, |w, window, cx| {
        w.run_command("problems:show", window, cx)
    });
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |w, _| w.picker_open()));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(cursor(&editor, cx), Point::new(2, 4));

    // Fixing the code clears the problem.
    editor.update_in(cx, |e, window, cx| {
        e.state.go_to_point(2, 4);
        e.run_command("editor:delete-word-right", window, cx);
    });
    wait_until(cx, "cleared diagnostics", |cx| {
        editor.read_with(cx, |e, _| e.lsp.diagnostics.is_empty())
    });
    cx.update(|_, cx| {
        let store = LspStore::global(cx).expect("store");
        assert_eq!(store.read(cx).counts(), (0, 0));
    });
}

#[gpui::test]
fn missing_server_is_reported_not_fatal(cx: &mut TestAppContext) {
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    let file = root.join("main.go");
    std::fs::write(&file, "package main\n").expect("write");
    cx.update(|cx| {
        // An empty PATH: gopls can't be found.
        let store = cx.new(|_| LspStore::with_path(String::new()));
        cx.set_global(GlobalLsp(store));
    });
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    workspace.update_in(cx, |w, window, cx| {
        w.open_file(&file, window, cx);
    });
    cx.run_until_parked();
    let editor = active(&workspace, cx);
    editor.read_with(cx, |e, _| assert!(!e.has_language_server()));
    let status = cx.update(|_, cx| {
        let store = LspStore::global(cx).expect("store");
        let e = editor.read(cx);
        store.read(cx).status_for(e.server_id(), e.language())
    });
    assert!(matches!(
        status,
        Some(crate::lsp_store::ServerStatus::Missing { ref install, .. }) if install.contains("gopls")
    ));
    // F12 explains instead of failing silently; typing still works.
    cx.simulate_keystrokes("f12");
    cx.simulate_input("x");
    cx.run_until_parked();
    editor.read_with(cx, |e, _| assert!(e.state.text().contains('x')));
}
