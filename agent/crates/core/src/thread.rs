//! The thread model shared by the built-in agent and ACP agents: user and
//! assistant messages made of text, thinking, tool calls and plans, with
//! token usage. Everything that changes a thread is a [`ThreadEvent`];
//! [`Thread::apply`] is the one reducer, used by the session (which
//! persists) and by any UI keeping its own copy.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::changeset::CheckpointId;
use crate::modes::Mode;
use crate::provider::{Message, Part, Role, StopReason, Usage};
use crate::tools::Access;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Pending,
    AwaitingPermission,
    Running,
    Completed,
    Failed,
    Denied,
    Cancelled,
}

impl ToolStatus {
    pub fn is_done(self) -> bool {
        matches!(
            self,
            ToolStatus::Completed | ToolStatus::Failed | ToolStatus::Denied | ToolStatus::Cancelled
        )
    }
}

/// An edit shown on a tool card (ACP diffs, or our own edits).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffPreview {
    pub path: PathBuf,
    pub old_text: Option<String>,
    pub new_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub id: String,
    pub name: String,
    pub title: String,
    pub input: Value,
    pub access: Option<Access>,
    pub status: ToolStatus,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub changed: Vec<PathBuf>,
    #[serde(default)]
    pub diffs: Vec<DiffPreview>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanEntry {
    pub content: String,
    /// `pending`, `in_progress`, `completed`.
    pub status: String,
    #[serde(default)]
    pub priority: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Text { text: String },
    Thinking { text: String },
    Tool { call: ToolCallRecord },
    Plan { entries: Vec<PlanEntry> },
    Error { message: String },
}

/// Context attached to a user message (a resolved @-mention or rule).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadMessage {
    pub id: u64,
    pub role: Role,
    pub blocks: Vec<Block>,
    /// For user messages: the checkpoint taken before the turn.
    #[serde(default)]
    pub checkpoint: Option<CheckpointId>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

impl ThreadMessage {
    /// All text blocks joined.
    pub fn text(&self) -> String {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub title: String,
    pub project: PathBuf,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    /// `provider/model` or `acp:<agent id>`.
    pub agent: String,
    pub mode: Mode,
    pub messages: Vec<ThreadMessage>,
    /// Summed over every request.
    pub usage: Usage,
    /// Tokens in the context at the last request.
    #[serde(default)]
    pub context_tokens: u64,
    /// The model's context size when known.
    #[serde(default)]
    pub context_window: Option<u64>,
    #[serde(default)]
    pub plan: Option<String>,
    /// An ACP agent's session id, for `session/load`.
    #[serde(default)]
    pub acp_session_id: Option<String>,
    #[serde(default)]
    pub last_stop: Option<StopReason>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ThreadEvent {
    UserMessage {
        id: u64,
        text: String,
        attachments: Vec<Attachment>,
        checkpoint: Option<CheckpointId>,
    },
    AssistantStarted {
        id: u64,
    },
    TextDelta(String),
    ThinkingDelta(String),
    ToolCallStarted(ToolCallRecord),
    ToolCallUpdated {
        id: String,
        status: Option<ToolStatus>,
        title: Option<String>,
        output: Option<String>,
        is_error: Option<bool>,
        changed: Vec<PathBuf>,
        diffs: Vec<DiffPreview>,
    },
    PlanEntries(Vec<PlanEntry>),
    PlanWritten(String),
    /// One request's usage, and the context size it saw.
    Usage {
        usage: Usage,
        context_tokens: u64,
    },
    ContextWindow(u64),
    ModeChanged(Mode),
    TitleChanged(String),
    Error(String),
    TurnEnded(StopReason),
    /// Messages after this id were rolled back with a checkpoint restore.
    Truncated {
        keep_through: u64,
    },
}

impl ThreadEvent {
    /// A tool update with only a status.
    pub fn tool_status(id: &str, status: ToolStatus) -> Self {
        ThreadEvent::ToolCallUpdated {
            id: id.into(),
            status: Some(status),
            title: None,
            output: None,
            is_error: None,
            changed: Vec::new(),
            diffs: Vec::new(),
        }
    }
}

impl Thread {
    pub fn new(project: PathBuf, agent: impl Into<String>, mode: Mode) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: "New thread".into(),
            project,
            created: now,
            updated: now,
            agent: agent.into(),
            mode,
            messages: Vec::new(),
            usage: Usage::default(),
            context_tokens: 0,
            context_window: None,
            plan: None,
            acp_session_id: None,
            last_stop: None,
        }
    }

    pub fn next_message_id(&self) -> u64 {
        self.messages.last().map_or(1, |m| m.id + 1)
    }

    fn assistant(&mut self) -> &mut ThreadMessage {
        if self
            .messages
            .last()
            .is_none_or(|m| m.role != Role::Assistant)
        {
            let id = self.next_message_id();
            self.messages.push(ThreadMessage {
                id,
                role: Role::Assistant,
                blocks: Vec::new(),
                checkpoint: None,
                attachments: Vec::new(),
            });
        }
        let last = self.messages.len() - 1;
        &mut self.messages[last]
    }

    pub fn tool_mut(&mut self, id: &str) -> Option<&mut ToolCallRecord> {
        self.messages
            .iter_mut()
            .rev()
            .flat_map(|m| m.blocks.iter_mut())
            .find_map(|b| match b {
                Block::Tool { call } if call.id == id => Some(call),
                _ => None,
            })
    }

    pub fn tool(&self, id: &str) -> Option<&ToolCallRecord> {
        self.messages
            .iter()
            .rev()
            .flat_map(|m| m.blocks.iter())
            .find_map(|b| match b {
                Block::Tool { call } if call.id == id => Some(call),
                _ => None,
            })
    }

    pub fn apply(&mut self, event: &ThreadEvent) {
        self.updated = Utc::now();
        match event {
            ThreadEvent::UserMessage {
                id,
                text,
                attachments,
                checkpoint,
            } => {
                if self.messages.iter().all(|m| m.role != Role::User) {
                    self.title = title_from(text);
                }
                self.messages.push(ThreadMessage {
                    id: *id,
                    role: Role::User,
                    blocks: vec![Block::Text { text: text.clone() }],
                    checkpoint: *checkpoint,
                    attachments: attachments.clone(),
                });
            }
            ThreadEvent::AssistantStarted { id } => self.messages.push(ThreadMessage {
                id: *id,
                role: Role::Assistant,
                blocks: Vec::new(),
                checkpoint: None,
                attachments: Vec::new(),
            }),
            ThreadEvent::TextDelta(delta) => {
                let message = self.assistant();
                match message.blocks.last_mut() {
                    Some(Block::Text { text }) => text.push_str(delta),
                    _ => message.blocks.push(Block::Text {
                        text: delta.clone(),
                    }),
                }
            }
            ThreadEvent::ThinkingDelta(delta) => {
                let message = self.assistant();
                match message.blocks.last_mut() {
                    Some(Block::Thinking { text }) => text.push_str(delta),
                    _ => message.blocks.push(Block::Thinking {
                        text: delta.clone(),
                    }),
                }
            }
            ThreadEvent::ToolCallStarted(call) => {
                if let Some(existing) = self.tool_mut(&call.id) {
                    *existing = call.clone();
                } else {
                    self.assistant()
                        .blocks
                        .push(Block::Tool { call: call.clone() });
                }
            }
            ThreadEvent::ToolCallUpdated {
                id,
                status,
                title,
                output,
                is_error,
                changed,
                diffs,
            } => {
                if let Some(call) = self.tool_mut(id) {
                    if let Some(status) = status {
                        call.status = *status;
                    }
                    if let Some(title) = title {
                        call.title = title.clone();
                    }
                    if let Some(output) = output {
                        call.output = Some(output.clone());
                    }
                    if let Some(is_error) = is_error {
                        call.is_error = *is_error;
                    }
                    for path in changed {
                        if !call.changed.contains(path) {
                            call.changed.push(path.clone());
                        }
                    }
                    call.diffs.extend(diffs.iter().cloned());
                }
            }
            ThreadEvent::PlanEntries(entries) => {
                let message = self.assistant();
                match message
                    .blocks
                    .iter_mut()
                    .find(|b| matches!(b, Block::Plan { .. }))
                {
                    Some(Block::Plan { entries: old }) => *old = entries.clone(),
                    _ => message.blocks.push(Block::Plan {
                        entries: entries.clone(),
                    }),
                }
            }
            ThreadEvent::PlanWritten(markdown) => self.plan = Some(markdown.clone()),
            ThreadEvent::Usage {
                usage,
                context_tokens,
            } => {
                self.usage.add(*usage);
                self.context_tokens = *context_tokens;
            }
            ThreadEvent::ContextWindow(size) => self.context_window = Some(*size),
            ThreadEvent::ModeChanged(mode) => self.mode = *mode,
            ThreadEvent::TitleChanged(title) => self.title = title.clone(),
            ThreadEvent::Error(message) => self.assistant().blocks.push(Block::Error {
                message: message.clone(),
            }),
            ThreadEvent::TurnEnded(stop) => {
                self.last_stop = Some(stop.clone());
                // Anything still running when the turn ends was cut off.
                for call in self.messages.iter_mut().flat_map(|m| {
                    m.blocks.iter_mut().filter_map(|b| match b {
                        Block::Tool { call } => Some(call),
                        _ => None,
                    })
                }) {
                    if !call.status.is_done() {
                        call.status = ToolStatus::Cancelled;
                    }
                }
            }
            ThreadEvent::Truncated { keep_through } => {
                self.messages.retain(|m| m.id <= *keep_through);
            }
        }
    }

    /// The history as provider messages: tool calls become tool_use parts
    /// followed by a user message of their results; same-role neighbours
    /// are merged.
    pub fn to_messages(&self) -> Vec<Message> {
        let mut out: Vec<Message> = Vec::new();
        let mut push = |message: Message| {
            if message.parts.is_empty() {
                return;
            }
            match out.last_mut() {
                Some(last) if last.role == message.role => last.parts.extend(message.parts),
                _ => out.push(message),
            }
        };
        for message in &self.messages {
            match message.role {
                Role::User => {
                    let mut text = message.text();
                    for attachment in &message.attachments {
                        text.push_str(&format!(
                            "\n\n<context label=\"{}\">\n{}\n</context>",
                            attachment.label, attachment.text
                        ));
                    }
                    push(Message {
                        role: Role::User,
                        parts: vec![Part::Text { text }],
                    });
                }
                Role::Assistant => {
                    let mut parts = Vec::new();
                    let mut results = Vec::new();
                    for block in &message.blocks {
                        match block {
                            Block::Text { text } if !text.is_empty() => {
                                parts.push(Part::Text { text: text.clone() })
                            }
                            Block::Tool { call } => {
                                parts.push(Part::ToolUse {
                                    id: call.id.clone(),
                                    name: call.name.clone(),
                                    input: call.input.clone(),
                                });
                                let (content, is_error) = match call.status {
                                    ToolStatus::Denied => {
                                        ("The user denied this tool call.".to_string(), true)
                                    }
                                    ToolStatus::Completed | ToolStatus::Failed => {
                                        (call.output.clone().unwrap_or_default(), call.is_error)
                                    }
                                    _ => ("Cancelled.".to_string(), true),
                                };
                                results.push(Part::ToolResult {
                                    id: call.id.clone(),
                                    content,
                                    is_error,
                                });
                            }
                            _ => {}
                        }
                    }
                    push(Message {
                        role: Role::Assistant,
                        parts,
                    });
                    push(Message {
                        role: Role::User,
                        parts: results,
                    });
                }
            }
        }
        out
    }
}

/// A title from the first prompt: its first line, at most 60 chars.
pub fn title_from(text: &str) -> String {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let mut title: String = line.chars().take(60).collect();
    if line.chars().count() > 60 {
        title.push('…');
    }
    if title.is_empty() {
        "New thread".into()
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(id: &str) -> ToolCallRecord {
        ToolCallRecord {
            id: id.into(),
            name: "read_file".into(),
            title: "Read a".into(),
            input: json!({"path": "a"}),
            access: Some(Access::Read),
            status: ToolStatus::Running,
            output: None,
            is_error: false,
            changed: vec![],
            diffs: vec![],
        }
    }

    #[test]
    fn reducer_builds_messages_and_provider_history() {
        let mut t = Thread::new("/p".into(), "mock/m", Mode::Agent);
        t.apply(&ThreadEvent::UserMessage {
            id: 1,
            text: "Explain a\nplease".into(),
            attachments: vec![Attachment {
                label: "a".into(),
                text: "fn a()".into(),
            }],
            checkpoint: Some(1),
        });
        assert_eq!(t.title, "Explain a");
        t.apply(&ThreadEvent::TextDelta("Let me ".into()));
        t.apply(&ThreadEvent::TextDelta("look.".into()));
        t.apply(&ThreadEvent::ToolCallStarted(call("t1")));
        t.apply(&ThreadEvent::ToolCallStarted(call("t2")));
        t.apply(&ThreadEvent::ToolCallUpdated {
            id: "t1".into(),
            status: Some(ToolStatus::Completed),
            title: None,
            output: Some("fn a()".into()),
            is_error: Some(false),
            changed: vec![],
            diffs: vec![],
        });
        t.apply(&ThreadEvent::Usage {
            usage: Usage {
                input_tokens: 10,
                output_tokens: 2,
                cache_read_tokens: 0,
            },
            context_tokens: 12,
        });
        t.apply(&ThreadEvent::TurnEnded(StopReason::EndTurn));
        assert_eq!(t.messages.len(), 2);
        assert_eq!(t.messages[1].text(), "Let me look.");
        assert_eq!(t.tool("t2").map(|c| c.status), Some(ToolStatus::Cancelled));
        assert_eq!(t.usage.input_tokens, 10);

        let history = t.to_messages();
        assert_eq!(history.len(), 3);
        assert!(
            matches!(&history[0].parts[0], Part::Text { text } if text.contains("<context label=\"a\">"))
        );
        assert_eq!(history[1].parts.len(), 3);
        assert!(matches!(
            &history[2].parts[1],
            Part::ToolResult { is_error: true, .. }
        ));

        // A follow-up user message merges into the tool-result message.
        t.apply(&ThreadEvent::UserMessage {
            id: 3,
            text: "thanks".into(),
            attachments: vec![],
            checkpoint: None,
        });
        assert_eq!(t.to_messages().len(), 3);
        t.apply(&ThreadEvent::Truncated { keep_through: 1 });
        assert_eq!(t.messages.len(), 1);
        assert_eq!(title_from(&"x".repeat(70)).chars().count(), 61);
    }
}
