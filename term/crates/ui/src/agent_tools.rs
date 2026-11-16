//! The agent's terminal tools: read the focused pane's screen (at once),
//! and type a command into it and press Enter — an action, so the chat
//! asks the user first. The terminal lives on the UI thread, so the tools
//! hand their calls to the view over a channel (see `agent_pane`) and wait
//! for its answer.

use std::sync::Arc;
use std::time::Duration;

use ferrumagent_ui::core::host::HostTool;
use ferrumagent_ui::core::provider::ToolSpec;
use ferrumagent_ui::core::tools::{Access, ToolDef, ToolOutput};
use futures::channel::{mpsc, oneshot};
use serde_json::{Value, json};

/// How long a tool waits for the view.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);

/// What a tool asks the view for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TermCall {
    /// The focused pane's screen.
    Screen,
    /// Type this line into the focused pane, press Enter, then the screen.
    Run(String),
}

/// A call and where its answer goes.
pub(crate) type Request = (TermCall, oneshot::Sender<Result<String, String>>);

/// The tools' side of the channel to the view.
#[derive(Clone)]
pub(crate) struct Bridge(mpsc::UnboundedSender<Request>);

impl Bridge {
    pub(crate) fn new() -> (Self, mpsc::UnboundedReceiver<Request>) {
        let (tx, rx) = mpsc::unbounded();
        (Self(tx), rx)
    }

    async fn ask(&self, call: TermCall) -> Result<String, String> {
        let (tx, rx) = oneshot::channel();
        self.0
            .unbounded_send((call, tx))
            .map_err(|_| "The terminal window is closed.".to_string())?;
        match tokio::time::timeout(ANSWER_TIMEOUT, rx).await {
            Ok(Ok(answer)) => answer,
            Ok(Err(_)) => Err("The terminal window is closed.".into()),
            Err(_) => Err("The terminal did not answer.".into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Screen,
    Run,
}

pub(crate) struct TermTool {
    kind: Kind,
    bridge: Bridge,
}

/// The tools, answered by the view behind `bridge`.
pub(crate) fn tools(bridge: &Bridge) -> Vec<Arc<dyn HostTool>> {
    [Kind::Screen, Kind::Run]
        .into_iter()
        .map(|kind| {
            Arc::new(TermTool {
                kind,
                bridge: bridge.clone(),
            }) as Arc<dyn HostTool>
        })
        .collect()
}

#[async_trait::async_trait]
impl HostTool for TermTool {
    fn def(&self) -> ToolDef {
        let (name, description, schema, access) = match self.kind {
            Kind::Screen => (
                "terminal_screen",
                "What the focused terminal pane shows now (its visible screen).",
                json!({"type": "object", "properties": {}}),
                Access::Read,
            ),
            Kind::Run => (
                "terminal_run",
                "Type a command line into the focused terminal pane and press Enter, as the user would; returns the screen a moment later. Use it for short commands at a shell prompt. The user approves it first.",
                json!({"type": "object", "properties": {
                    "command": {"type": "string", "description": "One line, without the trailing newline"}
                }, "required": ["command"]}),
                Access::Execute,
            ),
        };
        ToolDef {
            spec: ToolSpec {
                name: name.into(),
                description: description.into(),
                input_schema: schema,
            },
            access,
        }
    }

    fn title(&self, input: &Value) -> String {
        match self.kind {
            Kind::Screen => "Read the terminal".into(),
            Kind::Run => format!(
                "Run `{}` in the terminal",
                input["command"].as_str().unwrap_or_default().trim()
            ),
        }
    }

    async fn call(&self, input: Value) -> ToolOutput {
        let call = match self.kind {
            Kind::Screen => TermCall::Screen,
            Kind::Run => {
                let line = input["command"].as_str().unwrap_or_default().trim();
                if line.is_empty() || line.contains(['\n', '\r']) {
                    return ToolOutput::error("Give one command line.");
                }
                TermCall::Run(line.to_string())
            }
        };
        self.bridge
            .ask(call)
            .await
            .map_or_else(ToolOutput::error, ToolOutput::ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt as _;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("runtime")
    }

    #[test]
    fn the_screen_is_read_at_once_and_running_asks() {
        let (bridge, _rx) = Bridge::new();
        let tools = tools(&bridge);
        assert_eq!(tools[0].def().access, Access::Read);
        assert_eq!(tools[1].def().access, Access::Execute);
        assert_eq!(
            tools[1].title(&json!({"command": "cargo test"})),
            "Run `cargo test` in the terminal"
        );
    }

    #[test]
    fn calls_reach_the_view_and_its_answer_comes_back() {
        let (bridge, mut rx) = Bridge::new();
        let tools = tools(&bridge);
        let rt = runtime();
        let view = std::thread::spawn(move || {
            futures::executor::block_on(async {
                let mut seen = Vec::new();
                while let Some((call, reply)) = rx.next().await {
                    seen.push(call.clone());
                    let _ = reply.send(Ok(format!("{call:?} done")));
                }
                seen
            })
        });
        let out = rt.block_on(tools[1].call(json!({"command": "ls -la"})));
        assert_eq!(out.text, "Run(\"ls -la\") done");
        let out = rt.block_on(tools[1].call(json!({"command": "a\nb"})));
        assert!(out.is_error, "one line only");
        let out = rt.block_on(tools[0].call(json!({})));
        assert_eq!(out.text, "Screen done");
        drop(tools);
        drop(bridge);
        assert_eq!(
            view.join().expect("view"),
            [TermCall::Run("ls -la".into()), TermCall::Screen]
        );
    }

    #[test]
    fn a_closed_window_is_an_error() {
        let (bridge, rx) = Bridge::new();
        drop(rx);
        let out = runtime().block_on(tools(&bridge)[0].call(json!({})));
        assert!(out.is_error && out.text.contains("closed"));
    }
}
