//! Model providers behind one streaming trait. A provider turns a
//! [`ChatRequest`] (system prompt, history, tool specs) into a stream of
//! [`StreamEvent`]s: text deltas, complete tool calls, token usage and the
//! stop reason.

pub mod anthropic;
pub mod mock;
pub mod ollama;
pub mod openai;
pub mod ping;
pub mod sse;

use std::sync::Arc;

use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::secrets::{self, SecretStore};
use crate::settings::{ProviderKind, ProviderSettings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        id: String,
        content: String,
        is_error: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub parts: Vec<Part>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            parts: vec![Part::Text { text: text.into() }],
        }
    }
}

/// A tool the model may call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema of the input object.
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
}

impl Usage {
    pub fn add(&mut self, other: Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Cancelled,
    Refusal,
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    TextDelta(String),
    ThinkingDelta(String),
    /// A complete tool call (arguments fully streamed).
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
    /// Usage so far for this request (replaces earlier values).
    Usage(Usage),
    Stop(StopReason),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    #[error("no API key for {0}: add one in settings (stored in the keychain) or set {1}")]
    MissingKey(String, String),
    #[error("{0}")]
    Http(String),
    #[error("{status}: {message}")]
    Api { status: u16, message: String },
    #[error("unexpected response: {0}")]
    Protocol(String),
}

impl From<reqwest::Error> for ProviderError {
    fn from(e: reqwest::Error) -> Self {
        // Never include the request (headers hold the key).
        ProviderError::Http(e.without_url().to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

pub type EventStream = BoxStream<'static, Result<StreamEvent, ProviderError>>;

#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    /// Models the provider reports (network). Callers merge in configured
    /// ones.
    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError>;
    async fn stream(&self, request: ChatRequest) -> Result<EventStream, ProviderError>;
}

/// Builds the provider for `settings`, its key resolved from `store` or
/// the environment.
pub fn build(
    settings: &ProviderSettings,
    store: &dyn SecretStore,
) -> Result<Arc<dyn Provider>, ProviderError> {
    let key = if settings.needs_key() {
        let found = secrets::resolve(store, &settings.id, settings.env_key.as_deref());
        match found {
            Some((key, _)) => Some(key),
            // A custom endpoint may need no key (LM Studio, vLLM…).
            None if settings.custom => None,
            None => {
                return Err(ProviderError::MissingKey(
                    settings.name.clone(),
                    settings
                        .env_key
                        .clone()
                        .unwrap_or_else(|| "an env var".into()),
                ));
            }
        }
    } else {
        None
    };
    Ok(build_with_key(settings, key))
}

/// Builds the provider for `settings` with `key` (a form's key, before it
/// is saved).
pub fn build_with_key(
    settings: &ProviderSettings,
    key: Option<crate::secrets::ApiKey>,
) -> Arc<dyn Provider> {
    match settings.kind {
        ProviderKind::Anthropic => Arc::new(anthropic::Anthropic::new(
            &settings.id,
            &settings.base_url,
            key.unwrap_or_else(|| crate::secrets::ApiKey::new("")),
        )),
        ProviderKind::OpenAiChat => Arc::new(openai::OpenAi::new(
            &settings.id,
            &settings.base_url,
            key,
            openai::Api::Chat,
        )),
        ProviderKind::OpenAiResponses => Arc::new(openai::OpenAi::new(
            &settings.id,
            &settings.base_url,
            key,
            openai::Api::Responses,
        )),
        ProviderKind::Ollama => Arc::new(ollama::Ollama::new(&settings.id, &settings.base_url)),
    }
}

/// A shared HTTP client (rustls; no proxies read from the environment
/// are logged).
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .user_agent(concat!("FerrumChat/", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_default()
}

/// The error body of a failed response, trimmed.
pub(crate) async fn api_error(response: reqwest::Response) -> ProviderError {
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    let message = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.get("error"))
                .and_then(|m| m.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| text.chars().take(400).collect());
    ProviderError::Api { status, message }
}

/// Parses streamed tool arguments; empty means `{}`.
pub(crate) fn parse_args(raw: &str) -> Value {
    if raw.trim().is_empty() {
        return Value::Object(Default::default());
    }
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}
