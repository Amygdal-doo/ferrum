//! Provider and model settings (`~/.ferrum/agent/settings.json`). Keys are
//! not here: see [`crate::secrets`].

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The wire protocol a provider speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Anthropic,
    /// OpenAI Chat Completions (`/chat/completions`): OpenAI, OpenRouter,
    /// vLLM, LM Studio, Azure-compatible gateways…
    OpenAiChat,
    /// OpenAI Responses (`/responses`).
    OpenAiResponses,
    /// Ollama's OpenAI-compatible endpoint, models from `/api/tags`.
    Ollama,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSettings {
    /// Also the keychain account for its key.
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    /// Environment variable consulted when the keychain has no key.
    #[serde(default)]
    pub env_key: Option<String>,
    /// Models offered in the selector (user-editable; live listings are
    /// merged in when the provider answers).
    #[serde(default)]
    pub models: Vec<String>,
    pub default_model: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Max output tokens per request.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn yes() -> bool {
    true
}

fn default_max_tokens() -> u32 {
    16_000
}

pub const ANTHROPIC_MODELS: [&str; 3] =
    ["claude-sonnet-5-5", "claude-opus-5-5", "claude-haiku-5-5"];

impl ProviderSettings {
    pub fn anthropic() -> Self {
        Self {
            id: "anthropic".into(),
            name: "Anthropic".into(),
            kind: ProviderKind::Anthropic,
            base_url: "https://api.anthropic.com".into(),
            env_key: Some("ANTHROPIC_API_KEY".into()),
            models: ANTHROPIC_MODELS.iter().map(|m| m.to_string()).collect(),
            default_model: ANTHROPIC_MODELS[0].into(),
            enabled: true,
            max_tokens: default_max_tokens(),
        }
    }

    pub fn openai() -> Self {
        Self {
            id: "openai".into(),
            name: "OpenAI".into(),
            kind: ProviderKind::OpenAiResponses,
            base_url: "https://api.openai.com/v1".into(),
            env_key: Some("OPENAI_API_KEY".into()),
            models: vec!["gpt-5".into(), "gpt-5-mini".into()],
            default_model: "gpt-5".into(),
            enabled: true,
            max_tokens: default_max_tokens(),
        }
    }

    pub fn ollama() -> Self {
        Self {
            id: "ollama".into(),
            name: "Ollama (local)".into(),
            kind: ProviderKind::Ollama,
            base_url: "http://localhost:11434".into(),
            env_key: None,
            models: vec!["qwen2.5-coder".into(), "llama3.2".into()],
            default_model: "qwen2.5-coder".into(),
            enabled: true,
            max_tokens: 8_192,
        }
    }

    pub fn needs_key(&self) -> bool {
        self.kind != ProviderKind::Ollama
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSettings {
    pub providers: Vec<ProviderSettings>,
    /// `provider_id/model` or `acp:<agent id>`.
    #[serde(default)]
    pub default_agent: Option<String>,
    /// What runs without asking (the composer's approval menu).
    #[serde(default)]
    pub approval: crate::permissions::Approval,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            providers: vec![
                ProviderSettings::anthropic(),
                ProviderSettings::openai(),
                ProviderSettings::ollama(),
            ],
            default_agent: None,
            approval: crate::permissions::Approval::default(),
        }
    }
}

impl AgentSettings {
    /// The saved settings, or the defaults when missing or unreadable.
    pub fn load(file: &Path) -> Self {
        std::fs::read_to_string(file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        crate::fsutil::write_atomic(file, &json)
    }

    pub fn provider(&self, id: &str) -> Option<&ProviderSettings> {
        self.providers.iter().find(|p| p.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_without_secrets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("settings.json");
        assert_eq!(AgentSettings::load(&file), AgentSettings::default());
        let mut settings = AgentSettings::default();
        settings.providers[0].models.push("claude-custom".into());
        settings.save(&file).expect("save");
        let text = std::fs::read_to_string(&file).expect("read");
        assert!(!text.to_lowercase().contains("api_key\":"), "{text}");
        assert_eq!(AgentSettings::load(&file), settings);
        let anthropic = settings.provider("anthropic").expect("anthropic");
        assert_eq!(anthropic.default_model, "claude-sonnet-5-5");
        assert!(anthropic.models.contains(&"claude-opus-5-5".to_string()));
        assert!(!settings.provider("ollama").expect("ollama").needs_key());
        // Files from before the approval menu still load.
        let old: AgentSettings =
            serde_json::from_str(r#"{"providers": []}"#).expect("old settings");
        assert_eq!(old.approval, crate::permissions::Approval::Ask);
    }
}
