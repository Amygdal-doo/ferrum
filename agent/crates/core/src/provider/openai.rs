//! OpenAI-compatible providers: Chat Completions (`/chat/completions`,
//! also OpenRouter, vLLM, LM Studio, Ollama…) and Responses
//! (`/responses`), streaming, with function tools. `base_url` includes the
//! version path (`https://api.openai.com/v1`).

use std::collections::BTreeMap;

use futures::StreamExt;
use serde_json::{Value, json};

use super::sse::{self, SseEvent};
use super::{
    ChatRequest, EventStream, ModelInfo, Part, Provider, ProviderError, Role, StopReason,
    StreamEvent, Usage, api_error, http_client, parse_args,
};
use crate::secrets::ApiKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Api {
    Chat,
    Responses,
}

pub struct OpenAi {
    id: String,
    base_url: String,
    key: Option<ApiKey>,
    api: Api,
    http: reqwest::Client,
}

impl OpenAi {
    pub fn new(id: &str, base_url: &str, key: Option<ApiKey>, api: Api) -> Self {
        Self {
            id: id.into(),
            base_url: base_url.trim_end_matches('/').into(),
            key,
            api,
            http: http_client(),
        }
    }

    fn auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.key {
            Some(key) if !key.expose().is_empty() => builder.bearer_auth(key.expose()),
            _ => builder,
        }
    }
}

/// Chat Completions request JSON.
pub fn chat_body(request: &ChatRequest) -> Value {
    let mut messages = Vec::new();
    if !request.system.is_empty() {
        messages.push(json!({"role": "system", "content": request.system}));
    }
    for message in &request.messages {
        match message.role {
            Role::Assistant => {
                let text: String = message
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                let calls: Vec<Value> = message
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::ToolUse { id, name, input } => Some(json!({
                            "id": id,
                            "type": "function",
                            "function": {"name": name, "arguments": input.to_string()},
                        })),
                        _ => None,
                    })
                    .collect();
                let mut m = json!({"role": "assistant", "content": text});
                if !calls.is_empty() {
                    m["tool_calls"] = Value::Array(calls);
                }
                messages.push(m);
            }
            Role::User => {
                let mut text = String::new();
                for part in &message.parts {
                    match part {
                        Part::ToolResult { id, content, .. } => messages.push(json!({
                            "role": "tool",
                            "tool_call_id": id,
                            "content": content,
                        })),
                        Part::Text { text: t } => text.push_str(t),
                        Part::ToolUse { .. } => {}
                    }
                }
                if !text.is_empty() {
                    messages.push(json!({"role": "user", "content": text}));
                }
            }
        }
    }
    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "stream": true,
        "stream_options": {"include_usage": true},
        "max_completion_tokens": request.max_tokens,
    });
    if !request.tools.is_empty() {
        body["tools"] = request
            .tools
            .iter()
            .map(|t| json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.input_schema}}))
            .collect();
    }
    body
}

/// Responses API request JSON.
pub fn responses_body(request: &ChatRequest) -> Value {
    let mut input = Vec::new();
    for message in &request.messages {
        for part in &message.parts {
            match (message.role, part) {
                (Role::User, Part::Text { text }) => input.push(json!({
                    "role": "user",
                    "content": [{"type": "input_text", "text": text}],
                })),
                (Role::Assistant, Part::Text { text }) => input.push(json!({
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": text}],
                })),
                (
                    _,
                    Part::ToolUse {
                        id,
                        name,
                        input: args,
                    },
                ) => input.push(json!({
                    "type": "function_call",
                    "call_id": id,
                    "name": name,
                    "arguments": args.to_string(),
                })),
                (_, Part::ToolResult { id, content, .. }) => input.push(json!({
                    "type": "function_call_output",
                    "call_id": id,
                    "output": content,
                })),
            }
        }
    }
    let mut body = json!({
        "model": request.model,
        "input": input,
        "stream": true,
        "store": false,
        "max_output_tokens": request.max_tokens,
    });
    if !request.system.is_empty() {
        body["instructions"] = json!(request.system);
    }
    if !request.tools.is_empty() {
        body["tools"] = request
            .tools
            .iter()
            .map(|t| json!({"type": "function", "name": t.name, "description": t.description, "parameters": t.input_schema}))
            .collect();
    }
    body
}

/// Chat Completions chunks → [`StreamEvent`]s.
#[derive(Debug, Default)]
pub struct ChatState {
    /// Tool calls by index: (id, name, arguments so far).
    tools: BTreeMap<u64, (String, String, String)>,
    finish: Option<StopReason>,
    stopped: bool,
}

impl ChatState {
    pub fn on_event(&mut self, event: &SseEvent) -> Result<Vec<StreamEvent>, ProviderError> {
        let mut out = Vec::new();
        if event.data.trim() == "[DONE]" {
            out.extend(self.finish_up());
            return Ok(out);
        }
        if event.data.trim().is_empty() {
            return Ok(out);
        }
        let data: Value = serde_json::from_str(&event.data)
            .map_err(|e| ProviderError::Protocol(format!("bad chunk: {e}")))?;
        if let Some(message) = data.pointer("/error/message").and_then(Value::as_str) {
            return Err(ProviderError::Api {
                status: 0,
                message: message.into(),
            });
        }
        for choice in data["choices"].as_array().into_iter().flatten() {
            let delta = &choice["delta"];
            if let Some(text) = delta["content"].as_str().filter(|t| !t.is_empty()) {
                out.push(StreamEvent::TextDelta(text.into()));
            }
            if let Some(text) = delta["reasoning_content"]
                .as_str()
                .or(delta["reasoning"].as_str())
                .filter(|t| !t.is_empty())
            {
                out.push(StreamEvent::ThinkingDelta(text.into()));
            }
            for call in delta["tool_calls"].as_array().into_iter().flatten() {
                let index = call["index"].as_u64().unwrap_or(0);
                let entry = self.tools.entry(index).or_default();
                if let Some(id) = call["id"].as_str() {
                    entry.0 = id.into();
                }
                if let Some(name) = call["function"]["name"].as_str() {
                    entry.1.push_str(name);
                }
                if let Some(args) = call["function"]["arguments"].as_str() {
                    entry.2.push_str(args);
                }
            }
            if let Some(reason) = choice["finish_reason"].as_str() {
                self.finish = Some(match reason {
                    "tool_calls" | "function_call" => StopReason::ToolUse,
                    "length" => StopReason::MaxTokens,
                    "content_filter" => StopReason::Refusal,
                    _ => StopReason::EndTurn,
                });
            }
        }
        if data["usage"].is_object() {
            out.push(StreamEvent::Usage(Usage {
                input_tokens: data["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
                output_tokens: data["usage"]["completion_tokens"].as_u64().unwrap_or(0),
                cache_read_tokens: data
                    .pointer("/usage/prompt_tokens_details/cached_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            }));
        }
        Ok(out)
    }

    /// Tool calls and the stop at the end of the stream (`[DONE]` or EOF).
    pub fn finish_up(&mut self) -> Vec<StreamEvent> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        let mut out: Vec<StreamEvent> = std::mem::take(&mut self.tools)
            .into_values()
            .map(|(id, name, raw)| StreamEvent::ToolCall {
                id,
                name,
                input: parse_args(&raw),
            })
            .collect();
        let had_tools = !out.is_empty();
        let stop = match self.finish.take() {
            Some(StopReason::EndTurn) | None if had_tools => StopReason::ToolUse,
            Some(reason) => reason,
            None => StopReason::EndTurn,
        };
        out.push(StreamEvent::Stop(stop));
        out
    }
}

/// Responses API events → [`StreamEvent`]s.
#[derive(Debug, Default)]
pub struct ResponsesState {
    saw_tool: bool,
    stopped: bool,
}

impl ResponsesState {
    pub fn on_event(&mut self, event: &SseEvent) -> Result<Vec<StreamEvent>, ProviderError> {
        if event.data.trim().is_empty() || event.data.trim() == "[DONE]" {
            return Ok(Vec::new());
        }
        let data: Value = serde_json::from_str(&event.data)
            .map_err(|e| ProviderError::Protocol(format!("bad event: {e}")))?;
        let kind = data["type"]
            .as_str()
            .or(event.event.as_deref())
            .unwrap_or_default();
        let mut out = Vec::new();
        match kind {
            "response.output_text.delta" => {
                out.push(StreamEvent::TextDelta(
                    data["delta"].as_str().unwrap_or_default().into(),
                ));
            }
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                out.push(StreamEvent::ThinkingDelta(
                    data["delta"].as_str().unwrap_or_default().into(),
                ));
            }
            "response.output_item.done" if data["item"]["type"] == "function_call" => {
                let item = &data["item"];
                self.saw_tool = true;
                out.push(StreamEvent::ToolCall {
                    id: item["call_id"].as_str().unwrap_or_default().into(),
                    name: item["name"].as_str().unwrap_or_default().into(),
                    input: parse_args(item["arguments"].as_str().unwrap_or_default()),
                });
            }
            "response.completed" | "response.incomplete" => {
                let response = &data["response"];
                let usage = &response["usage"];
                out.push(StreamEvent::Usage(Usage {
                    input_tokens: usage["input_tokens"].as_u64().unwrap_or(0),
                    output_tokens: usage["output_tokens"].as_u64().unwrap_or(0),
                    cache_read_tokens: usage
                        .pointer("/input_tokens_details/cached_tokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                }));
                let stop = if kind == "response.incomplete" {
                    StopReason::MaxTokens
                } else if self.saw_tool {
                    StopReason::ToolUse
                } else {
                    StopReason::EndTurn
                };
                self.stopped = true;
                out.push(StreamEvent::Stop(stop));
            }
            "response.failed" | "error" => {
                let message = data
                    .pointer("/response/error/message")
                    .or_else(|| data.pointer("/error/message"))
                    .or_else(|| data.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("response failed");
                return Err(ProviderError::Api {
                    status: 0,
                    message: message.into(),
                });
            }
            _ => {}
        }
        Ok(out)
    }
}

#[async_trait::async_trait]
impl Provider for OpenAi {
    fn id(&self) -> &str {
        &self.id
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let response = self
            .auth(self.http.get(format!("{}/models", self.base_url)))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(api_error(response).await);
        }
        let body: Value = response.json().await?;
        let mut models: Vec<ModelInfo> = body["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| {
                let id = m["id"].as_str()?.to_string();
                Some(ModelInfo {
                    name: id.clone(),
                    id,
                })
            })
            .collect();
        models.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(models)
    }

    async fn stream(&self, request: ChatRequest) -> Result<EventStream, ProviderError> {
        let (path, body) = match self.api {
            Api::Chat => ("chat/completions", chat_body(&request)),
            Api::Responses => ("responses", responses_body(&request)),
        };
        let response = self
            .auth(self.http.post(format!("{}/{path}", self.base_url)))
            .json(&body)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(api_error(response).await);
        }
        let body = sse::events(response.bytes_stream().boxed());
        Ok(match self.api {
            Api::Chat => {
                let mut state = ChatState::default();
                body.map(Some)
                    .chain(futures::stream::once(async { None }))
                    .flat_map(move |event| {
                        let items: Vec<Result<StreamEvent, ProviderError>> = match event {
                            Some(Ok(event)) => match state.on_event(&event) {
                                Ok(items) => items.into_iter().map(Ok).collect(),
                                Err(e) => vec![Err(e)],
                            },
                            Some(Err(e)) => vec![Err(ProviderError::from(e))],
                            None => state.finish_up().into_iter().map(Ok).collect(),
                        };
                        futures::stream::iter(items)
                    })
                    .boxed()
            }
            Api::Responses => {
                let mut state = ResponsesState::default();
                body.map(Some)
                    .chain(futures::stream::once(async { None }))
                    .flat_map(move |event| {
                        let items: Vec<Result<StreamEvent, ProviderError>> = match event {
                            Some(Ok(event)) => match state.on_event(&event) {
                                Ok(items) => items.into_iter().map(Ok).collect(),
                                Err(e) => vec![Err(e)],
                            },
                            Some(Err(e)) => vec![Err(ProviderError::from(e))],
                            None if !state.stopped => {
                                vec![Ok(StreamEvent::Stop(StopReason::Error(
                                    "the stream ended early".into(),
                                )))]
                            }
                            None => Vec::new(),
                        };
                        futures::stream::iter(items)
                    })
                    .boxed()
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Message;
    use crate::provider::sse::SseDecoder;

    fn run<F: FnMut(&SseEvent) -> Vec<StreamEvent>>(raw: &str, mut f: F) -> Vec<StreamEvent> {
        SseDecoder::default()
            .feed(raw.as_bytes())
            .iter()
            .flat_map(&mut f)
            .collect()
    }

    #[test]
    fn chat_completions_stream_with_split_tool_arguments() {
        let raw = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"grep\",\"arguments\":\"{\\\"pat\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"tern\\\":\\\"x\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":7}}\n\n",
            "data: [DONE]\n\n",
        );
        let mut state = ChatState::default();
        let events = run(raw, |e| state.on_event(e).expect("chunk"));
        assert_eq!(
            events,
            [
                StreamEvent::TextDelta("Hi".into()),
                StreamEvent::Usage(Usage {
                    input_tokens: 5,
                    output_tokens: 7,
                    cache_read_tokens: 0
                }),
                StreamEvent::ToolCall {
                    id: "c1".into(),
                    name: "grep".into(),
                    input: json!({"pattern": "x"})
                },
                StreamEvent::Stop(StopReason::ToolUse),
            ]
        );
        assert!(state.finish_up().is_empty(), "stops once");
    }

    #[test]
    fn responses_stream_and_bodies() {
        let raw = concat!(
            "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
            "event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"call_id\":\"call_9\",\"name\":\"list_dir\",\"arguments\":\"{}\"}}\n\n",
            "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":3,\"output_tokens\":4}}}\n\n",
        );
        let mut state = ResponsesState::default();
        let events = run(raw, |e| state.on_event(e).expect("event"));
        assert_eq!(events.len(), 4);
        assert_eq!(events[3], StreamEvent::Stop(StopReason::ToolUse));
        assert!(matches!(&events[1], StreamEvent::ToolCall { id, .. } if id == "call_9"));

        let request = ChatRequest {
            model: "gpt-5".into(),
            system: "sys".into(),
            messages: vec![
                Message::user("q"),
                Message {
                    role: Role::Assistant,
                    parts: vec![Part::ToolUse {
                        id: "c".into(),
                        name: "n".into(),
                        input: json!({}),
                    }],
                },
                Message {
                    role: Role::User,
                    parts: vec![Part::ToolResult {
                        id: "c".into(),
                        content: "out".into(),
                        is_error: false,
                    }],
                },
            ],
            tools: vec![],
            max_tokens: 10,
        };
        let r = responses_body(&request);
        assert_eq!(r["instructions"], "sys");
        assert_eq!(r["input"][1]["type"], "function_call");
        assert_eq!(r["input"][2]["output"], "out");
        let c = chat_body(&request);
        assert_eq!(c["messages"][0]["role"], "system");
        assert_eq!(
            c["messages"][2]["tool_calls"][0]["function"]["arguments"],
            "{}"
        );
        assert_eq!(c["messages"][3]["role"], "tool");
    }
}
