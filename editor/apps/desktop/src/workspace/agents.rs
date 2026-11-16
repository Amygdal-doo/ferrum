//! AI agents: ⌥⌘A toggles the Agent pane (FerrumChat, which also drives
//! Claude Code, Codex, Gemini… over ACP), and *Agents: New Session* starts
//! a configured CLI agent (`[[agents]]` in config.toml) in a Term pane tab.

use ferrum_ui::prelude::*;
use ferrumeditor_core::agents::{self as core_agents, AgentDef};
use ferrumeditor_core::tool_layout::ToolKind;

use super::Workspace;
use crate::picker::{PickerItem, PickerKind};
use crate::registry::CommandRegistry;
use crate::settings::Settings;

pub(super) fn register(r: &mut CommandRegistry) {
    r.register("agents:toggle-panel", |w, window, cx| {
        w.toggle_tool_focus(ToolKind::Agent, window, cx)
    });
    r.register("agents:new-session", |w, window, cx| {
        w.toggle_picker(PickerKind::Agents, window, cx)
    });
}

fn agents(cx: &App) -> Vec<AgentDef> {
    core_agents::all_agents(&cx.global::<Settings>().config.agents)
}

fn path_env() -> String {
    std::env::var("PATH").unwrap_or_default()
}

impl Workspace {
    pub(super) fn agent_items(&mut self, cx: &mut Context<Self>) -> Option<Vec<PickerItem>> {
        if self.root.is_none() {
            self.info("Open a folder to run an agent in it (⌘⇧O)", cx);
            return None;
        }
        let has_file = self
            .active_editor()
            .is_some_and(|e| e.read(cx).path.is_some());
        let path = path_env();
        let mut items = Vec::new();
        for agent in agents(cx) {
            let logo = ferrum_ui::AgentLogo::for_program(&agent.id)
                .or_else(|| ferrum_ui::AgentLogo::for_program(&agent.command));
            match core_agents::locate(&agent, &path) {
                Some(program) => {
                    items.push(
                        PickerItem::new(
                            format!("Start {} in the terminal", agent.name),
                            program.to_string_lossy().to_string(),
                            format!("start:{}", agent.id),
                        )
                        .with_logo(logo),
                    );
                    if has_file && agent.takes_prompt() {
                        items.push(
                            PickerItem::new(
                                format!("Start {} with the current file / selection", agent.name),
                                "as the first prompt",
                                format!("context:{}", agent.id),
                            )
                            .with_logo(logo),
                        );
                    }
                }
                None => items.push(
                    PickerItem::new(
                        format!("Install {}", agent.name),
                        agent
                            .install_hint
                            .clone()
                            .unwrap_or_else(|| format!("`{}` not found on PATH", agent.command)),
                        format!("install:{}", agent.id),
                    )
                    .with_logo(logo),
                ),
            }
        }
        Some(items)
    }

    pub(super) fn agent_payload(
        &mut self,
        payload: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((verb, id)) = payload.split_once(':') else {
            return;
        };
        let Some(agent) = agents(cx).into_iter().find(|a| a.id == id) else {
            return;
        };
        match verb {
            "install" => match &agent.npm_package {
                Some(package) => {
                    let line = core_agents::shell_line(&[
                        "npm".into(),
                        "install".into(),
                        "-g".into(),
                        package.clone(),
                    ]);
                    self.run_in_terminal(&line, None, window, cx);
                }
                None => self.info(
                    match &agent.install_hint {
                        Some(hint) => format!("Install {} with: {hint}", agent.name),
                        None => format!("{} is not installed", agent.name),
                    },
                    cx,
                ),
            },
            "context" => {
                let prompt = self.context_prompt(cx);
                self.start_agent(&agent, prompt, window, cx);
            }
            _ => self.start_agent(&agent, None, window, cx),
        }
    }

    /// The active file, and its selection if there is one.
    fn context_prompt(&self, cx: &App) -> Option<String> {
        let editor = self.active_editor()?;
        let e = editor.read(cx);
        let path = e.path.as_deref()?;
        let rel = match &self.root {
            Some(root) => path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string(),
            None => path.to_string_lossy().to_string(),
        };
        let selection = e.state.primary();
        let buffer = e.state.buffer();
        let text = (!selection.is_empty()).then(|| buffer.slice_cow(selection.range()).to_string());
        let range = selection.range();
        let first = buffer.row_of(range.start) + 1;
        let last = buffer.row_of(range.end) + 1;
        Some(core_agents::context_prompt(
            &rel,
            text.as_deref().map(|t| (first, last, t)),
        ))
    }

    /// Runs `agent`'s CLI in a new Term pane tab in the project folder.
    pub fn start_agent(
        &mut self,
        agent: &AgentDef,
        prompt: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.root.clone() else {
            self.info("Open a folder to run an agent in it (⌘⇧O)", cx);
            return;
        };
        let mut words = agent.command_line(prompt.as_deref());
        if let Some(program) = core_agents::locate(agent, &path_env()) {
            words[0] = program.to_string_lossy().to_string();
        }
        if !agent.env.is_empty() {
            let env = agent.env.iter().map(|(k, v)| format!("{k}={v}"));
            words = std::iter::once("env".to_string())
                .chain(env)
                .chain(words)
                .collect();
        }
        self.run_in_terminal(&core_agents::shell_line(&words), Some(root), window, cx);
        self.info(format!("Started {} in the terminal", agent.name), cx);
    }
}
