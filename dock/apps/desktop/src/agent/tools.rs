//! The agent's container tools: list containers and images, read a
//! container's logs (both run at once), and start, stop or restart a
//! container — an action, so the chat asks the user first. They use the
//! engine the window is connected to, on the services runtime.

use std::sync::{Arc, Mutex, PoisonError};

use ferrumagent_ui::core::host::HostTool;
use ferrumagent_ui::core::provider::ToolSpec;
use ferrumagent_ui::core::tools::{Access, ToolDef, ToolOutput};
use ferrumdock_engine::{
    ContainerAction, LogOptions, SharedEngine, ansi,
    format::{bytes, ports, short_id},
};
use futures::StreamExt as _;
use serde_json::{Value, json};

use crate::services::Services;

/// Log lines returned at most.
const MAX_LOG_LINES: u32 = 1000;
/// Log lines returned when the model does not say.
const LOG_LINES: u32 = 200;

/// The connected engine, set by the window before each message.
#[derive(Clone, Default)]
pub struct EngineSlot(Arc<Mutex<Option<SharedEngine>>>);

impl EngineSlot {
    pub fn set(&self, engine: Option<SharedEngine>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = engine;
    }

    fn get(&self) -> Result<SharedEngine, String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| "No container engine is connected.".to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    List,
    Logs,
    Action,
}

/// One of the tools.
pub struct DockTool {
    kind: Kind,
    engine: EngineSlot,
    services: Services,
}

/// The tools, all on `engine`.
pub fn tools(engine: &EngineSlot, services: &Services) -> Vec<Arc<dyn HostTool>> {
    [Kind::List, Kind::Logs, Kind::Action]
        .into_iter()
        .map(|kind| {
            Arc::new(DockTool {
                kind,
                engine: engine.clone(),
                services: services.clone(),
            }) as Arc<dyn HostTool>
        })
        .collect()
}

fn spec(name: &str, description: &str, schema: Value) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        description: description.into(),
        input_schema: schema,
    }
}

#[async_trait::async_trait]
impl HostTool for DockTool {
    fn def(&self) -> ToolDef {
        let container = json!({"type": "string", "description": "Container name or id"});
        let (spec, access) = match self.kind {
            Kind::List => (
                spec(
                    "containers_list",
                    "List every container (state, status, image, ports, compose project) and image of the connected engine.",
                    json!({"type": "object", "properties": {}}),
                ),
                Access::Read,
            ),
            Kind::Logs => (
                spec(
                    "containers_logs",
                    "The last lines of a container's logs (stdout and stderr).",
                    json!({"type": "object", "properties": {
                        "container": container,
                        "lines": {"type": "integer", "minimum": 1, "maximum": MAX_LOG_LINES}
                    }, "required": ["container"]}),
                ),
                Access::Read,
            ),
            Kind::Action => (
                spec(
                    "containers_action",
                    "Start, stop or restart a container. The user approves it first.",
                    json!({"type": "object", "properties": {
                        "container": container,
                        "action": {"type": "string", "enum": ["start", "stop", "restart"]}
                    }, "required": ["container", "action"]}),
                ),
                Access::Execute,
            ),
        };
        ToolDef { spec, access }
    }

    fn title(&self, input: &Value) -> String {
        let container = input["container"].as_str().unwrap_or_default();
        match self.kind {
            Kind::List => "List containers".into(),
            Kind::Logs => format!("Logs of {container}"),
            Kind::Action => match action(input) {
                Ok(action) => format!("{} {container}", capitalized(action.verb())),
                Err(_) => format!("Change {container}"),
            },
        }
    }

    async fn call(&self, input: Value) -> ToolOutput {
        let engine = match self.engine.get() {
            Ok(engine) => engine,
            Err(e) => return ToolOutput::error(e),
        };
        let kind = self.kind;
        let work = async move {
            match kind {
                Kind::List => list(&engine).await,
                Kind::Logs => logs(&engine, &input).await,
                Kind::Action => act(&engine, &input).await,
            }
        };
        match self.services.spawn(work).await {
            Ok(Ok(text)) => ToolOutput::ok(text),
            Ok(Err(e)) => ToolOutput::error(e),
            Err(e) => ToolOutput::error(format!("background task failed: {e}")),
        }
    }
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

fn action(input: &Value) -> Result<ContainerAction, String> {
    match input["action"].as_str() {
        Some("start") => Ok(ContainerAction::Start),
        Some("stop") => Ok(ContainerAction::Stop),
        Some("restart") => Ok(ContainerAction::Restart),
        other => Err(format!(
            "Unknown action {other:?}: use start, stop or restart."
        )),
    }
}

/// The container `name_or_id` names: its id and name.
async fn find(engine: &SharedEngine, name_or_id: &str) -> Result<(String, String), String> {
    let wanted = name_or_id.trim().trim_start_matches('/');
    if wanted.is_empty() {
        return Err("Say which container.".into());
    }
    let containers = engine.containers().await.map_err(|e| e.to_string())?;
    containers
        .iter()
        .find(|c| c.name == wanted)
        .or_else(|| containers.iter().find(|c| c.id.starts_with(wanted)))
        .map(|c| (c.id.clone(), c.name.clone()))
        .ok_or_else(|| format!("No container is called {wanted}."))
}

async fn list(engine: &SharedEngine) -> Result<String, String> {
    let containers = engine.containers().await.map_err(|e| e.to_string())?;
    let images = engine.images().await.map_err(|e| e.to_string())?;
    let mut out = format!("Containers ({}):\n", containers.len());
    for c in &containers {
        out.push_str(&format!(
            "- {} [{}] {} — {}; image {}",
            c.name,
            short_id(&c.id),
            c.state.label(),
            c.status,
            c.image
        ));
        if !c.ports.is_empty() {
            out.push_str(&format!("; ports {}", ports(&c.ports)));
        }
        if let Some(project) = c.project() {
            out.push_str(&format!(
                "; compose {project}/{}",
                c.service().unwrap_or("?")
            ));
        }
        out.push('\n');
    }
    out.push_str(&format!("\nImages ({}):\n", images.len()));
    for i in &images {
        let tags = if i.tags.is_empty() {
            "<none>".to_string()
        } else {
            i.tags.join(", ")
        };
        out.push_str(&format!(
            "- {tags} [{}] {}\n",
            short_id(&i.id),
            bytes(i.size)
        ));
    }
    Ok(out)
}

async fn logs(engine: &SharedEngine, input: &Value) -> Result<String, String> {
    let (id, name) = find(engine, input["container"].as_str().unwrap_or_default()).await?;
    let lines = input["lines"]
        .as_u64()
        .map_or(LOG_LINES, |n| u32::try_from(n).unwrap_or(MAX_LOG_LINES))
        .clamp(1, MAX_LOG_LINES);
    let options = LogOptions {
        follow: false,
        tail: Some(lines),
        timestamps: false,
        since: None,
    };
    let mut stream = engine.logs(&id, options);
    let mut text = String::new();
    while let Some(chunk) = stream.next().await {
        text.push_str(&chunk.map_err(|e| e.to_string())?.text);
    }
    let text: Vec<String> = text.lines().map(ansi::strip).collect();
    let text = text.join("\n");
    if text.trim().is_empty() {
        return Ok(format!("{name} has no log output."));
    }
    Ok(format!("Last {lines} lines of {name}:\n{text}"))
}

async fn act(engine: &SharedEngine, input: &Value) -> Result<String, String> {
    let action = action(input)?;
    let (id, name) = find(engine, input["container"].as_str().unwrap_or_default()).await?;
    engine
        .container_action(&id, action)
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!("{} {name}.", action.past_tense()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdock_engine::fake::FakeEngine;
    use ferrumdock_engine::{LogChunk, LogStream};

    fn setup() -> (Arc<FakeEngine>, Vec<Arc<dyn HostTool>>) {
        let fake = Arc::new(FakeEngine::with_sample_data());
        fake.state().container_logs.insert(
            "aaa111".into(),
            vec![LogChunk {
                stream: LogStream::Stdout,
                text: "\u{1b}[32mlistening\u{1b}[0m on :8080\n".into(),
            }],
        );
        let slot = EngineSlot::default();
        slot.set(Some(fake.clone()));
        let services = Services::start().expect("services");
        (fake, tools(&slot, &services))
    }

    fn run(tool: &Arc<dyn HostTool>, input: Value) -> ToolOutput {
        futures::executor::block_on(tool.call(input))
    }

    fn named<'a>(tools: &'a [Arc<dyn HostTool>], name: &str) -> &'a Arc<dyn HostTool> {
        tools
            .iter()
            .find(|t| t.def().spec.name == name)
            .expect("tool")
    }

    #[test]
    fn listing_and_logs_are_read_only_and_actions_ask() {
        let (_, tools) = setup();
        let access = |name| named(&tools, name).def().access;
        assert_eq!(access("containers_list"), Access::Read);
        assert_eq!(access("containers_logs"), Access::Read);
        assert_eq!(access("containers_action"), Access::Execute);
    }

    #[test]
    fn lists_containers_and_images() {
        let (_, tools) = setup();
        let out = run(named(&tools, "containers_list"), json!({}));
        assert!(!out.is_error, "{}", out.text);
        assert!(out.text.contains("Containers (3)"));
        assert!(out.text.contains("web-app-1 [aaa111] running"));
        assert!(out.text.contains("compose web/app"));
        assert!(out.text.contains("web-app:latest"));
    }

    #[test]
    fn reads_logs_by_name_without_escape_codes() {
        let (fake, tools) = setup();
        let out = run(
            named(&tools, "containers_logs"),
            json!({"container": "web-app-1", "lines": 50}),
        );
        assert_eq!(out.text, "Last 50 lines of web-app-1:\nlistening on :8080");
        let (id, options) = fake.state().log_requests[0].clone();
        assert_eq!(id, "aaa111");
        assert!(!options.follow);
        assert_eq!(options.tail, Some(50));
        let missing = run(
            named(&tools, "containers_logs"),
            json!({"container": "nope"}),
        );
        assert!(missing.is_error);
    }

    #[test]
    fn restarts_a_container_and_refuses_other_actions() {
        let (fake, tools) = setup();
        let tool = named(&tools, "containers_action");
        let input = json!({"container": "web-db-1", "action": "restart"});
        assert_eq!(tool.title(&input), "Restart web-db-1");
        let out = run(tool, input);
        assert_eq!(out.text, "Restarted web-db-1.");
        assert_eq!(fake.state().calls, ["restart bbb222"]);

        let out = run(tool, json!({"container": "web-db-1", "action": "remove"}));
        assert!(out.is_error);
        assert_eq!(fake.state().calls.len(), 1, "nothing else ran");
    }

    #[test]
    fn without_an_engine_the_tools_say_so() {
        let tools = tools(
            &EngineSlot::default(),
            &Services::start().expect("services"),
        );
        let out = run(named(&tools, "containers_list"), json!({}));
        assert!(out.is_error);
        assert!(out.text.contains("No container engine"));
    }
}
