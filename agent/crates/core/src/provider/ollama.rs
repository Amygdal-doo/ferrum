//! Ollama (local): chat through its OpenAI-compatible `/v1` endpoint,
//! models from `/api/tags`. No key.

use serde_json::Value;

use super::openai::{Api, OpenAi};
use super::{ChatRequest, EventStream, ModelInfo, Provider, ProviderError, api_error, http_client};

pub struct Ollama {
    id: String,
    base_url: String,
    chat: OpenAi,
    http: reqwest::Client,
}

impl Ollama {
    /// `base_url` is the server root, e.g. `http://localhost:11434`.
    pub fn new(id: &str, base_url: &str) -> Self {
        let base = base_url.trim_end_matches('/').trim_end_matches("/v1");
        Self {
            id: id.into(),
            base_url: base.into(),
            chat: OpenAi::new(id, &format!("{base}/v1"), None, Api::Chat),
            http: http_client(),
        }
    }
}

/// Model names from an `/api/tags` answer.
pub fn parse_tags(body: &Value) -> Vec<ModelInfo> {
    body["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = m["name"].as_str().or(m["model"].as_str())?.to_string();
            Some(ModelInfo {
                name: id.clone(),
                id,
            })
        })
        .collect()
}

#[async_trait::async_trait]
impl Provider for Ollama {
    fn id(&self) -> &str {
        &self.id
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let response = self
            .http
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(api_error(response).await);
        }
        Ok(parse_tags(&response.json().await?))
    }

    async fn stream(&self, request: ChatRequest) -> Result<EventStream, ProviderError> {
        self.chat.stream(request).await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tags_list_models() {
        let body =
            serde_json::json!({"models": [{"name": "qwen2.5-coder:7b"}, {"model": "llama3.2"}]});
        let ids: Vec<String> = super::parse_tags(&body).into_iter().map(|m| m.id).collect();
        assert_eq!(ids, ["qwen2.5-coder:7b", "llama3.2"]);
    }
}
