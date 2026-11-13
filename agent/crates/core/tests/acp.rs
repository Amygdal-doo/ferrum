//! ACP against a scripted fake agent (this test binary re-run with
//! `FERRUM_FAKE_ACP=1`): streamed text, a tool call with a permission
//! request, file read/write through the change set, a terminal, a plan,
//! mode switching, and Ask mode refusing writes.

mod common;

use std::collections::BTreeMap;

use common::{Events, Peer, fake_process};
use ferrumagent_core::acp::AcpAgentConfig;
use ferrumagent_core::modes::Mode;
use ferrumagent_core::permissions::Decision;
use ferrumagent_core::session::{AgentChoice, ChatSession, SessionConfig, SessionEvent};
use ferrumagent_core::thread::{Block, ToolStatus};
use serde_json::json;

#[test]
fn fake_acp_agent() {
    if std::env::var("FERRUM_FAKE_ACP").is_err() {
        return;
    }
    let mut peer = Peer::new();
    let mut cwd = String::new();
    while let Some(message) = peer.read() {
        let id = message["id"].clone();
        let params = &message["params"];
        match message["method"].as_str().unwrap_or_default() {
            "initialize" => {
                assert_eq!(params["protocolVersion"], 1);
                assert_eq!(params["clientCapabilities"]["fs"]["writeTextFile"], true);
                peer.reply(
                    &id,
                    json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": false}, "agentInfo": {"name": "fake-agent", "version": "0"}, "authMethods": []}),
                );
            }
            "session/new" => {
                cwd = params["cwd"].as_str().unwrap_or_default().to_string();
                peer.reply(
                    &id,
                    json!({"sessionId": "s1", "modes": {"currentModeId": "default", "availableModes": [{"id": "default", "name": "Default"}, {"id": "plan", "name": "Plan"}]}}),
                );
            }
            "session/set_mode" => peer.reply(&id, json!({})),
            "session/prompt" => {
                let update = |peer: &mut Peer, update: serde_json::Value| {
                    peer.notify(
                        "session/update",
                        json!({"sessionId": "s1", "update": update}),
                    );
                };
                update(
                    &mut peer,
                    json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "Working"}}),
                );
                update(
                    &mut peer,
                    json!({"sessionUpdate": "tool_call", "toolCallId": "c1", "title": "Edit a.txt", "kind": "edit", "status": "pending"}),
                );
                let mut others = Vec::new();
                let answer = peer.call(
                    "session/request_permission",
                    json!({"sessionId": "s1", "toolCall": {"toolCallId": "c1", "kind": "edit", "title": "Edit a.txt"},
                           "options": [{"optionId": "yes", "name": "Allow", "kind": "allow_once"}, {"optionId": "no", "name": "Reject", "kind": "reject_once"}]}),
                    &mut others,
                );
                let path = format!("{cwd}/a.txt");
                if answer["result"]["outcome"]["optionId"] == "yes" {
                    let read = peer.call(
                        "fs/read_text_file",
                        json!({"sessionId": "s1", "path": path}),
                        &mut others,
                    );
                    let text = read["result"]["content"]
                        .as_str()
                        .unwrap_or_default()
                        .to_uppercase();
                    let wrote = peer.call(
                        "fs/write_text_file",
                        json!({"sessionId": "s1", "path": path, "content": text}),
                        &mut others,
                    );
                    let escape = peer.call("fs/write_text_file", json!({"sessionId": "s1", "path": "/tmp/ferrum-acp-escape.txt", "content": "x"}), &mut others);
                    let term = peer.call(
                        "terminal/create",
                        json!({"sessionId": "s1", "command": "sh", "args": ["-c", "echo hi"]}),
                        &mut others,
                    );
                    let terminal_id = term["result"]["terminalId"].clone();
                    peer.call(
                        "terminal/wait_for_exit",
                        json!({"sessionId": "s1", "terminalId": terminal_id}),
                        &mut others,
                    );
                    let ok = wrote.get("error").is_none() && escape.get("error").is_some();
                    update(
                        &mut peer,
                        json!({"sessionUpdate": "tool_call_update", "toolCallId": "c1", "status": if ok { "completed" } else { "failed" },
                        "content": [{"type": "content", "content": {"type": "text", "text": "edited"}}, {"type": "terminal", "terminalId": terminal_id}]}),
                    );
                    peer.call(
                        "terminal/release",
                        json!({"sessionId": "s1", "terminalId": terminal_id}),
                        &mut others,
                    );
                } else {
                    // Writes must be refused too.
                    let wrote = peer.call(
                        "fs/write_text_file",
                        json!({"sessionId": "s1", "path": path, "content": "NO"}),
                        &mut others,
                    );
                    let text = if wrote.get("error").is_some() {
                        "write refused"
                    } else {
                        "write went through"
                    };
                    update(
                        &mut peer,
                        json!({"sessionUpdate": "tool_call_update", "toolCallId": "c1", "status": "failed",
                        "content": [{"type": "content", "content": {"type": "text", "text": text}}]}),
                    );
                }
                update(
                    &mut peer,
                    json!({"sessionUpdate": "plan", "entries": [{"content": "Uppercase a.txt", "priority": "high", "status": "completed"}]}),
                );
                update(
                    &mut peer,
                    json!({"sessionUpdate": "usage_update", "used": 1234, "size": 200000}),
                );
                peer.reply(&id, json!({"stopReason": "end_turn"}));
            }
            _ if id.is_null() => {}
            _ => peer.send(
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "nope"}}),
            ),
        }
    }
    std::process::exit(0);
}

#[test]
fn acp_agent_in_the_thread_model_with_change_set() {
    let project = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("tempdir");
    let root = project.path().canonicalize().expect("canonical");
    std::fs::write(root.join("a.txt"), "hello\n").expect("write");
    let (command, args) = fake_process("fake_acp_agent");
    let mut env = BTreeMap::new();
    env.insert("FERRUM_FAKE_ACP".into(), "1".into());
    let mut config = SessionConfig::isolated(&root, home.path());
    config.acp_agents = vec![AcpAgentConfig {
        id: "fake".into(),
        name: "Fake".into(),
        command,
        args,
        env,
    }];
    config.agent = Some(AgentChoice::Acp {
        agent: "fake".into(),
    });
    let (session, rx) = ChatSession::new(config).expect("session");
    let mut events = Events::new(rx);

    session.send("shout a.txt", vec![]);
    let request = events.wait("the agent's permission request", |e| match e {
        SessionEvent::Permission(r) => Some(r.clone()),
        _ => None,
    });
    assert_eq!(request.tool, "acp:fake:edit");
    assert_eq!(request.tool_call_id, "c1");
    session.respond_permission(request.id, Decision::AllowOnce);
    events.idle();

    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        "HELLO\n"
    );
    assert!(!std::path::Path::new("/tmp/ferrum-acp-escape.txt").exists());
    let thread = session.thread();
    assert_eq!(thread.agent, "acp:fake");
    assert_eq!(thread.acp_session_id.as_deref(), Some("s1"));
    assert_eq!(thread.messages[1].text(), "Working");
    let call = thread.tool("c1").cloned().expect("call");
    assert_eq!(call.status, ToolStatus::Completed, "{call:#?}");
    assert_eq!(call.output.as_deref(), Some("edited\nhi"));
    assert!(
        thread.messages[1].blocks.iter().any(
            |b| matches!(b, Block::Plan { entries } if entries[0].content == "Uppercase a.txt")
        )
    );
    assert_eq!(thread.context_tokens, 1234);
    assert_eq!(thread.context_window, Some(200000));

    // The agent's write is in the change set: reviewable and restorable.
    let changes = session.changes().expect("changes");
    assert_eq!(changes.len(), 1);
    session
        .restore_checkpoint(thread.messages[0].id)
        .expect("restore");
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        "hello\n"
    );

    // Ask mode: the edit permission is refused without asking, and so is
    // the write.
    session.set_mode(Mode::Ask);
    session.send("shout again", vec![]);
    events.idle();
    let asked_again = events
        .seen
        .iter()
        .filter(|e| matches!(e, SessionEvent::Permission(_)))
        .count();
    assert_eq!(asked_again, 1, "only the first turn asked");
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        "hello\n"
    );
    let call = session.thread().tool("c1").cloned().expect("call");
    assert_eq!(call.output.as_deref(), Some("write refused"));
}
