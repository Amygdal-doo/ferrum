//! A scripted provider for tests and demos: each request pops the next
//! scripted response and streams it event by event. Requests are recorded
//! so tests can check what the agent sent.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use futures::StreamExt;
use serde_json::Value;

use super::{
    ChatRequest, EventStream, ModelInfo, Provider, ProviderError, StopReason, StreamEvent, Usage,
};

#[derive(Clone, Default)]
pub struct MockProvider {
    script: Arc<Mutex<VecDeque<Vec<StreamEvent>>>>,
    requests: Arc<Mutex<Vec<ChatRequest>>>,
    /// Delay between streamed events (lets tests cancel mid-stream).
    pub delay: Option<std::time::Duration>,
}

impl MockProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues one response.
    pub fn push(&self, events: Vec<StreamEvent>) -> &Self {
        self.script
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(events);
        self
    }

    /// Queues a text answer ending the turn.
    pub fn push_text(&self, text: &str) -> &Self {
        self.push(vec![
            StreamEvent::TextDelta(text.into()),
            StreamEvent::Usage(Usage {
                input_tokens: 10,
                output_tokens: 5,
                cache_read_tokens: 0,
            }),
            StreamEvent::Stop(StopReason::EndTurn),
        ])
    }

    /// Queues a response calling one tool.
    pub fn push_tool(&self, id: &str, name: &str, input: Value) -> &Self {
        self.push(vec![
            StreamEvent::ToolCall {
                id: id.into(),
                name: name.into(),
                input,
            },
            StreamEvent::Usage(Usage {
                input_tokens: 10,
                output_tokens: 5,
                cache_read_tokens: 0,
            }),
            StreamEvent::Stop(StopReason::ToolUse),
        ])
    }

    pub fn requests(&self) -> Vec<ChatRequest> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn remaining(&self) -> usize {
        self.script
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

#[async_trait::async_trait]
impl Provider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(vec![ModelInfo {
            id: "mock-model".into(),
            name: "Mock".into(),
        }])
    }

    async fn stream(&self, request: ChatRequest) -> Result<EventStream, ProviderError> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request);
        let events = self
            .script
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .ok_or_else(|| ProviderError::Protocol("the mock script is empty".into()))?;
        let delay = self.delay;
        Ok(futures::stream::iter(events)
            .then(move |event| async move {
                if let Some(delay) = delay {
                    tokio::time::sleep(delay).await;
                }
                Ok(event)
            })
            .boxed())
    }
}
