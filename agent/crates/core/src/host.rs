//! The app a chat is built into, when it is not a code editor (FerrumTerm,
//! FerrumData, FerrumContainers, FerrumSystem): its name, what the user is
//! looking at — refreshed by the host before each message — and tools of
//! its own. Host tools go through the same modes and permission prompts as
//! the built-ins: [`Access::Read`] ones run at once, anything else asks.
//!
//! Such a chat may have no project at all ([`Workspace::Advice`]): no file
//! tools, no snapshot, no review; just advice and the host's tools.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::Value;

use crate::tools::{Access, ToolDef, ToolOutput};

/// What the chat works on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Workspace {
    /// A code project: file tools, snapshots, review and checkpoints.
    #[default]
    Project,
    /// No project: advice and the host's tools only. The session's
    /// `project` is then a private, empty folder that keys its threads
    /// (see [`app_folder`]).
    Advice,
}

/// A tool an app offers its chat (`containers_restart`, `data_schema`…).
/// Name tools after the app: "always allow" answers are kept per name for
/// every app.
#[async_trait::async_trait]
pub trait HostTool: Send + Sync {
    /// Name, description, input schema and what it does: `Read` runs
    /// without asking; `Execute` (an action) asks unless allowed before.
    fn def(&self) -> ToolDef;

    /// The tool card's title for a call, e.g. "Restart web".
    fn title(&self, _input: &Value) -> String {
        self.def().spec.name
    }

    /// Runs on the chat's tokio runtime.
    async fn call(&self, input: Value) -> ToolOutput;
}

/// What the user is looking at in the app: set by the host before a
/// message is sent, read when the turn builds its prompt.
#[derive(Debug, Clone, Default)]
pub struct HostContext(Arc<Mutex<String>>);

impl HostContext {
    pub fn set(&self, text: impl Into<String>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = text.into();
    }

    pub fn get(&self) -> String {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// The host app as the session sees it.
#[derive(Clone, Default)]
pub struct AppHost {
    /// "FerrumData".
    pub app: String,
    /// One or two sentences on what the app does, for the system prompt.
    pub about: String,
    pub context: HostContext,
    pub tools: Vec<Arc<dyn HostTool>>,
}

impl AppHost {
    pub fn new(app: impl Into<String>, about: impl Into<String>) -> Self {
        Self {
            app: app.into(),
            about: about.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn tool(mut self, tool: Arc<dyn HostTool>) -> Self {
        self.tools.push(tool);
        self
    }
}

/// `<home>/apps/<app>`: the folder a chat without a project keeps its
/// threads under (created; it stays empty).
pub fn app_folder(home: &Path, app: &str) -> PathBuf {
    let slug: String = app
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = if slug.trim_matches('-').is_empty() {
        "chat".to_string()
    } else {
        slug
    };
    let folder = home.join("apps").join(slug);
    if let Err(e) = std::fs::create_dir_all(&folder) {
        tracing::warn!("creating {}: {e}", folder.display());
    }
    folder
}

/// The system prompt's part about the host app: who we are in it, what it
/// does, the project (or that there is none) and what the user sees.
pub(crate) fn prompt_section(host: &AppHost, workspace: Workspace) -> String {
    let mut text = format!("You are built into {}. {}\n", host.app, host.about.trim());
    if workspace == Workspace::Advice {
        text.push_str(
            "There is no code project here: you cannot read or edit files or run shell \
             commands. Help with advice and explanations, and use the app's own tools when \
             they help.\n",
        );
    }
    if !host.tools.is_empty() {
        let (read, act): (Vec<_>, Vec<_>) = host
            .tools
            .iter()
            .map(|t| t.def())
            .partition(|d| d.access == Access::Read);
        let names = |defs: &[ToolDef]| {
            defs.iter()
                .map(|d| d.spec.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        if !read.is_empty() {
            text.push_str(&format!(
                "{}'s read-only tools ({}) run at once.\n",
                host.app,
                names(&read)
            ));
        }
        if !act.is_empty() {
            text.push_str(&format!(
                "Its actions ({}) change things: the user approves each one first, so say \
                 what you are about to do and why.\n",
                names(&act)
            ));
        }
    }
    let context = host.context.get();
    if !context.trim().is_empty() {
        text.push_str(&format!(
            "\n# What the user is looking at in {}\n{}\n",
            host.app,
            context.trim()
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ToolSpec;
    use serde_json::json;

    struct Fixed(&'static str, Access);

    #[async_trait::async_trait]
    impl HostTool for Fixed {
        fn def(&self) -> ToolDef {
            ToolDef {
                spec: ToolSpec {
                    name: self.0.into(),
                    description: String::new(),
                    input_schema: json!({"type": "object"}),
                },
                access: self.1,
            }
        }

        async fn call(&self, _: Value) -> ToolOutput {
            ToolOutput::ok("ok")
        }
    }

    #[test]
    fn the_prompt_names_the_app_its_tools_and_what_the_user_sees() {
        let host = AppHost::new("FerrumData", "A database IDE.")
            .tool(Arc::new(Fixed("data_schema", Access::Read)))
            .tool(Arc::new(Fixed("data_write", Access::Execute)));
        host.context.set("Connection: shop (PostgreSQL)");
        let text = prompt_section(&host, Workspace::Advice);
        assert!(text.starts_with("You are built into FerrumData. A database IDE."));
        assert!(text.contains("no code project"));
        assert!(text.contains("read-only tools (data_schema)"));
        assert!(text.contains("actions (data_write)"));
        assert!(text.contains("Connection: shop (PostgreSQL)"));

        host.context.set("  ");
        let text = prompt_section(&host, Workspace::Project);
        assert!(!text.contains("no code project"));
        assert!(!text.contains("looking at"));
    }

    #[test]
    fn app_folders_are_created_and_named_after_the_app() {
        let home = tempfile::tempdir().expect("tempdir");
        let folder = app_folder(home.path(), "FerrumData");
        assert_eq!(folder, home.path().join("apps").join("ferrumdata"));
        assert!(folder.is_dir());
        assert!(app_folder(home.path(), "??").ends_with("chat"));
    }
}
