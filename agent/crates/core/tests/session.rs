//! The built-in agent end to end through `ChatSession` with the scripted
//! mock provider: tool calls with permission prompts, edits in the change
//! set, checkpoints, modes, denial, sandboxing, queueing, persistence.

mod common;

use std::path::Path;
use std::sync::Arc;

use common::Events;
use ferrumagent_core::modes::Mode;
use ferrumagent_core::permissions::{Approval, Decision};
use ferrumagent_core::provider::mock::MockProvider;
use ferrumagent_core::provider::{Part, Role};
use ferrumagent_core::session::{ChatSession, SessionConfig, SessionEvent};
use ferrumagent_core::thread::{Block, ToolStatus};
use serde_json::json;

fn session(project: &Path, home: &Path, mock: &MockProvider) -> (ChatSession, Events) {
    let mut config = SessionConfig::isolated(project, home);
    config.provider_override = Some(Arc::new(mock.clone()));
    let (session, rx) = ChatSession::new(config).expect("session");
    (session, Events::new(rx))
}

fn tool_status(session: &ChatSession, id: &str) -> Option<ToolStatus> {
    session.thread().tool(id).map(|c| c.status)
}

#[test]
fn agent_reads_edits_with_permission_and_checkpoints_restore() {
    let project = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("tempdir");
    let root = project.path();
    std::fs::write(root.join("a.txt"), "one\ntwo\n").expect("write");
    std::fs::write(root.join("AGENTS.md"), "Use tabs.").expect("write");
    let mock = MockProvider::new();
    mock.push_tool("t1", "read_file", json!({"path": "a.txt"}));
    mock.push_tool(
        "t2",
        "edit_file",
        json!({"path": "a.txt", "old_string": "one", "new_string": "ONE"}),
    );
    mock.push_text("Done.");
    let (session, mut events) = session(root, home.path(), &mock);

    session.send("Fix @a.txt please", vec![]);
    let request = events.wait("the edit permission prompt", |e| match e {
        SessionEvent::Permission(r) => Some(r.clone()),
        _ => None,
    });
    assert_eq!(request.tool, "edit_file");
    assert_eq!(
        tool_status(&session, "t2"),
        Some(ToolStatus::AwaitingPermission)
    );
    session.respond_permission(request.id, Decision::AllowSession);
    events.idle();

    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        "ONE\ntwo\n"
    );
    let thread = session.thread();
    assert_eq!(thread.title, "Fix @a.txt please");
    assert_eq!(thread.messages.len(), 4, "{:#?}", thread.messages);
    assert_eq!(thread.messages[0].attachments[0].label, "a.txt");
    assert_eq!(tool_status(&session, "t1"), Some(ToolStatus::Completed));
    assert_eq!(tool_status(&session, "t2"), Some(ToolStatus::Completed));
    assert_eq!(thread.messages[3].text(), "Done.");
    assert_eq!(thread.usage.input_tokens, 30);

    // What the model saw: rules and tools in the first request, the read
    // result in the second.
    let requests = mock.requests();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].system.contains("Use tabs."));
    assert!(requests[0].tools.iter().any(|t| t.name == "edit_file"));
    let results: Vec<&Part> = requests[1]
        .messages
        .iter()
        .filter(|m| m.role == Role::User)
        .flat_map(|m| &m.parts)
        .filter(|p| matches!(p, Part::ToolResult { .. }))
        .collect();
    assert!(matches!(results[0], Part::ToolResult { content, .. } if content.contains("1\tone")));

    // The edit is reviewable.
    let changes = session.changes().expect("changes");
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].hunks.len(), 1);

    // Restore to before the message, then undo the restore.
    let files = session
        .restore_checkpoint(thread.messages[0].id)
        .expect("restore");
    assert_eq!(files, [Path::new("a.txt").to_path_buf()]);
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        "one\ntwo\n"
    );
    assert!(session.can_undo_restore());
    session.undo_restore().expect("undo");
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        "ONE\ntwo\n"
    );

    // Persisted, and reopened from the list.
    let threads = session.threads();
    assert_eq!(threads.len(), 1);
    session.new_thread();
    assert!(session.thread().messages.is_empty());
    session.open_thread(&threads[0].id).expect("open");
    assert_eq!(session.thread().messages.len(), 4);
}

#[test]
fn ask_mode_denials_sandbox_and_queue() {
    let project = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("tempdir");
    let root = project.path().join("p");
    std::fs::create_dir_all(&root).expect("mkdir");
    let mock = MockProvider::new();
    let (session, mut events) = session(&root, home.path(), &mock);

    // Ask mode: write tools are not offered, and refused if called.
    session.set_mode(Mode::Ask);
    mock.push_tool("w1", "write_file", json!({"path": "x.txt", "content": "x"}));
    mock.push_text("Cannot.");
    session.send("write x", vec![]);
    events.idle();
    assert!(!root.join("x.txt").exists());
    let thread = session.thread();
    let call = thread.tool("w1").expect("call");
    assert_eq!(call.status, ToolStatus::Failed);
    assert!(
        call.output
            .as_deref()
            .unwrap_or_default()
            .contains("Ask mode")
    );
    assert!(
        mock.requests()[0]
            .tools
            .iter()
            .all(|t| t.name != "write_file")
    );
    assert_eq!(thread.mode, Mode::Ask);

    // Agent mode: denying a command ends the turn.
    session.set_mode(Mode::Agent);
    mock.push_tool("c1", "run_command", json!({"command": "echo hi"}));
    session.send("run it", vec![]);
    let request = events.wait("the command prompt", |e| match e {
        SessionEvent::Permission(r) => Some(r.clone()),
        _ => None,
    });
    session.respond_permission(request.id, Decision::Deny);
    events.idle();
    assert_eq!(tool_status(&session, "c1"), Some(ToolStatus::Denied));
    assert_eq!(mock.remaining(), 0);

    // Paths outside the project are refused even when writes are allowed.
    mock.push_tool(
        "e1",
        "write_file",
        json!({"path": "../outside.txt", "content": "pwned"}),
    );
    mock.push_text("ok");
    session.send("escape", vec![]);
    let request = events.wait("the write prompt", |e| match e {
        SessionEvent::Permission(r) => Some(r.clone()),
        _ => None,
    });
    session.respond_permission(request.id, Decision::AllowOnce);
    events.idle();
    assert!(!project.path().join("outside.txt").exists());
    let call = session.thread().tool("e1").cloned().expect("call");
    assert_eq!(call.status, ToolStatus::Failed);
    assert!(
        call.output
            .unwrap_or_default()
            .contains("outside the project")
    );

    // Messages sent while busy are queued and run in order.
    let mut slow = MockProvider::new();
    slow.delay = Some(std::time::Duration::from_millis(30));
    slow.push_text("first");
    slow.push_text("second");
    let (session, mut events) = {
        let mut config = SessionConfig::isolated(&root, home.path());
        config.provider_override = Some(Arc::new(slow.clone()));
        let (s, rx) = ChatSession::new(config).expect("session");
        (s, Events::new(rx))
    };
    session.send("one", vec![]);
    session.send("two", vec![]);
    let queue = events.wait("the queue", |e| match e {
        SessionEvent::Queue(q) if !q.is_empty() => Some(q.clone()),
        _ => None,
    });
    assert_eq!(queue, ["two"]);
    events.idle();
    let texts: Vec<String> = session.thread().messages.iter().map(|m| m.text()).collect();
    assert_eq!(texts, ["one", "first", "two", "second"]);
}

#[test]
fn cancel_stops_a_streaming_turn() {
    let project = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("tempdir");
    let mut mock = MockProvider::new();
    mock.delay = Some(std::time::Duration::from_millis(200));
    mock.push_text("slow answer");
    let (session, mut events) = session(project.path(), home.path(), &mock);
    session.send("hello", vec![]);
    events.wait("busy", |e| {
        matches!(e, SessionEvent::Busy(true)).then_some(())
    });
    session.cancel();
    events.idle();
    let thread = session.thread();
    assert_eq!(
        thread.last_stop,
        Some(ferrumagent_core::provider::StopReason::Cancelled)
    );
    assert!(thread.messages.iter().all(|m| {
        m.blocks
            .iter()
            .all(|b| !matches!(b, Block::Text { text } if text.contains("slow answer")))
    }));
}

#[test]
fn approval_levels_rename_and_thread_list_events() {
    let project = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("tempdir");
    let root = project.path();
    std::fs::write(root.join("a.txt"), "one\n").expect("write");
    let mock = MockProvider::new();
    mock.push_tool(
        "t1",
        "edit_file",
        json!({"path": "a.txt", "old_string": "one", "new_string": "ONE"}),
    );
    mock.push_tool("t2", "run_command", json!({"command": "true"}));
    mock.push_text("Done.");
    let (session, mut events) = session(root, home.path(), &mock);

    // Auto-accept edits: the edit runs without a prompt, the command asks.
    session
        .set_approval(Approval::AutoEdits)
        .expect("set approval");
    session.send("Edit and run", vec![]);
    events.wait("the thread to be listed", |e| {
        matches!(e, SessionEvent::ThreadsChanged).then_some(())
    });
    let request = events.wait("the command prompt", |e| match e {
        SessionEvent::Permission(r) => Some(r.clone()),
        _ => None,
    });
    assert_eq!(request.tool, "run_command");
    assert_eq!(tool_status(&session, "t1"), Some(ToolStatus::Completed));
    session.respond_permission(request.id, Decision::Deny);
    events.idle();
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        "ONE\n"
    );

    // The level is saved with the settings and survives a settings save.
    let reopened = ferrumagent_core::settings::AgentSettings::load(
        &ferrumagent_core::paths::settings_file(home.path()),
    );
    assert_eq!(reopened.approval, Approval::AutoEdits);
    session
        .update_settings(session.settings())
        .expect("settings");
    assert_eq!(session.approval(), Approval::AutoEdits);

    // Renaming the current thread, then a saved one.
    let id = session.thread().id;
    session.rename_thread(&id, "Loud a").expect("rename");
    assert_eq!(session.thread().title, "Loud a");
    assert_eq!(session.threads()[0].title, "Loud a");
    assert_eq!(session.threads()[0].mode, Mode::Agent);
    assert!(session.threads()[0].tokens > 0);
    session.new_thread_in(Mode::Plan);
    assert_eq!(session.mode(), Mode::Plan);
    assert_eq!(session.thread().mode, Mode::Plan);
    session.rename_thread(&id, "Older").expect("rename saved");
    assert_eq!(session.threads()[0].title, "Older");
    assert!(session.rename_thread(&id, "  ").is_err());
    session.delete_thread(&id).expect("delete");
    assert!(session.threads().is_empty());
    events.wait("the list change", |e| {
        matches!(e, SessionEvent::ThreadsChanged).then_some(())
    });
}
