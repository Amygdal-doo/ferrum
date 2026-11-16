//! Models the user adds in "Models and API keys": an OpenAI-compatible or
//! Anthropic-compatible endpoint, a model id and a display name. Each one
//! is a provider of its own in `settings.json` (marked `custom`); its key
//! goes to the keychain under the provider's id, never into the file.
//!
//! Also the FerrumAI placeholder: listed first in every model selector,
//! but disabled until Ferrum's own model service exists — nothing is ever
//! sent to it.

use std::collections::BTreeMap;

use crate::settings::{AgentSettings, ProviderKind, ProviderSettings};

/// The placeholder's provider id (`ferrum/ferrum-ai`); no such provider is
/// ever built.
pub const FERRUM_AI_PROVIDER: &str = "ferrum";
pub const FERRUM_AI_MODEL: &str = "ferrum-ai";
pub const FERRUM_AI_LABEL: &str = "FerrumAI";
pub const COMING_SOON: &str = "Coming soon";

/// Whether `provider` is the FerrumAI placeholder.
pub fn is_ferrum_ai(provider: &str) -> bool {
    provider == FERRUM_AI_PROVIDER
}

/// The wire protocol of a custom endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomKind {
    /// `/chat/completions`: OpenRouter, Gemini, DeepSeek, Groq, vLLM, LM
    /// Studio…
    OpenAi,
    /// `/v1/messages`: Anthropic-compatible gateways.
    Anthropic,
}

impl CustomKind {
    pub const ALL: [CustomKind; 2] = [CustomKind::OpenAi, CustomKind::Anthropic];

    pub fn label(self) -> &'static str {
        match self {
            CustomKind::OpenAi => "OpenAI-compatible",
            CustomKind::Anthropic => "Anthropic-compatible",
        }
    }

    pub fn provider_kind(self) -> ProviderKind {
        match self {
            CustomKind::OpenAi => ProviderKind::OpenAiChat,
            CustomKind::Anthropic => ProviderKind::Anthropic,
        }
    }

    pub fn placeholder_url(self) -> &'static str {
        match self {
            CustomKind::OpenAi => "https://api.example.com/v1",
            CustomKind::Anthropic => "https://api.example.com",
        }
    }
}

/// Known endpoints that fill the form's base URL (the model id stays the
/// user's: providers rename them often).
pub const PRESETS: [(&str, CustomKind, &str); 7] = [
    (
        "OpenRouter",
        CustomKind::OpenAi,
        "https://openrouter.ai/api/v1",
    ),
    (
        "Gemini",
        CustomKind::OpenAi,
        "https://generativelanguage.googleapis.com/v1beta/openai",
    ),
    (
        "DeepSeek",
        CustomKind::OpenAi,
        "https://api.deepseek.com/v1",
    ),
    ("Mistral", CustomKind::OpenAi, "https://api.mistral.ai/v1"),
    ("Groq", CustomKind::OpenAi, "https://api.groq.com/openai/v1"),
    ("xAI", CustomKind::OpenAi, "https://api.x.ai/v1"),
    ("LM Studio", CustomKind::OpenAi, "http://localhost:1234/v1"),
];

/// What the "Add a custom model" form holds (the key travels separately).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomModel {
    pub kind: CustomKind,
    /// Shown in the selector; the model id when empty.
    pub name: String,
    pub base_url: String,
    pub model: String,
}

impl CustomModel {
    /// The provider it becomes (not yet added: its id is the slug of its
    /// name). Errors say what to fix.
    pub fn to_provider(&self) -> Result<ProviderSettings, String> {
        let model = self.model.trim();
        if model.is_empty() {
            return Err("Enter the model id (e.g. gpt-4o or deepseek-chat).".into());
        }
        let url = self.base_url.trim().trim_end_matches('/');
        if !(url.starts_with("https://") || url.starts_with("http://")) || url.len() < 10 {
            return Err("Enter the base URL, starting with https:// or http://.".into());
        }
        let name = match self.name.trim() {
            "" => model.to_string(),
            name => name.to_string(),
        };
        let mut labels = BTreeMap::new();
        if name != model {
            labels.insert(model.to_string(), name.clone());
        }
        Ok(ProviderSettings {
            id: format!("custom-{}", slug(&name)),
            name,
            kind: self.kind.provider_kind(),
            base_url: url.to_string(),
            env_key: None,
            models: vec![model.to_string()],
            default_model: model.to_string(),
            enabled: true,
            max_tokens: 8_192,
            custom: true,
            labels,
        })
    }
}

/// `My Gateway (EU)` → `my-gateway-eu`.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').chars().take(40).collect::<String>();
    if out.is_empty() { "model".into() } else { out }
}

impl AgentSettings {
    /// Adds `model` as a custom provider and returns its id (unique: `-2`,
    /// `-3`… when the name is taken).
    pub fn add_custom(&mut self, model: &CustomModel) -> Result<String, String> {
        let mut provider = model.to_provider()?;
        let base = provider.id.clone();
        let mut n = 2;
        while self.provider(&provider.id).is_some() {
            provider.id = format!("{base}-{n}");
            n += 1;
        }
        let id = provider.id.clone();
        self.providers.push(provider);
        Ok(id)
    }

    /// Removes a custom provider; the built-in ones stay. True if removed.
    pub fn remove_custom(&mut self, id: &str) -> bool {
        let before = self.providers.len();
        self.providers.retain(|p| !(p.custom && p.id == id));
        if self.providers.len() == before {
            return false;
        }
        if self
            .default_agent
            .as_deref()
            .is_some_and(|a| a.split_once('/').is_some_and(|(p, _)| p == id))
        {
            self.default_agent = None;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gateway() -> CustomModel {
        CustomModel {
            kind: CustomKind::OpenAi,
            name: "My Gateway (EU)".into(),
            base_url: "https://gw.example.com/v1/".into(),
            model: "deepseek-chat".into(),
        }
    }

    #[test]
    fn custom_models_become_providers_and_are_validated() {
        let provider = gateway().to_provider().expect("valid");
        assert_eq!(provider.id, "custom-my-gateway-eu");
        assert_eq!(provider.kind, ProviderKind::OpenAiChat);
        assert_eq!(provider.base_url, "https://gw.example.com/v1");
        assert_eq!(provider.label("deepseek-chat"), "My Gateway (EU)");
        assert!(provider.custom && provider.needs_key() && provider.env_key.is_none());

        let anthropic = CustomModel {
            kind: CustomKind::Anthropic,
            name: String::new(),
            ..gateway()
        };
        let provider = anthropic.to_provider().expect("valid");
        assert_eq!(provider.kind, ProviderKind::Anthropic);
        assert_eq!(provider.name, "deepseek-chat", "the id names it");
        assert!(provider.labels.is_empty());

        let no_model = CustomModel {
            model: " ".into(),
            ..gateway()
        };
        assert!(
            no_model
                .to_provider()
                .expect_err("an error")
                .contains("model id")
        );
        let no_url = CustomModel {
            base_url: "gw.example.com".into(),
            ..gateway()
        };
        assert!(
            no_url
                .to_provider()
                .expect_err("an error")
                .contains("base URL")
        );
        for (_, _, url) in PRESETS {
            let preset = CustomModel {
                base_url: url.into(),
                ..gateway()
            };
            assert!(preset.to_provider().is_ok(), "{url}");
        }
    }

    #[test]
    fn adding_twice_gets_unique_ids_and_only_custom_ones_are_removed() {
        let mut settings = AgentSettings::default();
        assert_eq!(
            settings.add_custom(&gateway()),
            Ok("custom-my-gateway-eu".into())
        );
        assert_eq!(
            settings.add_custom(&gateway()),
            Ok("custom-my-gateway-eu-2".into())
        );
        settings.default_agent = Some("custom-my-gateway-eu-2/deepseek-chat".into());
        assert!(settings.remove_custom("custom-my-gateway-eu-2"));
        assert_eq!(
            settings.default_agent, None,
            "no default on a removed model"
        );
        assert!(!settings.remove_custom("anthropic"), "built-ins stay");
        assert!(settings.provider("anthropic").is_some());
        assert!(is_ferrum_ai("ferrum") && !is_ferrum_ai("anthropic"));
    }
}
