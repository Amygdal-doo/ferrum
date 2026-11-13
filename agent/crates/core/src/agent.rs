//! The built-in agent loop: stream the model, run the tool calls it makes
//! (mode check → permission policy → maybe ask the user → run), feed the
//! results back, and repeat until the model ends its turn.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::modes::Mode;
use crate::permissions::{Decision, PermissionBroker, PermissionRequest, Permissions, Policy};
use crate::provider::{ChatRequest, Message, Provider, StopReason, StreamEvent, Usage};
use crate::rules::Rule;
use crate::thread::{ThreadEvent, ToolCallRecord, ToolStatus};
use crate::tools::{self, ToolContext, ToolRegistry};

/// Model requests per user turn before the agent stops and asks.
pub const MAX_ROUNDS: usize = 40;

/// Where a turn reads history from and reports to.
pub trait TurnSink: Send + Sync {
    fn emit(&self, event: ThreadEvent);
    fn history(&self) -> Vec<Message>;
    fn next_message_id(&self) -> u64;
    /// Files were written (the review should refresh).
    fn changes_updated(&self);
}

pub struct NativeAgent {
    pub provider: Arc<dyn Provider>,
    pub model: String,
    pub max_tokens: u32,
    pub tools: Arc<ToolRegistry>,
    pub ctx: ToolContext,
    pub broker: Arc<dyn PermissionBroker>,
    pub permissions: Arc<Mutex<Permissions>>,
    pub rules: Vec<Rule>,
}

/// The system prompt: who we are, the project, the mode, the rules.
pub fn system_prompt(
    root: &std::path::Path,
    mode: Mode,
    rules: &[Rule],
    files: &[PathBuf],
) -> String {
    let mut prompt = format!(
        "You are FerrumChat, an expert coding agent working inside the user's project.\n\
         Project root: {}\nPlatform: {}\nToday: {}\n\
         Paths you pass to tools are relative to the project root; you cannot reach outside it.\n\
         Be concise. Use markdown; put code in fenced blocks with a language.\n\n{}\n",
        root.display(),
        std::env::consts::OS,
        chrono::Utc::now().format("%Y-%m-%d"),
        mode.instructions(),
    );
    let section = crate::rules::prompt_section(rules, files);
    if !section.is_empty() {
        prompt.push_str("\n# Project rules\n");
        prompt.push_str(&section);
    }
    prompt
}

impl NativeAgent {
    /// Runs one user turn (the user message is already in the history).
    pub async fn run_turn(
        &self,
        sink: &dyn TurnSink,
        mode: Mode,
        files: &[PathBuf],
        cancel: &CancellationToken,
    ) -> StopReason {
        let system = system_prompt(self.ctx.sandbox.root(), mode, &self.rules, files);
        for _ in 0..MAX_ROUNDS {
            if cancel.is_cancelled() {
                return StopReason::Cancelled;
            }
            let tool_defs = self.tools.for_mode(mode);
            let request = ChatRequest {
                model: self.model.clone(),
                system: system.clone(),
                messages: sink.history(),
                tools: tool_defs.into_iter().map(|t| t.spec).collect(),
                max_tokens: self.max_tokens,
            };
            sink.emit(ThreadEvent::AssistantStarted {
                id: sink.next_message_id(),
            });
            let (stop, calls) = match self.stream_round(sink, request, cancel).await {
                Ok(round) => round,
                Err(stop) => return stop,
            };
            if calls.is_empty() {
                return match stop {
                    StopReason::ToolUse => StopReason::EndTurn,
                    other => other,
                };
            }
            let mut denied = false;
            for call in calls {
                if cancel.is_cancelled() {
                    return StopReason::Cancelled;
                }
                match self.run_call(sink, mode, call, cancel).await {
                    CallResult::Done => {}
                    CallResult::Denied => denied = true,
                    CallResult::Cancelled => return StopReason::Cancelled,
                }
            }
            if denied {
                // Hand control back so the user can redirect.
                return StopReason::EndTurn;
            }
        }
        sink.emit(ThreadEvent::Error(format!(
            "Stopped after {MAX_ROUNDS} model requests. Send a message to continue."
        )));
        StopReason::MaxTokens
    }

    /// Streams one model request; the tool calls it made.
    async fn stream_round(
        &self,
        sink: &dyn TurnSink,
        request: ChatRequest,
        cancel: &CancellationToken,
    ) -> Result<(StopReason, Vec<ToolCallRecord>), StopReason> {
        let stream = tokio::select! {
            s = self.provider.stream(request) => s,
            _ = cancel.cancelled() => return Err(StopReason::Cancelled),
        };
        let mut stream = match stream {
            Ok(stream) => stream,
            Err(e) => {
                sink.emit(ThreadEvent::Error(e.to_string()));
                return Err(StopReason::Error(e.to_string()));
            }
        };
        let mut usage = Usage::default();
        let mut stop = StopReason::EndTurn;
        let mut calls = Vec::new();
        loop {
            let next = tokio::select! {
                n = stream.next() => n,
                _ = cancel.cancelled() => {
                    report_usage(sink, usage);
                    return Err(StopReason::Cancelled);
                }
            };
            match next {
                None => break,
                Some(Err(e)) => {
                    report_usage(sink, usage);
                    sink.emit(ThreadEvent::Error(e.to_string()));
                    return Err(StopReason::Error(e.to_string()));
                }
                Some(Ok(event)) => match event {
                    StreamEvent::TextDelta(text) => sink.emit(ThreadEvent::TextDelta(text)),
                    StreamEvent::ThinkingDelta(text) => sink.emit(ThreadEvent::ThinkingDelta(text)),
                    StreamEvent::ToolCall { id, name, input } => {
                        let record = ToolCallRecord {
                            title: tools::title(&name, &input),
                            access: self.tools.access(&name),
                            id,
                            name,
                            input,
                            status: ToolStatus::Pending,
                            output: None,
                            is_error: false,
                            changed: Vec::new(),
                            diffs: Vec::new(),
                        };
                        sink.emit(ThreadEvent::ToolCallStarted(record.clone()));
                        calls.push(record);
                    }
                    StreamEvent::Usage(u) => usage = u,
                    StreamEvent::Stop(reason) => stop = reason,
                },
            }
        }
        report_usage(sink, usage);
        if let StopReason::Error(message) = &stop {
            sink.emit(ThreadEvent::Error(message.clone()));
            return Err(stop);
        }
        Ok((stop, calls))
    }

    async fn run_call(
        &self,
        sink: &dyn TurnSink,
        mode: Mode,
        call: ToolCallRecord,
        cancel: &CancellationToken,
    ) -> CallResult {
        let finish = |status: ToolStatus, output: String, is_error: bool, changed: Vec<PathBuf>| {
            sink.emit(ThreadEvent::ToolCallUpdated {
                id: call.id.clone(),
                status: Some(status),
                title: None,
                output: Some(output),
                is_error: Some(is_error),
                changed,
                diffs: Vec::new(),
            });
        };
        let Some(access) = call.access else {
            finish(
                ToolStatus::Failed,
                format!("Unknown tool `{}`.", call.name),
                true,
                Vec::new(),
            );
            return CallResult::Done;
        };
        if !mode.allows(access) {
            finish(
                ToolStatus::Failed,
                format!("`{}` is not available in {} mode.", call.name, mode.label()),
                true,
                Vec::new(),
            );
            return CallResult::Done;
        }
        let policy = self
            .permissions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .policy(&call.name, access);
        let allowed = match policy {
            Policy::Allow => true,
            Policy::Deny => false,
            Policy::Ask => {
                sink.emit(ThreadEvent::tool_status(
                    &call.id,
                    ToolStatus::AwaitingPermission,
                ));
                let request = PermissionRequest {
                    id: 0,
                    tool_call_id: call.id.clone(),
                    tool: call.name.clone(),
                    title: call.title.clone(),
                    input: call.input.clone(),
                    access,
                };
                let decision = tokio::select! {
                    d = self.broker.ask(request) => d,
                    _ = cancel.cancelled() => return CallResult::Cancelled,
                };
                self.permissions
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .record(&call.name, decision);
                decision != Decision::Deny
            }
        };
        if !allowed {
            sink.emit(ThreadEvent::ToolCallUpdated {
                id: call.id.clone(),
                status: Some(ToolStatus::Denied),
                title: None,
                output: Some("Denied by the user.".into()),
                is_error: Some(true),
                changed: Vec::new(),
                diffs: Vec::new(),
            });
            return CallResult::Denied;
        }
        sink.emit(ThreadEvent::tool_status(&call.id, ToolStatus::Running));
        let output = tokio::select! {
            o = self.tools.call(&self.ctx, &call.name, call.input.clone()) => o,
            _ = cancel.cancelled() => return CallResult::Cancelled,
        };
        if call.name == "write_plan"
            && !output.is_error
            && let Some(markdown) = call.input["markdown"].as_str()
        {
            sink.emit(ThreadEvent::PlanWritten(markdown.to_string()));
        }
        let wrote = !output.changed.is_empty();
        finish(
            if output.is_error {
                ToolStatus::Failed
            } else {
                ToolStatus::Completed
            },
            output.text,
            output.is_error,
            output.changed,
        );
        if wrote {
            sink.changes_updated();
        }
        CallResult::Done
    }
}

enum CallResult {
    Done,
    Denied,
    Cancelled,
}

fn report_usage(sink: &dyn TurnSink, usage: Usage) {
    if usage != Usage::default() {
        sink.emit(ThreadEvent::Usage {
            usage,
            context_tokens: usage.input_tokens + usage.cache_read_tokens + usage.output_tokens,
        });
    }
}
