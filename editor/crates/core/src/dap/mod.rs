//! Debug Adapter Protocol support without GPUI: the client (stdio or TCP,
//! the language servers' framing), the session state and the arguments of
//! every request, launch configurations (`.vscode/launch.json` and
//! defaults), the adapters for Rust (`lldb-dap`, CodeLLDB), Go (`dlv dap`)
//! and JavaScript / TypeScript (js-debug), and the breakpoints saved per
//! project. The desktop app runs the session and draws it.

pub mod adapters;
pub mod breakpoints;
pub mod client;
pub mod launch;
pub mod state;

pub use adapters::{AdapterKind, Missing};
pub use breakpoints::Breakpoints;
pub use client::{AdapterCommand, DapClient, DapEvent, Transport};
pub use launch::LaunchConfig;
pub use state::{DebugState, Followup, Phase};

#[cfg(test)]
mod tests {
    //! A whole session against a scripted adapter (a small Python program,
    //! like the fake language server of the desktop tests): handshake,
    //! breakpoints, a stop with stack / scopes / variables, evaluate, a
    //! step, output, exit — once over stdio and once over TCP.

    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use futures::StreamExt;
    use futures::channel::mpsc::{UnboundedReceiver, unbounded};
    use futures::executor::block_on;
    use serde_json::{Value, json};

    use super::state::{self, DebugState, Followup, Phase};
    use super::*;

    const FAKE_ADAPTER: &str = r#"#!/usr/bin/env python3
import json, socket, sys

if len(sys.argv) > 2 and sys.argv[1] == "--port":
    server = socket.socket()
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", int(sys.argv[2])))
    server.listen(1)
    conn, _ = server.accept()
    inp, out = conn.makefile("rb"), conn.makefile("wb")
else:
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

line = 0
bps = []
launch = None
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
    def reply(body=None, ok=True, message=None):
        r = {"type": "response", "request_seq": msg["seq"], "command": cmd, "success": ok, "body": body or {}}
        if message: r["message"] = message
        send(r)
    if cmd == "initialize":
        reply({"supportsConfigurationDoneRequest": True, "supportsEvaluateForHovers": True})
    elif cmd == "launch":
        launch = msg  # answered after configurationDone, like debugpy / js-debug
        program = args["program"]
        event("initialized", {})
    elif cmd == "setBreakpoints":
        bps = [b["line"] for b in args["breakpoints"]]
        reply({"breakpoints": [{"id": i + 1, "verified": l != 99, "line": l} for i, l in enumerate(bps)]})
    elif cmd == "setExceptionBreakpoints":
        reply({})
    elif cmd == "configurationDone":
        reply()
        send({"type": "response", "request_seq": launch["seq"], "command": "launch", "success": True, "body": {}})
        line = bps[0]
        event("stopped", {"reason": "breakpoint", "threadId": 1, "allThreadsStopped": True})
    elif cmd == "threads":
        reply({"threads": [{"id": 1, "name": "main"}]})
    elif cmd == "stackTrace":
        reply({"stackFrames": [{"id": 10, "name": "main", "line": line, "column": 1,
               "source": {"path": program}}], "totalFrames": 1})
    elif cmd == "scopes":
        reply({"scopes": [{"name": "Locals", "variablesReference": 100, "expensive": False}]})
    elif cmd == "variables":
        if args["variablesReference"] == 100:
            reply({"variables": [{"name": "count", "value": str(line), "type": "int", "variablesReference": 0},
                                 {"name": "items", "value": "[2]", "variablesReference": 101}]})
        else:
            reply({"variables": [{"name": "[0]", "value": "7", "variablesReference": 0}]})
    elif cmd == "evaluate":
        if args["expression"] == "count * 2":
            reply({"result": str(line * 2), "variablesReference": 0})
        else:
            reply(ok=False, message="cannot evaluate")
    elif cmd == "next":
        reply()
        line += 1
        event("stopped", {"reason": "step", "threadId": 1})
    elif cmd == "continue":
        reply({"allThreadsContinued": True})
        event("continued", {"threadId": 1})
        event("output", {"category": "stdout", "output": "done\n"})
        event("exited", {"exitCode": 3})
        event("terminated", {})
    elif cmd == "disconnect":
        reply()
        sys.exit(0)
    else:
        reply(ok=False, message="unsupported " + str(cmd))
"#;

    fn fake_adapter(dir: &Path) -> PathBuf {
        let path = dir.join("fake-adapter.py");
        std::fs::write(&path, FAKE_ADAPTER).expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        path
    }

    fn have_python() -> bool {
        std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_ok()
    }

    fn next_event(events: &mut UnboundedReceiver<DapEvent>) -> (String, Value) {
        loop {
            match block_on(events.next()) {
                Some(DapEvent::Event { event, body }) => return (event, body),
                Some(DapEvent::Request { .. }) => {}
                other => panic!("adapter ended: {other:?}"),
            }
        }
    }

    /// Applies events until one asks for something; returns the followups.
    fn pump(state: &mut DebugState, events: &mut UnboundedReceiver<DapEvent>) -> Vec<Followup> {
        loop {
            let (event, body) = next_event(events);
            let followups = state.on_event(&event, &body);
            if !followups.is_empty() {
                return followups;
            }
        }
    }

    /// What the desktop does for `FetchStack`: threads, stack, scopes and
    /// the first scope's variables.
    fn fetch_stack(client: &DapClient, state: &mut DebugState, thread: Option<i64>) {
        let threads = block_on(client.request("threads", Value::Null)).expect("threads");
        state.set_threads(&threads);
        let thread = thread.or(state.thread_id).expect("a thread");
        let stack =
            block_on(client.request("stackTrace", state::stack_trace_args(thread))).expect("stack");
        state.set_stack(thread, &stack);
        let frame = state.selected_frame().expect("frame").id;
        let scopes = block_on(client.request("scopes", state::scopes_args(frame))).expect("scopes");
        state.set_scopes(&scopes);
        let reference = state.scopes[0].variables_reference;
        let vars =
            block_on(client.request("variables", state::variables_args(reference))).expect("vars");
        state.set_variables(reference, &vars);
    }

    fn run_session(client: DapClient, mut events: UnboundedReceiver<DapEvent>, program: &Path) {
        let mut state = DebugState::new();
        let caps =
            block_on(client.request("initialize", state::initialize_args("fake"))).expect("init");
        state.set_capabilities(&caps);
        assert_eq!(state.capabilities["supportsConfigurationDoneRequest"], true);
        let launch = client.request("launch", json!({"program": program.to_string_lossy()}));

        assert_eq!(pump(&mut state, &mut events), [Followup::Configure]);
        let lines = [4, 99];
        let response = block_on(client.request(
            "setBreakpoints",
            state::set_breakpoints_args(program, &lines),
        ))
        .expect("breakpoints");
        state.set_breakpoint_results(program, &lines, &response);
        assert_eq!(state.verified(program, 4), Some(true));
        assert_eq!(state.verified(program, 99), Some(false));
        block_on(client.request("setExceptionBreakpoints", json!({"filters": []})))
            .expect("exceptions");
        block_on(client.request("configurationDone", Value::Null)).expect("configured");
        block_on(launch).expect("launched");

        let followups = pump(&mut state, &mut events);
        assert_eq!(followups, [Followup::FetchStack { thread_id: Some(1) }]);
        fetch_stack(&client, &mut state, Some(1));
        assert_eq!(state.current_location(), Some((program.to_path_buf(), 4)));
        let locals = &state.variables[&100];
        assert_eq!(
            (locals[0].name.as_str(), locals[0].value.as_str()),
            ("count", "4")
        );
        let children =
            block_on(client.request("variables", state::variables_args(101))).expect("children");
        state.set_variables(101, &children);
        assert_eq!(state.variables[&101][0].value, "7");

        let frame = state.selected_frame().map(|f| f.id);
        let result =
            block_on(client.request("evaluate", state::evaluate_args("count * 2", frame, "repl")))
                .expect("evaluate");
        assert_eq!(state::evaluate_result(&result).0, "8");
        assert_eq!(
            block_on(client.request("evaluate", state::evaluate_args("nope", frame, "watch"))),
            Err("cannot evaluate".into())
        );

        block_on(client.request("next", state::thread_args(1))).expect("next");
        state.resumed();
        assert_eq!(state.phase(), Phase::Running);
        let followups = pump(&mut state, &mut events);
        fetch_stack(
            &client,
            &mut state,
            followups.iter().find_map(|f| match f {
                Followup::FetchStack { thread_id } => *thread_id,
                _ => None,
            }),
        );
        assert_eq!(state.current_location().map(|l| l.1), Some(5));
        assert_eq!(state.phase().label(), "paused (step)");

        block_on(client.request("continue", state::thread_args(1))).expect("continue");
        assert_eq!(pump(&mut state, &mut events), [Followup::End]);
        assert_eq!(state.exit_code, Some(3));
        assert_eq!(state.output.last().map(|o| o.text.as_str()), Some("done\n"));
        block_on(client.request("disconnect", state::disconnect_args(true))).expect("disconnect");
    }

    #[test]
    fn full_session_over_stdio() {
        if !have_python() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let program = dir.path().join("main.rs");
        let command = AdapterCommand {
            name: "fake".into(),
            program: fake_adapter(dir.path()),
            args: vec![],
            cwd: Some(dir.path().to_path_buf()),
            path_env: None,
            transport: Transport::Stdio,
        };
        let (tx, rx) = unbounded();
        let client = DapClient::spawn(&command, tx).expect("spawn");
        run_session(client, rx, &program);
    }

    #[test]
    fn full_session_over_tcp() {
        if !have_python() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let program = dir.path().join("main.go");
        let command = AdapterCommand {
            name: "fake".into(),
            program: fake_adapter(dir.path()),
            args: vec!["--port".into(), "{port}".into()],
            cwd: None,
            path_env: None,
            transport: Transport::Tcp,
        };
        let (tx, rx) = unbounded();
        let client = DapClient::spawn(&command, tx).expect("spawn");
        assert!(client.address().is_some());
        run_session(client, rx, &program);
    }

    /// The real `lldb-dap` on a tiny C program: stop at a breakpoint, read
    /// a local, continue to the exit code. `cargo test -- --ignored`.
    #[test]
    #[ignore = "needs lldb-dap and a C compiler"]
    fn lldb_dap_debugs_a_c_program() {
        let path_env = std::env::var("PATH").unwrap_or_default();
        let Some(lldb_dap) = adapters::find_lldb_dap(&path_env) else {
            eprintln!("skipping: no lldb-dap");
            return;
        };
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canonical");
        let source = root.join("main.c");
        std::fs::write(
            &source,
            "#include <stdio.h>\nint main(void) {\n    int answer = 21;\n    answer = answer * 2;\n    printf(\"%d\\n\", answer);\n    return 3;\n}\n",
        )
        .expect("write");
        let program = root.join("main");
        let built = std::process::Command::new("cc")
            .args(["-g", "-O0", "-o"])
            .arg(&program)
            .arg(&source)
            .status()
            .expect("cc");
        assert!(built.success());
        let command = AdapterCommand {
            name: "lldb-dap".into(),
            program: lldb_dap,
            args: vec![],
            cwd: Some(root.clone()),
            path_env: None,
            transport: Transport::Stdio,
        };
        let (tx, mut events) = unbounded();
        let client = DapClient::spawn(&command, tx).expect("spawn");
        let mut state = DebugState::new();
        let caps = block_on(client.request("initialize", state::initialize_args("lldb-dap")))
            .expect("init");
        state.set_capabilities(&caps);
        let args = adapters::launch_arguments(
            AdapterKind::LldbDap,
            &json!({"program": program.to_string_lossy(), "cwd": root.to_string_lossy(), "env": {}}),
        );
        let launch = client.request("launch", args);
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        let mut launched = Some(launch);
        loop {
            assert!(std::time::Instant::now() < deadline, "timed out");
            let (event, body) = next_event(&mut events);
            for followup in state.on_event(&event, &body) {
                match followup {
                    Followup::Configure => {
                        let response =
                            block_on(client.request(
                                "setBreakpoints",
                                state::set_breakpoints_args(&source, &[4]),
                            ))
                            .expect("breakpoints");
                        state.set_breakpoint_results(&source, &[4], &response);
                        block_on(client.request("configurationDone", Value::Null)).expect("done");
                        if let Some(launch) = launched.take() {
                            block_on(launch).expect("launch");
                        }
                    }
                    Followup::FetchStack { thread_id } => {
                        fetch_stack(&client, &mut state, thread_id);
                        let location = state.current_location().expect("location");
                        assert_eq!(location.1, 4);
                        let answer = state
                            .variables
                            .values()
                            .flatten()
                            .find(|v| v.name == "answer");
                        assert_eq!(answer.map(|v| v.value.as_str()), Some("21"));
                        let thread = state.thread_id.expect("thread");
                        block_on(client.request("continue", state::thread_args(thread)))
                            .expect("continue");
                        state.resumed();
                    }
                    Followup::FetchThreads | Followup::End => {}
                }
            }
            if matches!(state.phase(), Phase::Exited { .. } | Phase::Terminated)
                && state.exit_code.is_some()
            {
                break;
            }
        }
        assert_eq!(state.exit_code, Some(3));
        block_on(client.request("disconnect", state::disconnect_args(true))).ok();
    }
}
