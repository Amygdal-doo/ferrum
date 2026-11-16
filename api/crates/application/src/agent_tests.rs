//! Drafting test scripts with the shared agent core (`ferrumagent-core`):
//! the same models and API keys as FerrumChat and the other Ferrum apps
//! (`~/.ferrum/agent/settings.json` + the keychain), FerrumAI listed but
//! disabled. One prompt per request, no tools; the answer is a script the
//! user reviews before anything is saved.
//!
//! What is sent: the request's name, method, URL (with `{{variables}}`, not
//! their values), header names, body (truncated), description (OpenAPI
//! expected responses) and, if there is one, the last response's status,
//! content type and body (truncated). Never auth or header values.

use ferrumagent_core::custom_models;
use ferrumagent_core::models::ModelStore;
use ferrumagent_core::provider::{ChatRequest, Message, Provider, StreamEvent};
use ferrumagent_core::session::AgentChoice;
use ferrumagent_core::settings::AgentSettings;
use ferrumapi_domain::script::{LANGUAGE_GUIDE, check};
use ferrumapi_domain::{HttpRequest, HttpResponse, RequestBody};
use futures::StreamExt as _;
use std::sync::{Arc, Mutex};

/// Longest body / description text put in a prompt.
const MAX_TEXT: usize = 4_000;

/// A model the drafter can use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftModel {
    pub provider: String,
    pub model: String,
    pub label: String,
    /// False for FerrumAI (coming soon).
    pub enabled: bool,
}

/// One request to draft tests for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftInput {
    pub name: String,
    pub request: HttpRequest,
    pub description: String,
    pub response: Option<HttpResponse>,
}

/// A drafted script and the lines that do not parse (`(line, message)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub script: String,
    pub problems: Vec<(usize, String)>,
}

const SYSTEM: &str = "You write test scripts for FerrumAPI, an HTTP client. \
Reply with the script only, in one ```ferrum code block, no explanation. \
Write 3 to 8 checks: the expected status, the content type, and the fields and types of \
the response body that the API description promises. Use only the statements, values, \
operators and functions of the language below; anything else is a syntax error. \
Do not invent fields the description does not mention; when there is no description, \
check what the last response shows. Name important checks with `test \"…\":`.";

#[derive(Clone)]
pub struct TestDrafter {
    models: ModelStore,
}

impl TestDrafter {
    /// The models and keys every Ferrum app with AI shares.
    #[must_use]
    pub fn load() -> Self {
        let home = ferrumagent_core::paths::agent_home();
        let settings = AgentSettings::load(&ferrumagent_core::paths::settings_file(&home));
        Self {
            models: ModelStore::new(
                home,
                ferrumagent_core::secrets::default_store(),
                Arc::new(Mutex::new(settings)),
                None,
                Vec::new(),
            ),
        }
    }

    /// A drafter answering with `provider` (tests): default settings, no keys.
    #[must_use]
    pub fn with_provider(provider: Arc<dyn Provider>) -> Self {
        let home = std::env::temp_dir().join(format!("ferrumapi-drafter-{}", uuid::Uuid::new_v4()));
        Self {
            models: ModelStore::new(
                home,
                Arc::new(ferrumagent_core::secrets::MemoryStore::default()),
                Arc::new(Mutex::new(AgentSettings::default())),
                Some(provider),
                Vec::new(),
            ),
        }
    }

    /// The chat models of "Models and API keys" (FerrumAI first, disabled);
    /// external ACP agents are left out (they take whole sessions, not a prompt).
    #[must_use]
    pub fn models(&self) -> Vec<DraftModel> {
        self.models
            .agent_options()
            .into_iter()
            .filter_map(|o| match o.choice {
                AgentChoice::Native { provider, model } => Some(DraftModel {
                    enabled: o.enabled && !custom_models::is_ferrum_ai(&provider),
                    provider,
                    model,
                    label: o.label,
                }),
                AgentChoice::Acp { .. } => None,
            })
            .collect()
    }

    /// The settings' default model when it is a usable chat model, else the
    /// first usable one.
    #[must_use]
    pub fn default_model(&self) -> Option<DraftModel> {
        let models = self.models();
        let preferred = self
            .models
            .settings()
            .default_agent
            .and_then(|k| AgentChoice::parse(&k));
        let usable = |m: &&DraftModel| m.enabled;
        preferred
            .and_then(|p| match p {
                AgentChoice::Native { provider, model } => models
                    .iter()
                    .filter(usable)
                    .find(|m| m.provider == provider && m.model == model)
                    .cloned(),
                AgentChoice::Acp { .. } => None,
            })
            .or_else(|| models.iter().find(usable).cloned())
    }

    /// Asks `model` for a test script for `input`.
    pub async fn draft(&self, model: &DraftModel, input: &DraftInput) -> Result<Draft, String> {
        if !model.enabled || custom_models::is_ferrum_ai(&model.provider) {
            return Err(format!("{} is not available yet.", model.label));
        }
        let store = self.models.clone();
        let id = model.provider.clone();
        // Building a provider reads its key from the keychain (blocking).
        let provider = tokio::task::spawn_blocking(move || store.provider(&id))
            .await
            .map_err(|e| e.to_string())??;
        let request = ChatRequest {
            model: model.model.clone(),
            system: format!("{SYSTEM}\n\n{LANGUAGE_GUIDE}"),
            messages: vec![Message::user(prompt(input))],
            tools: Vec::new(),
            max_tokens: 2_000,
        };
        let mut stream = provider.stream(request).await.map_err(|e| e.to_string())?;
        let mut answer = String::new();
        while let Some(event) = stream.next().await {
            match event.map_err(|e| e.to_string())? {
                StreamEvent::TextDelta(t) => answer.push_str(&t),
                StreamEvent::Stop(_) => break,
                _ => {}
            }
        }
        let script = extract_script(&answer);
        if script.trim().is_empty() {
            return Err("the model returned no script".into());
        }
        Ok(Draft {
            problems: check(&script),
            script,
        })
    }
}

fn cut(s: &str) -> String {
    if s.chars().count() > MAX_TEXT {
        format!(
            "{}\n…(truncated)",
            s.chars().take(MAX_TEXT).collect::<String>()
        )
    } else {
        s.to_owned()
    }
}

/// The user message for `input` (no auth, no header values).
#[must_use]
pub fn prompt(input: &DraftInput) -> String {
    let r = &input.request;
    let mut out = format!(
        "Request \"{}\" ({}): {} {}\n",
        input.name,
        r.kind.label(),
        r.method,
        r.url.trim()
    );
    let headers: Vec<&str> = r
        .headers
        .iter()
        .filter(|h| h.is_active())
        .map(|h| h.key.trim())
        .collect();
    if !headers.is_empty() {
        out.push_str(&format!("Header names: {}\n", headers.join(", ")));
    }
    let body = match &r.body {
        RequestBody::Raw { text, .. } => text.clone(),
        RequestBody::UrlEncoded { fields } => fields
            .iter()
            .filter(|f| f.is_active())
            .map(|f| format!("{}=…", f.key))
            .collect::<Vec<_>>()
            .join("&"),
        _ => String::new(),
    };
    let body = if r.specs.graphql.query.trim().is_empty() {
        body
    } else {
        r.specs.graphql.query.clone()
    };
    if !body.trim().is_empty() {
        out.push_str(&format!("Request body:\n{}\n", cut(&body)));
    }
    if !input.description.trim().is_empty() {
        out.push_str(&format!("API description:\n{}\n", cut(&input.description)));
    }
    if let Some(resp) = &input.response {
        out.push_str(&format!(
            "Last response: {} {}, Content-Type: {}\nBody:\n{}\n",
            resp.status,
            resp.status_text,
            resp.content_type().unwrap_or("none"),
            cut(&resp.body_text())
        ));
    }
    out.push_str("Write the test script.");
    out
}

/// The script inside the first fenced block (any language tag), or the
/// whole answer when there is none.
#[must_use]
pub fn extract_script(answer: &str) -> String {
    let Some(start) = answer.find("```") else {
        return answer.trim().to_owned();
    };
    let after = &answer[start + 3..];
    // Skip the language tag line.
    let body = after.split_once('\n').map_or("", |(_, rest)| rest);
    let end = body.find("```").unwrap_or(body.len());
    body[..end].trim().to_owned()
}

#[cfg(test)]
#[path = "agent_tests_tests.rs"]
mod tests;
