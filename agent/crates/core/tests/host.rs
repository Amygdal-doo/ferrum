//! A chat built into an app, without a project: it opens at once, offers
//! the app's tools instead of the file tools, runs read-only tools
//! without asking, asks before an action, and tells the model what the
//! user is looking at.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use common::Events;
use ferrumagent_core::host::{AppHost, HostTool, Workspace};
use ferrumagent_core::modes::Mode;
use ferrumagent_core::permissions::Decision;
use ferrumagent_core::provider::ToolSpec;
use ferrumagent_core::provider::mock::MockProvider;
use ferrumagent_core::session::{ChatSession, SessionConfig, SessionEvent};
use ferrumagent_core::thread::ToolStatus;
use ferrumagent_core::tools::{Access, ToolDef, ToolOutput};
use serde_json::{Value, json};

/// Counts its calls and answers with its name.
struct Counting {
    name: &'static str,
    access: Access,
    calls: AtomicUsize,
    inputs: Mutex<Vec<Value>>,
}

impl Counting {
    fn new(name: &'static str, access: Access) -> Arc<Self> {
        Arc::new(Self {
            name,
            access,
            calls: AtomicUsize::new(0),
            inputs: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl HostTool for Counting {
    fn def(&self) -> ToolDef {
        ToolDef {
            spec: ToolSpec {
                name: self.name.into(),
                description: format!("{} for tests", self.name),
                input_schema: json!({"type": "object", "properties": {"id": {"type": "string"}}}),
            },
            access: self.access,
        }
    }

    fn title(&self, input: &Value) -> String {
        format!("{} {}", self.name, input["id"].as_str().unwrap_or_default())
    }

    async fn call(&self, input: Value) -> ToolOutput {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inputs.lock().expect("lock").push(input);
        ToolOutput::ok(format!("{} ran", self.name))
    }
}

#[test]
fn a_chat_without_a_project_uses_the_apps_tools_and_context() {
    let home = tempfile::tempdir().expect("tempdir");
    let list = Counting::new("containers_list", Access::Read);
    let restart = Counting::new("containers_restart", Access::Execute);
    let host = AppHost::new("FerrumContainers", "A container manager.")
        .tool(list.clone())
        .tool(restart.clone());
    let context = host.context.clone();
    let mock = MockProvider::new();
    mock.push_tool("t1", "containers_list", json!({}));
    mock.push_tool("t2", "read_file", json!({"path": "x"}));
    mock.push_tool("t3", "containers_restart", json!({"id": "web"}));
    mock.push_text("Restarted web.");
    let mut config = SessionConfig::isolated(home.path(), home.path()).without_project(host);
    config.provider_override = Some(Arc::new(mock.clone()));
    assert!(config.project.ends_with("apps/ferrumcontainers"));
    let (session, rx) = ChatSession::new(config).expect("session");
    let mut events = Events::new(rx);
    assert_eq!(session.workspace(), Workspace::Advice);

    context.set("Section: Containers\nweb (running)");
    session.send("Why is web slow? Restart it.", vec![]);
    let request = events.wait("the restart prompt", |e| match e {
        SessionEvent::Permission(r) => Some(r.clone()),
        _ => None,
    });
    // The read-only tool ran without asking; the file tool does not exist.
    assert_eq!(list.calls(), 1);
    assert_eq!(request.tool, "containers_restart");
    assert_eq!(request.title, "containers_restart web");
    assert_eq!(restart.calls(), 0, "nothing runs before the answer");
    session.respond_permission(request.id, Decision::AllowOnce);
    events.idle();
    assert_eq!(restart.calls(), 1);
    assert_eq!(restart.inputs.lock().expect("lock")[0]["id"], "web");

    let thread = session.thread();
    assert_eq!(
        thread.tool("t1").map(|c| c.status),
        Some(ToolStatus::Completed)
    );
    let t2 = thread.tool("t2").expect("t2");
    assert_eq!(t2.status, ToolStatus::Failed);
    assert!(
        t2.output
            .as_deref()
            .unwrap_or_default()
            .contains("Unknown tool")
    );
    assert_eq!(
        thread.tool("t3").map(|c| c.title.as_str()),
        Some("containers_restart web")
    );

    let requests = mock.requests();
    let first = &requests[0];
    assert!(first.system.contains("built into FerrumContainers"));
    assert!(first.system.contains("no code project"));
    assert!(first.system.contains("web (running)"));
    let tools: Vec<&str> = first.tools.iter().map(|t| t.name.as_str()).collect();
    assert!(tools.contains(&"containers_list") && tools.contains(&"containers_restart"));
    assert!(!tools.contains(&"read_file") && !tools.contains(&"run_command"));
}

#[test]
fn ask_mode_offers_only_the_read_only_tools_and_denied_actions_do_not_run() {
    let home = tempfile::tempdir().expect("tempdir");
    let list = Counting::new("data_schema", Access::Read);
    let write = Counting::new("data_write_query", Access::Execute);
    let host = AppHost::new("FerrumData", "A database IDE.")
        .tool(list.clone())
        .tool(write.clone());
    let mock = MockProvider::new();
    let mut config = SessionConfig::isolated(home.path(), home.path()).without_project(host);
    config.provider_override = Some(Arc::new(mock.clone()));
    let (session, rx) = ChatSession::new(config).expect("session");
    let mut events = Events::new(rx);

    session.set_mode(Mode::Ask);
    mock.push_text("Here is the schema.");
    session.send("Describe the schema", vec![]);
    events.idle();
    let tools: Vec<String> = mock.requests()[0]
        .tools
        .iter()
        .map(|t| t.name.clone())
        .collect();
    assert!(tools.contains(&"data_schema".to_string()));
    assert!(!tools.contains(&"data_write_query".to_string()));

    session.set_mode(Mode::Agent);
    mock.push_tool("w1", "data_write_query", json!({"id": "1"}));
    mock.push_text("Not run.");
    session.send("Delete the old rows", vec![]);
    let request = events.wait("the write prompt", |e| match e {
        SessionEvent::Permission(r) => Some(r.clone()),
        _ => None,
    });
    session.respond_permission(request.id, Decision::Deny);
    events.idle();
    assert_eq!(write.calls(), 0);
    assert_eq!(
        session.thread().tool("w1").map(|c| c.status),
        Some(ToolStatus::Denied)
    );
}
