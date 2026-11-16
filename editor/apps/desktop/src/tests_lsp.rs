//! End to end with a scripted language server (a small Python program):
//! problems, completion while typing, hover, go to definition and back,
//! the problems list, rename across files (with its preview), find all references, code
//! actions (edits and commands that make the server apply an edit),
//! formatting (command and on save) and signature help.

use std::path::{Path, PathBuf};
use std::time::Duration;

use ferrumeditor_core::buffer::Point;
use ferrumeditor_core::config::LspServerConfig;
use ferrumeditor_core::tool_layout::ToolKind;
use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};

use crate::editor::EditorView;
use crate::lsp_store::{GlobalLsp, LspStore};
use crate::settings::Settings;
use crate::tests::init;
use crate::workspace::Workspace;

const FAKE_SERVER: &str = r#"#!/usr/bin/env python3
import json, os, re, sys, urllib.parse

inp, out = sys.stdin.buffer, sys.stdout.buffer
docs = {}
root = "/"
pending_command = None

def path_of(uri):
    return urllib.parse.unquote(urllib.parse.urlparse(uri).path)

def text_of(uri):
    if uri in docs:
        return docs[uri]
    with open(path_of(uri)) as f:
        return f.read()

def word_at(uri, pos):
    line = text_of(uri).split("\n")[pos["line"]]
    for m in re.finditer(r"\w+", line):
        if m.start() <= pos["character"] <= m.end():
            return m.group(0), m.start(), m.end()
    return None

def rng(line, start, end):
    return {"start": {"line": line, "character": start}, "end": {"line": line, "character": end}}

def occurrences(word):
    """(uri, range) of `word` in open documents and the project's .rs files."""
    uris = {path_of(u): u for u in docs}
    for name in sorted(os.listdir(root)):
        path = os.path.join(root, name)
        if name.endswith(".rs") and path not in uris:
            uris[path] = "file://" + urllib.parse.quote(path)
    found = []
    for path in sorted(uris):
        uri = uris[path]
        for n, line in enumerate(text_of(uri).split("\n")):
            for m in re.finditer(r"\b%s\b" % re.escape(word), line):
                found.append((uri, rng(n, m.start(), m.end())))
    return found

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
        root = path_of(params["rootUri"])
        send({"jsonrpc": "2.0", "id": mid, "result": {"capabilities": {
            "textDocumentSync": 1, "hoverProvider": True, "definitionProvider": True,
            "completionProvider": {"triggerCharacters": ["."], "resolveProvider": True},
            "renameProvider": {"prepareProvider": True}, "referencesProvider": True,
            "documentFormattingProvider": True, "codeActionProvider": True,
            "executeCommandProvider": {"commands": ["fake.addComment"]},
            "signatureHelpProvider": {"triggerCharacters": ["("], "retriggerCharacters": [","]}}}})
    elif method == "textDocument/prepareRename":
        word = word_at(params["textDocument"]["uri"], params["position"])
        send({"jsonrpc": "2.0", "id": mid, "result": word and {
            "range": rng(params["position"]["line"], word[1], word[2]), "placeholder": word[0]}})
    elif method == "textDocument/rename":
        word = word_at(params["textDocument"]["uri"], params["position"])[0]
        by_uri = {}
        for uri, r in occurrences(word):
            by_uri.setdefault(uri, []).append({"range": r, "newText": params["newName"]})
        send({"jsonrpc": "2.0", "id": mid, "result": {"documentChanges": [
            {"textDocument": {"uri": uri, "version": None}, "edits": edits}
            for uri, edits in by_uri.items()]}})
    elif method == "textDocument/references":
        word = word_at(params["textDocument"]["uri"], params["position"])[0]
        send({"jsonrpc": "2.0", "id": mid, "result": [
            {"uri": uri, "range": r} for uri, r in occurrences(word)]})
    elif method == "textDocument/formatting":
        edits = []
        for n, line in enumerate(text_of(params["textDocument"]["uri"]).split("\n")):
            stripped = line.rstrip(" ")
            if stripped != line:
                edits.append({"range": rng(n, len(stripped), len(line)), "newText": ""})
        send({"jsonrpc": "2.0", "id": mid, "result": edits})
    elif method == "textDocument/codeAction":
        uri = params["textDocument"]["uri"]
        send({"jsonrpc": "2.0", "id": mid, "result": [
            {"title": "Insert header", "kind": "source",
             "edit": {"changes": {uri: [{"range": rng(0, 0, 0), "newText": "// header\n"}]}}},
            {"title": "Add comment", "kind": "quickfix",
             "command": {"title": "Add comment", "command": "fake.addComment", "arguments": [uri]}}]})
    elif method == "workspace/executeCommand":
        pending_command = mid
        uri = params["arguments"][0]
        send({"jsonrpc": "2.0", "id": "apply-1", "method": "workspace/applyEdit", "params": {
            "label": "Add comment",
            "edit": {"changes": {uri: [{"range": rng(0, 0, 0), "newText": "// added\n"}]}}}})
    elif method is None and mid == "apply-1":
        assert msg["result"]["applied"], msg
        send({"jsonrpc": "2.0", "id": pending_command, "result": None})
    elif method == "textDocument/signatureHelp":
        pos = params["position"]
        line = text_of(params["textDocument"]["uri"]).split("\n")[pos["line"]]
        send({"jsonrpc": "2.0", "id": mid, "result": {
            "signatures": [{"label": "fn helper(a: i32, b: i32)",
                            "parameters": [{"label": "a: i32"}, {"label": "b: i32"}]}],
            "activeParameter": line[:pos["character"]].count(",")}})
    elif method == "textDocument/didOpen":
        docs[params["textDocument"]["uri"]] = params["textDocument"]["text"]
        publish(params["textDocument"]["uri"])
    elif method == "textDocument/didChange":
        docs[params["textDocument"]["uri"]] = params["contentChanges"][-1]["text"]
        publish(params["textDocument"]["uri"])
    elif method == "textDocument/completion":
        send({"jsonrpc": "2.0", "id": mid, "result": {"isIncomplete": False, "items": [
            {"label": "helper", "kind": 3, "insertText": "helper()", "data": 1},
            {"label": "hello_world", "kind": 3, "insertText": "hello_world($1)",
             "insertTextFormat": 2}]}})
    elif method == "completionItem/resolve":
        item = dict(params)
        item["detail"] = "fn helper()"
        item["additionalTextEdits"] = [{"range": {"start": {"line": 0, "character": 0},
            "end": {"line": 0, "character": 0}}, "newText": "use helper;\n"}]
        send({"jsonrpc": "2.0", "id": mid, "result": item})
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
    // Enter resolves the item first: its auto-import lands at the top.
    cx.simulate_keystrokes("enter");
    wait_until(cx, "resolved completion", |cx| {
        editor.read_with(cx, |e, _| e.state.buffer().line(4) == "    helper()")
    });
    editor.update_in(cx, |e, window, cx| {
        assert_eq!(e.state.buffer().line(0), "use helper;");
        assert!(e.completion_labels().is_none());
        // Drop the import again so the lines below keep their numbers.
        e.state.replace_ranges(vec![(0..12, String::new())]);
        e.run_command("editor:move-to-line-end", window, cx);
    });
    editor.read_with(cx, |e, _| {
        assert_eq!(e.state.buffer().line(3), "    helper()")
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

#[gpui::test]
fn build_task_runs_in_the_terminal_and_reports_problems(cx: &mut TestAppContext) {
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo\"\n").expect("write");
    std::fs::create_dir_all(root.join("src")).expect("mkdir");
    let main = root.join("src/main.rs");
    std::fs::write(&main, "fn main() {\n    let x: u8 = \"a\";\n}\n").expect("write");
    cx.update(|cx| {
        let store = cx.new(|_| LspStore::with_path(String::new()));
        cx.set_global(GlobalLsp(store));
    });
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    workspace.update_in(cx, |w, window, cx| {
        w.set_root(root.clone(), window, cx);
        w.open_file(&main, window, cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes(&crate::tests::primary("cmd-b"));
    cx.run_until_parked();
    let ran = |cx: &mut VisualTestContext| {
        workspace.read_with(cx, |w, _| w.tools().dry_run.clone().unwrap_or_default())
    };
    assert_eq!(ran(cx), vec!["cargo build".to_string()]);
    assert!(workspace.read_with(cx, |w, _| w.tool_open(ToolKind::Term)));
    workspace.update(cx, |w, cx| {
        w.finish_last_task(
            "   Compiling demo v0.1.0\nerror[E0308]: mismatched types\n --> src/main.rs:2:17\n",
            101,
            cx,
        )
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let store = LspStore::global(cx).expect("store");
        assert_eq!(store.read(cx).counts(), (1, 0));
    });
    let editor = active(&workspace, cx);
    editor.read_with(cx, |e, _| {
        let d = &e.lsp.diagnostics[0];
        assert_eq!(
            e.state.buffer().offset_to_point(d.range.start),
            Point::new(1, 16)
        );
        assert_eq!(d.message, "mismatched types [E0308] (cargo)");
    });
    // A clean rerun clears the problems.
    workspace.update_in(cx, |w, window, cx| {
        w.run_command("tasks:rerun-last", window, cx)
    });
    cx.run_until_parked();
    assert_eq!(ran(cx).len(), 2);
    workspace.update(cx, |w, cx| w.finish_last_task("    Finished dev\n", 0, cx));
    cx.run_until_parked();
    editor.read_with(cx, |e, _| assert!(e.lsp.diagnostics.is_empty()));
}

#[gpui::test]
fn missing_server_is_installed_and_started(cx: &mut TestAppContext) {
    if !has_python() {
        eprintln!("skipping: python3 is not installed");
        return;
    }
    cx.executor().allow_parking();
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    let server = write_server(&root);
    let tools = root.join("tools");
    std::fs::create_dir_all(&tools).expect("mkdir");
    // A stand-in npm: "installs" typescript-language-server as a script
    // that runs the scripted server.
    let npm = tools.join("npm");
    std::fs::write(
        &npm,
        format!(
            "#!/bin/sh\nprefix=\"$3\"\nmkdir -p \"$prefix/node_modules/.bin\"\nprintf '#!/bin/sh\\nexec python3 {}\\n' > \"$prefix/node_modules/.bin/typescript-language-server\"\nchmod +x \"$prefix/node_modules/.bin/typescript-language-server\"\necho installed\n",
            server.display()
        ),
    )
    .expect("write npm");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&npm, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let file = root.join("app.ts");
    std::fs::write(&file, "const x = bad;\n").expect("write");
    let servers_dir = root.join("servers");
    let path_env = format!(
        "{}:{}",
        tools.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    // Only the stand-in npm may be found, not a real one on this machine.
    let path_env = std::env::split_paths(&path_env)
        .filter(|d| {
            *d == tools || !d.join("npm").exists() && !d.join("typescript-language-server").exists()
        })
        .map(|d| d.display().to_string())
        .collect::<Vec<_>>()
        .join(":");
    cx.update(|cx| {
        let store = cx.new(|_| LspStore::with_install_dir(path_env, servers_dir.clone()));
        cx.set_global(GlobalLsp(store));
    });
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    workspace.update_in(cx, |w, window, cx| {
        w.set_root(root.clone(), window, cx);
        w.open_file(&file, window, cx);
    });
    let editor = active(&workspace, cx);
    wait_until(cx, "install, start and diagnostics", |cx| {
        editor.read_with(cx, |e, _| !e.lsp.diagnostics.is_empty())
    });
    assert!(
        servers_dir
            .join("typescript-language-server/node_modules/.bin/typescript-language-server")
            .is_file()
    );
    assert!(servers_dir.join("logs/typescript.log").is_file());
    editor.read_with(cx, |e, _| assert!(e.has_language_server()));
}

/// Starts the scripted server for Rust files in `root` and opens `file`.
fn open_with_server<'a>(
    root: &Path,
    file: &Path,
    cx: &'a mut TestAppContext,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    let server = write_server(root);
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
    let (root, file) = (root.to_path_buf(), file.to_path_buf());
    workspace.update_in(cx, |w, window, cx| {
        w.set_root(root, window, cx);
        w.open_file(&file, window, cx);
    });
    (workspace, cx)
}

fn line(editor: &Entity<EditorView>, row: usize, cx: &mut VisualTestContext) -> String {
    editor.read_with(cx, |e, _| e.state.buffer().line(row).to_string())
}

#[gpui::test]
fn rename_references_code_actions_and_formatting(cx: &mut TestAppContext) {
    if !has_python() {
        eprintln!("skipping: python3 is not installed");
        return;
    }
    cx.executor().allow_parking();
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    let main = root.join("main.rs");
    let other = root.join("other.rs");
    std::fs::write(
        &main,
        "fn helper(a: i32, b: i32) {}\nfn main() {\n    helper(1, 2);   \n}\n",
    )
    .expect("write");
    std::fs::write(&other, "fn other() {\n    helper(3, 4);\n}\n").expect("write");
    let (workspace, cx) = open_with_server(&root, &main, cx);
    let editor = active(&workspace, cx);
    wait_until(cx, "the server", |cx| {
        editor.read_with(cx, |e, _| e.server_ready())
    });
    editor.update_in(cx, |e, window, cx| {
        e.state.go_to_point(2, 6);
        e.focus(window, cx);
    });

    // Shift+F12 lists every reference, grouped by file, in the bottom panel.
    cx.simulate_keystrokes("shift-f12");
    let panel = workspace.read_with(cx, |w, _| w.project_find().clone());
    wait_until(cx, "references", |cx| {
        workspace.read_with(cx, |w, _| w.project_find_visible())
    });
    panel.read_with(cx, |p, _| {
        assert_eq!(p.status().as_ref(), "3 references to `helper` in 2 files");
        assert_eq!(p.result_rows(), 5);
    });
    panel.update(cx, |p, cx| p.open_first_match(cx));
    cx.run_until_parked();
    assert_eq!(cursor(&editor, cx), Point::new(0, 3));

    // F2 opens the inline input with the name selected; Enter renames in
    // the open buffer and in the file that is not open.
    editor.update_in(cx, |e, window, cx| e.focus(window, cx));
    cx.simulate_keystrokes("f2");
    wait_until(cx, "rename input", |cx| {
        editor.read_with(cx, |e, _| e.rename_open())
    });
    cx.simulate_input("assist");
    cx.simulate_keystrokes("enter");
    // The preview lists both files; nothing changes until Enter applies.
    wait_until(cx, "rename preview", |cx| {
        workspace.read_with(cx, |w, _| w.edit_preview_open())
    });
    assert_eq!(line(&editor, 0, cx), "fn helper(a: i32, b: i32) {}");
    cx.simulate_keystrokes("escape");
    assert!(!workspace.read_with(cx, |w, _| w.edit_preview_open()));
    assert_eq!(line(&editor, 0, cx), "fn helper(a: i32, b: i32) {}");
    cx.simulate_keystrokes("f2");
    wait_until(cx, "rename input again", |cx| {
        editor.read_with(cx, |e, _| e.rename_open())
    });
    cx.simulate_input("assist");
    cx.simulate_keystrokes("enter");
    wait_until(cx, "rename preview again", |cx| {
        workspace.read_with(cx, |w, _| w.edit_preview_open())
    });
    cx.simulate_keystrokes("enter");
    wait_until(cx, "rename", |cx| {
        line(&editor, 0, cx).starts_with("fn assist(")
    });
    assert_eq!(line(&editor, 2, cx), "    assist(1, 2);   ");
    assert!(!editor.read_with(cx, |e, _| e.rename_open()));
    assert_eq!(
        std::fs::read_to_string(&other).expect("read"),
        "fn other() {\n    assist(3, 4);\n}\n"
    );
    // One undo step reverts the open buffer.
    cx.simulate_keystrokes(&crate::tests::primary("cmd-z"));
    assert_eq!(line(&editor, 0, cx), "fn helper(a: i32, b: i32) {}");
    assert_eq!(line(&editor, 2, cx), "    helper(1, 2);   ");

    // Shift+Alt+F formats; saving formats too.
    cx.simulate_keystrokes("shift-alt-f");
    wait_until(cx, "format", |cx| {
        line(&editor, 2, cx) == "    helper(1, 2);"
    });
    editor.update_in(cx, |e, window, cx| {
        e.state.go_to_point(2, 17);
        e.focus(window, cx);
    });
    cx.simulate_input("  ");
    assert_eq!(line(&editor, 2, cx), "    helper(1, 2);  ");
    cx.simulate_keystrokes(&crate::tests::primary("cmd-s"));
    wait_until(cx, "format on save", |cx| {
        !editor.read_with(cx, |e, _| e.is_dirty())
    });
    assert_eq!(
        std::fs::read_to_string(&main).expect("read"),
        "fn helper(a: i32, b: i32) {}\nfn main() {\n    helper(1, 2);\n}\n"
    );

    // Cmd+. lists the actions, quick fixes first. A command action makes
    // the server send workspace/applyEdit; an edit action applies directly.
    cx.simulate_keystrokes(&crate::tests::primary("cmd-."));
    wait_until(cx, "code actions", |cx| {
        workspace.read_with(cx, |w, _| w.picker_open())
    });
    cx.simulate_keystrokes("enter");
    wait_until(cx, "command edit", |cx| line(&editor, 0, cx) == "// added");
    cx.simulate_keystrokes(&crate::tests::primary("cmd-."));
    wait_until(cx, "code actions again", |cx| {
        workspace.read_with(cx, |w, _| w.picker_open())
    });
    cx.simulate_keystrokes("down enter");
    wait_until(cx, "edit action", |cx| line(&editor, 0, cx) == "// header");
    assert_eq!(line(&editor, 1, cx), "// added");

    // Typing `(` shows the signature; Escape hides it.
    editor.update_in(cx, |e, window, cx| {
        e.state.go_to_point(5, 0);
        e.focus(window, cx);
    });
    cx.simulate_input("helper(");
    wait_until(cx, "signature help", |cx| {
        editor.read_with(cx, |e, _| e.signature_label().is_some())
    });
    assert_eq!(
        editor.read_with(cx, |e, _| e.signature_label()).as_deref(),
        Some("fn helper(a: i32, b: i32)")
    );
    cx.simulate_keystrokes("escape");
    assert!(editor.read_with(cx, |e, _| e.signature_label().is_none()));
}
