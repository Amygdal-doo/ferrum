//! FerrumChat beside the terminal (the desktop window): the Agent toggle
//! before the account opens it as a card right of the terminal (and of
//! FerrumGIT when that is open too). The chat is on the focused pane's
//! repository when it has one — the agent can then read and edit the
//! project like FerrumIDE's — else it has no project. It is picked when
//! the pane opens; each folder keeps its chat. Before each message the
//! chat learns the focused pane, its folder and its screen, and two tools
//! read that screen and type a command into the pane (see
//! [`crate::agent_tools`]). "Ask agent" on a failed command block opens
//! the pane with the command, its exit code and its output attached and a
//! question ready to send (see [`TermView::ask_agent_about_command`]).

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use ferrum_ui::components::{Clickable, IconButton};
use ferrum_ui::gpui::{App, Context, Entity, Window};
use ferrumagent_ui::core::context::Mention;
use ferrumagent_ui::core::host::{AppHost, HostTool};
use ferrumagent_ui::core::session::SessionConfig;
use ferrumagent_ui::{AgentPane, ChatEvent, ChatOptions, ChatView};
use ferrumterm_protocol::{BlockId, Input, Request, Response};
use futures::StreamExt as _;

use crate::agent_tools::{self, Bridge, TermCall};
use crate::view::TermView;

/// Screen lines in the context at most (the most recent).
const CONTEXT_LINES: usize = 60;
/// After typing a command its screen is read every `RUN_POLL`, until it
/// stops changing, at most `RUN_POLLS` times (10 s).
const RUN_POLL: Duration = Duration::from_millis(500);
const RUN_POLLS: usize = 20;

const ABOUT: &str = "FerrumTerm is a terminal: tabs and split panes of shells and coding \
                     agents, sessions that keep running when the window closes, SSH hosts.";

/// The context chip for a failed command (the command, where it ran, its
/// output, its exit code) and the question to send with it.
pub(crate) fn failed_command_context(
    command: &ferrumterm_protocol::CommandMark,
    cwd: Option<&str>,
    output: &str,
) -> (Mention, String) {
    let code = command
        .exit
        .map_or_else(|| "an unknown code".to_owned(), |c| c.to_string());
    let line = if command.command.is_empty() {
        "the last command".to_owned()
    } else {
        format!("`{}`", command.command)
    };
    let mut text = String::new();
    if let Some(cwd) = cwd {
        text.push_str(&format!("In {cwd}:\n"));
    }
    text.push_str(&format!("$ {}\n", command.command));
    if !output.is_empty() {
        text.push_str(output);
        text.push('\n');
    }
    text.push_str(&format!("[exit code {code}]"));
    let mention = Mention::Terminal {
        title: format!("{line} exited with {code}"),
        text,
    };
    let question = format!(
        "{line} failed with exit code {code} (its output is attached). What went wrong, and how \
         do I fix it?"
    );
    (mention, question)
}

/// The Agent pane's state.
pub(crate) struct AgentState {
    pub(crate) pane: AgentPane,
    /// Chats by repository (`None`: the one without a project).
    chats: HashMap<Option<PathBuf>, Entity<ChatView>>,
    /// The tools' channel to this view.
    bridge: Bridge,
    _calls: ferrum_ui::gpui::Task<()>,
}

impl AgentState {
    /// Listens for the tools' calls on `view`. Without the pane (`enabled`
    /// false: embedded views) nothing else starts.
    pub(crate) fn new(enabled: bool, cx: &mut Context<TermView>) -> Self {
        let (bridge, mut rx) = Bridge::new();
        let calls = cx.spawn(async move |this, cx| {
            while let Some((call, reply)) = rx.next().await {
                if this
                    .update(cx, |view, cx| view.agent_call(call, reply, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            pane: if enabled {
                AgentPane::new()
            } else {
                AgentPane::default()
            },
            chats: HashMap::new(),
            bridge,
            _calls: calls,
        }
    }
}

impl TermView {
    /// The tools the Agent pane's chat gets.
    pub fn agent_tools(&self) -> Vec<std::sync::Arc<dyn HostTool>> {
        agent_tools::tools(&self.agent.bridge)
    }

    /// Whether the Agent pane is open.
    pub fn agent_shown(&self) -> bool {
        self.agent.pane.is_shown()
    }

    /// The chat the Agent pane shows (or showed last).
    pub fn agent_chat(&self) -> Option<Entity<ChatView>> {
        self.agent.pane.chat().cloned()
    }

    /// Shows the pane on the focused pane's repository (or without a
    /// project), or hides it.
    pub fn toggle_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.agent.pane.is_shown() {
            self.agent.pane.hide(cx);
            cx.notify();
            return;
        }
        let repo = self
            .focused_folder()
            .and_then(|f| ferrumagent_ui::core::fsutil::repo_root(&f));
        let chat = match self.agent.chats.get(&repo) {
            Some(chat) => chat.clone(),
            None => {
                let chat = self.new_chat(repo.clone(), window, cx);
                self.agent.chats.insert(repo, chat.clone());
                chat
            }
        };
        self.agent.pane.show(chat, window, cx);
        cx.notify();
    }

    fn new_chat(
        &mut self,
        repo: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ChatView> {
        let host = AppHost {
            tools: self.agent_tools(),
            ..AppHost::new("FerrumTerm", ABOUT)
        };
        let weak = cx.weak_entity();
        let context = Rc::new(move |cx: &mut App| {
            weak.read_with(cx, |view, _| view.agent_context())
                .unwrap_or_default()
        });
        let home = self.options.agent_home.clone();
        let config =
            home.map(|home| SessionConfig::isolated(repo.as_ref().unwrap_or(&home), &home));
        let options = ChatOptions {
            config,
            ..AgentPane::options(Some(context))
        };
        let chat = match repo {
            Some(root) => ChatView::new(
                &root,
                ChatOptions {
                    host: Some(host),
                    ..options
                },
                window,
                cx,
            ),
            None => ChatView::without_project(host, options, window, cx),
        };
        cx.subscribe(&chat, |view, chat, event: &ChatEvent, cx| match event {
            ChatEvent::OpenFile { path, line } => {
                let project = chat.read(cx).session().map(|s| s.project().to_path_buf());
                if let Err(error) =
                    ferrum_ui::project::open_file_in_editor(project.as_deref(), path, *line)
                {
                    view.app.notify(error);
                    cx.notify();
                }
            }
            ChatEvent::RunInTerminal { cmd, cwd } => {
                view.run_command(cmd, Some(cwd.clone()), cx);
            }
            _ => {}
        })
        .detach();
        chat
    }

    /// "Ask agent" on command `id` of pane `block`: shows the Agent pane,
    /// attaches the command, its exit code and its output (the server's
    /// copy, scrollback included), and puts a question in the composer for
    /// the user to send. Whatever the agent then wants to run still asks
    /// first.
    pub fn ask_agent_about_command(
        &mut self,
        block: BlockId,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(command) = self
            .app
            .frames
            .get(&block)
            .and_then(|f| f.commands.iter().find(|c| c.id == id))
            .cloned()
        else {
            return;
        };
        let Some(client) = self.app.client.clone() else {
            return;
        };
        if !self.agent.pane.is_shown() {
            self.toggle_agent(window, cx);
        }
        let Some(chat) = self.agent.pane.chat().cloned() else {
            return;
        };
        let cwd = self
            .app
            .sessions
            .iter()
            .flat_map(|s| &s.windows)
            .flat_map(|w| &w.blocks)
            .find(|b| b.id == block)
            .and_then(|b| b.cwd.clone());
        let (tx, rx) = futures::channel::oneshot::channel();
        self.app.rt.spawn(async move {
            let request = Request::CommandOutput {
                target: Some(block.to_string()),
                command: id,
            };
            let output = match client.request(request).await {
                Ok(Response::Text { text }) => text,
                Ok(_) => String::new(),
                Err(e) => format!("(its output could not be read: {e})"),
            };
            let _ = tx.send(output);
        });
        cx.spawn_in(window, async move |this, cx| {
            let output = rx.await.unwrap_or_default();
            let (mention, question) = failed_command_context(&command, cwd.as_deref(), &output);
            let _ = this.update_in(cx, |_, window, cx| {
                chat.update(cx, |chat, cx| {
                    chat.add_mention(mention, cx);
                    chat.set_draft(&question, window, cx);
                });
            });
        })
        .detach();
    }

    /// The Agent toggle, for the pane header (before the account).
    pub(crate) fn agent_toggle(&self, cx: &mut Context<Self>) -> IconButton {
        ferrumagent_ui::agent_toggle("toggle-agent", self.agent.pane.is_shown())
            .on_click(cx.listener(|view, _, window, cx| view.toggle_agent(window, cx)))
    }

    /// The focused pane's working directory: the live one when known.
    fn focused_folder(&self) -> Option<PathBuf> {
        if let Some(dir) = self.project.as_ref().and_then(|p| p.focused_dir()) {
            return Some(dir);
        }
        let window = self.app.window()?;
        let block = window.blocks.iter().find(|b| b.id == window.focused)?;
        block.cwd.as_ref().map(PathBuf::from)
    }

    /// The focused pane's screen, without the empty lines below the text.
    fn screen_text(&self) -> Option<String> {
        let frame = self.app.frames.get(&self.app.focused()?)?;
        let mut lines: Vec<String> = (0..frame.lines.len()).map(|r| frame.line_text(r)).collect();
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        Some(lines.join("\n"))
    }

    /// What the window shows, for the chat.
    pub fn agent_context(&self) -> String {
        let mut out = String::new();
        if let Some(session) = self.app.session() {
            out.push_str(&format!(
                "Session: {} ({} tabs)\n",
                session.name,
                session.windows.len()
            ));
        }
        let block = self
            .app
            .window()
            .and_then(|w| w.blocks.iter().find(|b| b.id == w.focused));
        if let Some(block) = block {
            let name = block.label.clone().unwrap_or_else(|| block.title.clone());
            out.push_str(&format!("Focused pane: \"{name}\""));
            if !block.command.is_empty() {
                out.push_str(&format!(", started as `{}`", block.command.join(" ")));
            }
            if let Some(agent) = &block.agent {
                out.push_str(&format!(", running the agent {agent}"));
            }
            if let Some(status) = &block.status {
                out.push_str(&format!(", {:?}", status.state));
            }
            if let Some(code) = block.exited {
                out.push_str(&format!(", exited with {code}"));
            }
            out.push('\n');
        }
        if let Some(folder) = self.focused_folder() {
            out.push_str(&format!("Folder: {}", folder.display()));
            if let Some(repo) = ferrumagent_ui::core::fsutil::repo_root(&folder) {
                out.push_str(&format!(" (git repository {})", repo.display()));
            }
            out.push('\n');
        }
        if let Some(screen) = self.screen_text().filter(|s| !s.trim().is_empty()) {
            let lines: Vec<&str> = screen.lines().collect();
            let start = lines.len().saturating_sub(CONTEXT_LINES);
            out.push_str(&format!(
                "Its screen (latest at the bottom):\n```text\n{}\n```\n",
                lines[start..].join("\n")
            ));
        }
        out
    }

    /// Whether "Ask agent" is offered (the view has the Agent pane).
    pub(crate) fn can_ask_agent(&self) -> bool {
        self.options.agent
    }

    /// A tool's call (see [`crate::agent_tools`]).
    fn agent_call(
        &mut self,
        call: TermCall,
        reply: futures::channel::oneshot::Sender<Result<String, String>>,
        cx: &mut Context<Self>,
    ) {
        match call {
            TermCall::Screen => {
                let _ = reply.send(
                    self.screen_text()
                        .ok_or_else(|| "No terminal pane is showing.".to_string()),
                );
            }
            TermCall::Run(line) => {
                let Some(block) = self.app.focused().filter(|_| self.app.client.is_some()) else {
                    let _ = reply.send(Err("No terminal pane is connected.".into()));
                    return;
                };
                self.app.send(Request::Input {
                    target: Some(block.to_string()),
                    input: vec![Input::Text(line.clone()), Input::Key("Enter".into())],
                });
                cx.spawn(async move |this, cx| {
                    // The screen once it stops changing (or at the limit:
                    // a long command is still running).
                    let mut last = None;
                    for _ in 0..RUN_POLLS {
                        cx.background_executor().timer(RUN_POLL).await;
                        let Ok(screen) = this.read_with(cx, |view, _| view.screen_text()) else {
                            return;
                        };
                        if screen.is_some() && screen == last {
                            break;
                        }
                        last = screen;
                    }
                    let screen = last.unwrap_or_default();
                    let _ = reply.send(Ok(format!(
                        "Typed `{line}` and Enter. The screen a moment later:\n{screen}"
                    )));
                })
                .detach();
            }
        }
    }
}
