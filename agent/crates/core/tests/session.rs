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

#[test]
fn sibling_threads_run_alongside_and_share_the_change_set() {
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
    mock.push_text("B answer");
    mock.push_text("A done");
    let (a, mut a_events) = session(root, home.path(), &mock);

    // A waits on a prompt; the prompt can be re-read by a UI that starts
    // showing A later.
    a.send("Edit a", vec![]);
    let request = a_events.wait("A's prompt", |e| match e {
        SessionEvent::Permission(r) => Some(r.clone()),
        _ => None,
    });
    assert_eq!(a.open_permissions(), std::slice::from_ref(&request));

    // B, a new thread beside it, runs a whole turn meanwhile.
    let (b, b_events) = a.sibling(None, Mode::Ask).expect("sibling");
    let mut b_events = Events::new(b_events);
    assert!(!b.same(&a) && a.same(&a.clone()));
    assert_ne!(b.thread().id, a.thread().id);
    assert_eq!(b.mode(), Mode::Ask);
    b.send("Hello B", vec![]);
    b_events.idle();
    assert_eq!(b.thread().messages[1].text(), "B answer");
    assert!(a.is_busy(), "A is still waiting on its prompt");

    // A goes on; its edit is in the change set B sees too.
    a.respond_permission(request.id, Decision::AllowOnce);
    a_events.idle();
    assert!(a.open_permissions().is_empty());
    assert_eq!(
        a.thread().messages.last().map(|m| m.text()),
        Some("A done".into())
    );
    assert_eq!(b.changes().expect("changes").len(), 1);

    // Both are saved; a sibling can open a saved thread.
    assert_eq!(a.threads().len(), 2);
    let (reopened, _rx) = a.sibling(Some(&b.thread().id), Mode::Agent).expect("open");
    assert_eq!(reopened.thread().messages.len(), 2);
    assert_eq!(reopened.mode(), Mode::Ask, "the thread's own mode");
    assert!(a.sibling(Some("missing"), Mode::Agent).is_err());
}

#[test]
fn a_turn_whose_view_is_gone_is_denied_rather_than_left_waiting() {
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
    mock.push_text("never");
    let (session, events) = session(root, home.path(), &mock);
    drop(events);
    session.send("Edit a", vec![]);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while session.is_busy() || session.thread().messages.len() < 2 {
        assert!(std::time::Instant::now() < deadline, "the turn never ended");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(tool_status(&session, "t1"), Some(ToolStatus::Denied));
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        "one\n"
    );
    // The thread was saved even though nobody watched it.
    assert_eq!(session.threads().len(), 1);
}

#[test]
fn deleting_a_running_sibling_thread_is_final() {
    let project = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("tempdir");
    let mut mock = MockProvider::new();
    mock.delay = Some(std::time::Duration::from_millis(200));
    mock.push_text("slow answer");
    let (a, _a_events) = session(project.path(), home.path(), &mock);
    let (b, b_events) = a.sibling(None, Mode::Agent).expect("sibling");
    let mut b_events = Events::new(b_events);
    b.send("hello", vec![]);
    b_events.wait("B's thread to be saved", |e| {
        matches!(e, SessionEvent::ThreadsChanged).then_some(())
    });
    let id = b.thread().id;
    assert!(a.threads().iter().any(|t| t.id == id));

    // What the chat view does when a background thread is deleted.
    b.discard();
    a.delete_thread(&id).expect("delete");
    b_events.idle();
    assert!(
        a.threads().iter().all(|t| t.id != id),
        "the cancelled turn wrote the deleted thread back"
    );
}

#[test]
fn siblings_share_one_settings() {
    let project = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("tempdir");
    let mock = MockProvider::new();
    let (a, _a_events) = session(project.path(), home.path(), &mock);
    let (b, _b_events) = a.sibling(None, Mode::Agent).expect("sibling");
    let mut settings = a.settings();
    settings.default_agent = Some("acp:shared-test".into());
    a.update_settings(settings).expect("settings");
    assert_eq!(
        b.settings().default_agent.as_deref(),
        Some("acp:shared-test")
    );
    b.set_approval(Approval::AutoEdits).expect("approval");
    assert_eq!(a.settings().approval, Approval::AutoEdits);
}
