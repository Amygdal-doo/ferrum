//! Anthropic Messages API: `POST /v1/messages` with `stream: true`, tool
//! use, and `GET /v1/models`.

use std::collections::BTreeMap;

use futures::StreamExt;
use serde_json::{Value, json};

use super::sse::{self, SseEvent};
use super::{
    ChatRequest, EventStream, ModelInfo, Part, Provider, ProviderError, Role, StopReason,
    StreamEvent, Usage, api_error, http_client, parse_args,
};
use crate::secrets::ApiKey;

const VERSION: &str = "2023-06-01";

pub struct Anthropic {
    id: String,
    base_url: String,
    key: ApiKey,
    http: reqwest::Client,
}

impl Anthropic {
    pub fn new(id: &str, base_url: &str, key: ApiKey) -> Self {
        Self {
            id: id.into(),
            base_url: base_url.trim_end_matches('/').into(),
            key,
            http: http_client(),
        }
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.http
            .post(format!("{}{path}", self.base_url))
            .header("x-api-key", self.key.expose())
            .header("anthropic-version", VERSION)
    }
}

/// The request JSON for `request`.
pub fn request_body(request: &ChatRequest) -> Value {
    let messages: Vec<Value> = request
        .messages
        .iter()
        .filter(|m| !m.parts.is_empty())
        .map(|m| {
            let content: Vec<Value> = m
                .parts
                .iter()
                .map(|part| match part {
                    Part::Text { text } => json!({"type": "text", "text": text}),
                    Part::ToolUse { id, name, input } => {
                        json!({"type": "tool_use", "id": id, "name": name, "input": input})
                    }
                    Part::ToolResult {
                        id,
                        content,
                        is_error,
                    } => json!({
                        "type": "tool_result",
                        "tool_use_id": id,
                        "content": content,
                        "is_error": is_error,
                    }),
                })
                .collect();
            let role = match m.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            json!({"role": role, "content": content})
        })
        .collect();
    let mut body = json!({
        "model": request.model,
        "max_tokens": request.max_tokens,
        "messages": messages,
        "stream": true,
    });
    if !request.system.is_empty() {
        body["system"] = json!(request.system);
    }
    if !request.tools.is_empty() {
        body["tools"] = request
            .tools
            .iter()
            .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.input_schema}))
            .collect();
    }
    body
}

/// Turns Messages API stream events into [`StreamEvent`]s.
#[derive(Debug, Default)]
pub struct StreamState {
    usage: Usage,
    /// Tool-use blocks being streamed, by index: (id, name, json so far).
    tools: BTreeMap<u64, (String, String, String)>,
    stop: Option<StopReason>,
}

impl StreamState {
    pub fn on_event(&mut self, event: &SseEvent) -> Result<Vec<StreamEvent>, ProviderError> {
        if event.data.is_empty() {
            return Ok(Vec::new());
        }
        let data: Value = serde_json::from_str(&event.data)
            .map_err(|e| ProviderError::Protocol(format!("bad event: {e}")))?;
        let kind = data["type"].as_str().unwrap_or_default();
        let mut out = Vec::new();
        match kind {
            "message_start" => {
                let usage = &data["message"]["usage"];
                self.usage.input_tokens = usage["input_tokens"].as_u64().unwrap_or(0)
                    + usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
                self.usage.cache_read_tokens =
                    usage["cache_read_input_tokens"].as_u64().unwrap_or(0);
                self.usage.output_tokens = usage["output_tokens"].as_u64().unwrap_or(0);
                out.push(StreamEvent::Usage(self.usage));
            }
            "content_block_start" => {
                let block = &data["content_block"];
                if block["type"] == "tool_use" {
                    let index = data["index"].as_u64().unwrap_or(0);
                    self.tools.insert(
                        index,
                        (
                            block["id"].as_str().unwrap_or_default().to_string(),
                            block["name"].as_str().unwrap_or_default().to_string(),
                            String::new(),
                        ),
                    );
                }
            }
            "content_block_delta" => {
                let delta = &data["delta"];
                match delta["type"].as_str().unwrap_or_default() {
                    "text_delta" => out.push(StreamEvent::TextDelta(
                        delta["text"].as_str().unwrap_or_default().to_string(),
                    )),
                    "thinking_delta" => out.push(StreamEvent::ThinkingDelta(
                        delta["thinking"].as_str().unwrap_or_default().to_string(),
                    )),
                    "input_json_delta" => {
                        let index = data["index"].as_u64().unwrap_or(0);
                        if let Some(tool) = self.tools.get_mut(&index) {
                            tool.2
                                .push_str(delta["partial_json"].as_str().unwrap_or_default());
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = data["index"].as_u64().unwrap_or(0);
                if let Some((id, name, raw)) = self.tools.remove(&index) {
                    out.push(StreamEvent::ToolCall {
                        id,
                        name,
                        input: parse_args(&raw),
                    });
                }
            }
            "message_delta" => {
                if let Some(output) = data["usage"]["output_tokens"].as_u64() {
                    self.usage.output_tokens = output;
                    out.push(StreamEvent::Usage(self.usage));
                }
                if let Some(reason) = data["delta"]["stop_reason"].as_str() {
                    self.stop = Some(match reason {
                        "tool_use" => StopReason::ToolUse,
                        "max_tokens" => StopReason::MaxTokens,
                        "refusal" => StopReason::Refusal,
                        _ => StopReason::EndTurn,
                    });
                }
            }
            "message_stop" => {
                out.push(StreamEvent::Stop(
                    self.stop.take().unwrap_or(StopReason::EndTurn),
                ));
            }
            "error" => {
                return Err(ProviderError::Api {
                    status: 0,
                    message: data["error"]["message"]
                        .as_str()
                        .unwrap_or("stream error")
                        .to_string(),
                });
            }
            _ => {}
        }
        Ok(out)
    }
}

#[async_trait::async_trait]
impl Provider for Anthropic {
    fn id(&self) -> &str {
        &self.id
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let response = self
            .http
            .get(format!("{}/v1/models?limit=100", self.base_url))
            .header("x-api-key", self.key.expose())
            .header("anthropic-version", VERSION)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(api_error(response).await);
        }
        let body: Value = response.json().await?;
        Ok(body["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| {
                let id = m["id"].as_str()?.to_string();
                let name = m["display_name"].as_str().unwrap_or(&id).to_string();
                Some(ModelInfo { id, name })
            })
            .collect())
    }

    async fn stream(&self, request: ChatRequest) -> Result<EventStream, ProviderError> {
        let response = self
            .post("/v1/messages")
            .json(&request_body(&request))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(api_error(response).await);
        }
        let mut state = StreamState::default();
        let events = sse::events(response.bytes_stream().boxed()).flat_map(move |event| {
            let items: Vec<Result<StreamEvent, ProviderError>> = match event {
                Ok(event) => match state.on_event(&event) {
                    Ok(items) => items.into_iter().map(Ok).collect(),
                    Err(e) => vec![Err(e)],
                },
                Err(e) => vec![Err(ProviderError::from(e))],
            };
            futures::stream::iter(items)
        });
        Ok(events.boxed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::sse::SseDecoder;
    use crate::provider::{Message, ToolSpec};

    #[test]
    fn body_has_tools_system_and_tool_results() {
        let request = ChatRequest {
            model: "claude-sonnet-5-5".into(),
            system: "be brief".into(),
            messages: vec![
                Message::user("hi"),
                Message {
                    role: Role::Assistant,
                    parts: vec![Part::ToolUse {
                        id: "t1".into(),
                        name: "read_file".into(),
                        input: json!({"path": "a"}),
                    }],
                },
                Message {
                    role: Role::User,
                    parts: vec![Part::ToolResult {
                        id: "t1".into(),
                        content: "x".into(),
                        is_error: false,
                    }],
                },
            ],
            tools: vec![ToolSpec {
                name: "read_file".into(),
                description: "d".into(),
                input_schema: json!({"type": "object"}),
            }],
            max_tokens: 100,
        };
        let body = request_body(&request);
        assert_eq!(body["system"], "be brief");
        assert_eq!(body["stream"], true);
        assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
        assert_eq!(body["messages"][2]["content"][0]["tool_use_id"], "t1");
        assert_eq!(body["messages"][1]["role"], "assistant");
    }

    #[test]
    fn parses_a_recorded_stream_with_text_and_a_tool_call() {
        let raw = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":12,\"output_tokens\":1}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Let me look.\"}}\n\n",
            "event: ping\ndata: {\"type\":\"ping\"}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"read_file\",\"input\":{}}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\": \\\"src/\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"main.rs\\\"}\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":40}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        );
        let mut decoder = SseDecoder::default();
        let mut state = StreamState::default();
        let events: Vec<StreamEvent> = decoder
            .feed(raw.as_bytes())
            .iter()
            .flat_map(|e| state.on_event(e).expect("event"))
            .collect();
        assert_eq!(
            events,
            [
                StreamEvent::Usage(Usage {
                    input_tokens: 12,
                    output_tokens: 1,
                    cache_read_tokens: 0
                }),
                StreamEvent::TextDelta("Let me look.".into()),
                StreamEvent::ToolCall {
                    id: "toolu_1".into(),
                    name: "read_file".into(),
                    input: json!({"path": "src/main.rs"}),
                },
                StreamEvent::Usage(Usage {
                    input_tokens: 12,
                    output_tokens: 40,
                    cache_read_tokens: 0
                }),
                StreamEvent::Stop(StopReason::ToolUse),
            ]
        );
        let error = SseEvent {
            event: Some("error".into()),
            data: r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#
                .into(),
        };
        assert!(matches!(
            state.on_event(&error),
            Err(ProviderError::Api { message, .. }) if message == "Overloaded"
        ));
    }
}
