//! AI agents in the workspace: the `agents:*` commands, the new-session
//! picker, starting / stopping / restarting a session's terminal tab, the
//! Agents panel of the right dock (sessions, and the configured agents
//! with install hints and masked keys), the status bar item, and applying
//! the review's accept / reject to disk and open editors.

use ferrum_ui::prelude::*;
use ferrumeditor_core::agents::review::Written;
use ferrumeditor_core::agents::{self as core_agents, AgentDef, masked};
use ui::{Button, ButtonStyle};

use super::Workspace;
use super::dock::RightTab;
use super::git::ViewMode;
use crate::agents::{AgentSessions, AgentsEvent, ReviewAction, ReviewEvent, relative};
use crate::picker::{PickerItem, PickerKind};
use crate::registry::CommandRegistry;
use crate::settings::Settings;

pub(super) fn register(r: &mut CommandRegistry) {
    r.register("agents:toggle-panel", |w, window, cx| {
        w.toggle_right(RightTab::Agents, window, cx)
    });
    r.register("agents:new-session", |w, window, cx| {
        w.toggle_picker(PickerKind::Agents, window, cx)
    });
    r.register("agents:stop-session", |w, _, cx| {
        if let Some(id) = w.current_session(cx) {
            w.stop_session(id, cx);
        }
    });
    r.register("agents:restart-session", |w, window, cx| {
        if let Some(id) = w.current_session(cx) {
            w.restart_session(id, window, cx);
        }
    });
    r.register("agents:review-changes", |w, window, cx| {
        match w.agents.read(cx).reviewable() {
            Some(id) => w.review_session(id, window, cx),
            None => w.info("No agent sessions yet: Agents: New Session starts one", cx),
        }
    });
    for (name, action) in [
        ("agents:accept-all", ReviewAction::AcceptAll),
        ("agents:reject-all", ReviewAction::RejectAll),
    ] {
        r.register(name, move |w, window, cx| {
            let session = w
                .agent_review
                .read(cx)
                .session
                .or_else(|| w.agents.read(cx).reviewable());
            if let Some(session) = session {
                w.apply_review(session, &action, window, cx);
            }
        });
    }
}

impl Workspace {
    /// The newest session still running, else the newest.
    fn current_session(&self, cx: &App) -> Option<u64> {
        let agents = self.agents.read(cx);
        agents
            .sessions
            .iter()
            .rev()
            .find(|s| s.exit_code.is_none() && !s.stopped)
            .or_else(|| agents.sessions.last())
            .map(|s| s.id)
    }

    pub(super) fn agent_items(&mut self, cx: &mut Context<Self>) -> Option<Vec<PickerItem>> {
        if self.root.is_none() {
            self.info("Open a folder to run an agent in it (⌘⇧O)", cx);
            return None;
        }
        let has_file = self
            .active_editor()
            .is_some_and(|e| e.read(cx).path.is_some());
        let agents = self.agents.read(cx);
        let mut items = Vec::new();
        for agent in AgentSessions::agents(cx) {
            match agents.locate(&agent) {
                Some(program) => {
                    items.push(PickerItem::new(
                        format!("Start {}", agent.name),
                        program.to_string_lossy().to_string(),
                        format!("start:{}", agent.id),
                    ));
                    if has_file && agent.takes_prompt() {
                        items.push(PickerItem::new(
                            format!("Start {} with the current file / selection", agent.name),
                            "as the first prompt",
                            format!("context:{}", agent.id),
                        ));
                    }
                }
                None => items.push(PickerItem::new(
                    format!("Install {}", agent.name),
                    agent
                        .install_hint
                        .clone()
                        .unwrap_or_else(|| format!("`{}` not found on PATH", agent.command)),
                    format!("install:{}", agent.id),
                )),
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
        let Some(agent) = AgentSessions::agents(cx).into_iter().find(|a| a.id == id) else {
            return;
        };
        match verb {
            "install" => self.agents.update(cx, |a, cx| a.install(&agent, cx)),
            "context" => {
                let prompt = self.context_prompt(cx);
                self.start_agent(agent, prompt, window, cx);
            }
            _ => self.start_agent(agent, None, window, cx),
        }
    }

    /// The active file, and its selection if there is one.
    fn context_prompt(&self, cx: &App) -> Option<String> {
        let editor = self.active_editor()?;
        let e = editor.read(cx);
        let path = e.path.as_deref()?;
        let rel = match &self.root {
            Some(root) => relative(root, path),
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

    /// Starts a session of `agent` in the project: snapshot, then a
    /// terminal tab (see [`AgentsEvent::Launch`]).
    pub fn start_agent(
        &mut self,
        agent: AgentDef,
        prompt: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.root.clone() else {
            self.info("Open a folder to run an agent in it (⌘⇧O)", cx);
            return;
        };
        let _ = window;
        let name = agent.name.clone();
        self.agents
            .update(cx, |a, cx| a.start(agent, root, prompt, cx));
        self.show_right(RightTab::Agents, cx);
        self.info(format!("Starting {name}…"), cx);
    }

    pub(super) fn on_agents_event(
        &mut self,
        _: &Entity<AgentSessions>,
        event: &AgentsEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            AgentsEvent::Launch(id) => self.launch_agent_terminal(*id, window, cx),
            AgentsEvent::Notice(message) => self.info(message.clone(), cx),
            AgentsEvent::Error(message) => self.error(message.clone(), cx),
        }
    }

    /// Runs the session's agent in a new terminal tab.
    fn launch_agent_terminal(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let (agent, root, prompt, program) = {
            let agents = self.agents.read(cx);
            let Some(session) = agents.session(id) else {
                return;
            };
            (
                session.agent.clone(),
                session.root.clone(),
                session.prompt.clone(),
                agents.locate(&session.agent),
            )
        };
        let mut words = agent.command_line(prompt.as_deref());
        // The program as found (the shell's PATH may not have it).
        if let Some(program) = program {
            words[0] = program.to_string_lossy().to_string();
        }
        let line = core_agents::shell_line(&words);
        let env: Vec<(String, String)> = agent.env.clone().into_iter().collect();
        let result = self.terminal.update(cx, |panel, cx| {
            panel.cwd = Some(root.clone());
            panel.run_agent(&agent.name, &line, &root, &env, window, cx)
        });
        match result {
            Ok(terminal) => self
                .agents
                .update(cx, |a, cx| a.set_terminal(id, Some(terminal), cx)),
            Err(error) => {
                self.agents.update(cx, |a, cx| a.mark_stopped(id, cx));
                self.error(format!("Could not start {}: {error}", agent.name), cx);
            }
        }
        cx.notify();
    }

    pub fn stop_session(&mut self, id: u64, cx: &mut Context<Self>) {
        let terminal = self.agents.read(cx).session(id).and_then(|s| s.terminal);
        if let Some(view) = terminal.and_then(|t| self.terminal.read(cx).view(t)) {
            view.read(cx).kill();
        }
        self.agents.update(cx, |a, cx| a.mark_stopped(id, cx));
    }

    /// A new process for the session; the snapshot (and so the review)
    /// stays the one taken when the session first started.
    pub fn restart_session(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let terminal = self.agents.read(cx).session(id).and_then(|s| s.terminal);
        if let Some(terminal) = terminal {
            self.terminal
                .update(cx, |p, cx| p.close(terminal, window, cx));
        }
        self.launch_agent_terminal(id, window, cx);
    }

    fn show_session_terminal(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(terminal) = self.agents.read(cx).session(id).and_then(|s| s.terminal) {
            self.terminal
                .update(cx, |p, cx| p.reveal(terminal, window, cx));
        }
        cx.notify();
    }

    /// Opens the AI Diff Review for a session.
    pub fn review_session(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.agents.update(cx, |a, cx| a.rescan_now(id, cx)) {
            self.error(error, cx);
        }
        self.agent_review.update(cx, |r, cx| r.show(id, cx));
        self.set_mode(ViewMode::Review, window, cx);
    }

    pub(super) fn on_review_event(
        &mut self,
        _: &Entity<crate::agents::AgentReview>,
        event: &ReviewEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ReviewEvent::Apply { session, action } => {
                self.apply_review(*session, action, window, cx)
            }
            ReviewEvent::Open { path, line } => {
                let root = self
                    .agents
                    .read(cx)
                    .session(self.agent_review.read(cx).session.unwrap_or_default())
                    .map(|s| s.root.clone());
                if let Some(root) = root {
                    self.set_mode(ViewMode::Editor, window, cx);
                    self.open_file_at(&root.join(path), Some(*line), None, window, cx);
                }
            }
            ReviewEvent::Refresh { session } => {
                if let Err(error) = self.agents.update(cx, |a, cx| a.rescan_now(*session, cx)) {
                    self.error(error, cx);
                }
            }
            ReviewEvent::Close => self.set_mode(ViewMode::Editor, window, cx),
        }
    }

    /// Accepts or rejects. Rejections rewrite files: an open editor with
    /// unsaved edits blocks that (save or revert first); clean ones reload
    /// as one undoable step.
    pub fn apply_review(
        &mut self,
        session: u64,
        action: &ReviewAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (root, writes) = {
            let agents = self.agents.read(cx);
            let Some(s) = agents.session(session) else {
                return;
            };
            (s.root.clone(), action.writes(&s.changes))
        };
        for rel in &writes {
            let path = root.join(rel);
            if let Some(editor) = self
                .all_editors()
                .into_iter()
                .find(|e| e.read(cx).path.as_deref() == Some(path.as_path()))
                && editor.read(cx).is_dirty()
            {
                self.error(
                    format!(
                        "{} has unsaved edits: save or revert it before rejecting",
                        rel.display()
                    ),
                    cx,
                );
                return;
            }
        }
        let result = self.agents.update(cx, |a, cx| a.apply(session, action, cx));
        match result {
            Ok(written) => {
                for (path, outcome) in written {
                    let editor = self
                        .all_editors()
                        .into_iter()
                        .find(|e| e.read(cx).path.as_deref() == Some(path.as_path()));
                    if let (Some(editor), Written::Content(_)) = (editor, outcome)
                        && let Err(error) = editor.update(cx, |e, cx| e.reload_from_disk(cx))
                    {
                        self.error(format!("Reload failed: {error}"), cx);
                    }
                }
                let what = match action {
                    ReviewAction::AcceptHunk { .. } => "Change accepted",
                    ReviewAction::RejectHunk { .. } => "Change rejected",
                    ReviewAction::AcceptFile { .. } => "File accepted",
                    ReviewAction::RejectFile { .. } => "File restored",
                    ReviewAction::AcceptAll => "All changes accepted",
                    ReviewAction::RejectAll => "All changes rejected",
                };
                self.info(what, cx);
                self.refresh_git(cx);
            }
            Err(error) => self.error(error, cx),
        }
        let _ = window;
        cx.notify();
    }

    pub(super) fn render_agent_status(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let threshold = idle_threshold(cx);
        let agents = self.agents.read(cx);
        let session = agents.sessions.last()?;
        let idle = self.session_idle(session.terminal, cx);
        let (files, _, _) = session.change_counts();
        let text = format!(
            "✦ {}: {}{}",
            session.agent.name,
            session.status(idle, threshold).label(),
            if files > 0 {
                format!(" · {files} file{}", if files == 1 { "" } else { "s" })
            } else {
                String::new()
            }
        );
        let _ = agents;
        Some(
            div()
                .id("status-agents")
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.toggle_right(RightTab::Agents, window, cx)
                }))
                .child(Label::new(text).size(LabelSize::Small).color(Color::Muted))
                .into_any_element(),
        )
    }

    fn session_idle(&self, terminal: Option<u64>, cx: &App) -> Option<std::time::Duration> {
        let view = self.terminal.read(cx).view(terminal?)?;
        Some(view.read(cx).last_output.elapsed())
    }

    pub(super) fn render_agents_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let threshold = idle_threshold(cx);
        struct Row {
            id: u64,
            name: String,
            status: core_agents::AgentStatus,
            counts: (usize, usize, usize),
            error: Option<String>,
        }
        let rows: Vec<Row> = {
            let agents = self.agents.read(cx);
            agents
                .sessions
                .iter()
                .rev()
                .map(|s| Row {
                    id: s.id,
                    name: s.agent.name.clone(),
                    status: s.status(self.session_idle(s.terminal, cx), threshold),
                    counts: s.change_counts(),
                    error: s.error.clone(),
                })
                .collect()
        };
        let known: Vec<(AgentDef, Option<std::path::PathBuf>, bool)> = {
            let agents = self.agents.read(cx);
            AgentSessions::agents(cx)
                .into_iter()
                .map(|a| {
                    let found = agents.locate(&a);
                    let installing = agents.installing() == Some(a.name.as_str());
                    (a, found, installing)
                })
                .collect()
        };
        let mut sessions: Vec<AnyElement> = Vec::new();
        for row in rows {
            let id = row.id;
            let status = row.status;
            let (files, added, removed) = row.counts;
            let live = status.is_live();
            let ids = |name: &str| SharedString::from(format!("agent-{name}-{id}"));
            sessions.push(
                v_flex()
                    .px_2()
                    .py_1p5()
                    .gap_1()
                    .border_b_1()
                    .border_color(colors.border_variant)
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().text_sm().child(row.name.clone()))
                            .child(
                                Label::new(status.label())
                                    .size(LabelSize::XSmall)
                                    .color(if live { Color::Accent } else { Color::Muted }),
                            ),
                    )
                    .child(
                        Label::new(if files == 0 {
                            "no changes yet".to_string()
                        } else {
                            format!(
                                "{files} file{} changed · +{added} −{removed}",
                                if files == 1 { "" } else { "s" }
                            )
                        })
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                    )
                    .when_some(row.error.clone(), |d, error| {
                        d.child(
                            Label::new(error)
                                .size(LabelSize::XSmall)
                                .color(Color::Error),
                        )
                    })
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new(ids("review"), "Review")
                                    .style(ButtonStyle::Filled)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.review_session(id, window, cx)
                                    })),
                            )
                            .child(
                                Button::new(ids("terminal"), "Terminal")
                                    .style(ButtonStyle::Subtle)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.show_session_terminal(id, window, cx)
                                    })),
                            )
                            .child(if live {
                                Button::new(ids("stop"), "Stop")
                                    .style(ButtonStyle::Subtle)
                                    .on_click(
                                        cx.listener(move |this, _, _, cx| {
                                            this.stop_session(id, cx)
                                        }),
                                    )
                            } else {
                                Button::new(ids("restart"), "Restart")
                                    .style(ButtonStyle::Subtle)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.restart_session(id, window, cx)
                                    }))
                            })
                            .when(!live, |d| {
                                d.child(
                                    Button::new(ids("remove"), "Remove")
                                        .style(ButtonStyle::Subtle)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.agents.update(cx, |a, cx| a.remove(id, cx))
                                        })),
                                )
                            }),
                    )
                    .into_any_element(),
            );
        }
        // Configured agents: found or how to install; keys masked.
        let mut configured: Vec<AnyElement> = Vec::new();
        for (i, (agent, found, installing)) in known.into_iter().enumerate() {
            let env = agent
                .env
                .iter()
                .map(|(k, v)| format!("{k}={}", masked(v)))
                .collect::<Vec<_>>()
                .join("  ");
            let detail = match (&found, &agent.install_hint) {
                (Some(path), _) => path.to_string_lossy().to_string(),
                (None, Some(hint)) => format!("not installed — {hint}"),
                (None, None) => format!("`{}` not found on PATH", agent.command),
            };
            let can_install = found.is_none() && agent.npm_package.is_some();
            configured.push(
                v_flex()
                    .px_2()
                    .py_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().text_sm().child(agent.name.clone()))
                            .child(div().flex_1())
                            .when(can_install, |d| {
                                let agent = agent.clone();
                                d.child(
                                    Button::new(
                                        ("agent-install", i),
                                        if installing {
                                            "Installing…"
                                        } else {
                                            "Install"
                                        },
                                    )
                                    .style(ButtonStyle::Subtle)
                                    .disabled(installing)
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            this.agents.update(cx, |a, cx| a.install(&agent, cx))
                                        },
                                    )),
                                )
                            })
                            .when(found.is_some(), |d| {
                                let agent = agent.clone();
                                d.child(
                                    Button::new(("agent-start", i), "Start")
                                        .style(ButtonStyle::Subtle)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.start_agent(agent.clone(), None, window, cx)
                                        })),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(colors.text_muted)
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(detail),
                    )
                    .when(!env.is_empty(), |d| {
                        d.child(
                            div()
                                .text_xs()
                                .text_color(colors.text_muted)
                                .child(format!("env: {env}")),
                        )
                    })
                    .into_any_element(),
            );
        }
        v_flex()
            .id("agents-panel")
            .size_full()
            .overflow_y_scroll()
            .child(
                h_flex()
                    .p_2()
                    .gap_2()
                    .child(
                        Button::new("agents-new", "New Session…")
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_picker(PickerKind::Agents, window, cx)
                            })),
                    )
                    .child(
                        Button::new("agents-review", "Review Changes")
                            .style(ButtonStyle::Subtle)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run_command("agents:review-changes", window, cx)
                            })),
                    ),
            )
            .when(sessions.is_empty(), |d| {
                d.child(
                    div()
                        .px_2()
                        .text_sm()
                        .text_color(colors.text_muted)
                        .child("No sessions. Start one to run an agent in a terminal tab; its changes are tracked for review."),
                )
            })
            .children(sessions)
            .child(
                div()
                    .px_2()
                    .pt_3()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .child("AGENTS ([[agents]] in config.toml)"),
            )
            .children(configured)
            .into_any_element()
    }
}

fn idle_threshold(cx: &App) -> std::time::Duration {
    std::time::Duration::from_secs(cx.global::<Settings>().config.agent_idle_seconds.max(1))
}
