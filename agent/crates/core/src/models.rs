//! [`ModelStore`]: the providers, models and keys of "Models and API keys"
//! without a chat session — `settings.json` in the agent home (shared by
//! every Ferrum app with AI, so a model added in one shows up in all) and
//! the keychain. A UI uses it before its session has opened; a session
//! hands out one sharing its in-memory settings ([`crate::ChatSession::models`]).
//!
//! Keychain calls block (they may show OS prompts): call [`ModelStore::key_status`],
//! [`ModelStore::set_api_key`], [`ModelStore::add_custom`] and
//! [`ModelStore::remove_custom`] off the UI thread.

use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use crate::acp::AcpAgentConfig;
use crate::custom_models::{self, CustomModel};
use crate::permissions::Approval;
use crate::provider::{self, ModelInfo, Provider, ping};
use crate::secrets::{self, ApiKey, KeySource, SecretStore};
use crate::session::{AgentChoice, AgentOption, SessionConfig};
use crate::settings::{AgentSettings, ProviderSettings};

#[derive(Clone)]
pub struct ModelStore {
    home: PathBuf,
    secrets: Arc<dyn SecretStore>,
    /// What applies while `settings.json` does not exist (the defaults, or
    /// a test's settings); kept in step with every save.
    fallback: Arc<Mutex<AgentSettings>>,
    /// Answers every request instead of a real provider (tests, demos).
    provider_override: Option<Arc<dyn Provider>>,
    acp_agents: Vec<AcpAgentConfig>,
}

impl ModelStore {
    pub fn new(
        home: PathBuf,
        secrets: Arc<dyn SecretStore>,
        fallback: Arc<Mutex<AgentSettings>>,
        provider_override: Option<Arc<dyn Provider>>,
        acp_agents: Vec<AcpAgentConfig>,
    ) -> Self {
        Self {
            home,
            secrets,
            fallback,
            provider_override,
            acp_agents,
        }
    }

    /// The store a session made from `config` will use.
    pub fn from_config(config: &SessionConfig) -> Self {
        Self::new(
            config.home.clone(),
            config.secrets.clone(),
            Arc::new(Mutex::new(config.settings.clone())),
            config.provider_override.clone(),
            config.acp_agents.clone(),
        )
    }

    fn file(&self) -> PathBuf {
        crate::paths::settings_file(&self.home)
    }

    /// The saved settings, else the fallback.
    pub fn settings(&self) -> AgentSettings {
        AgentSettings::read(&self.file()).unwrap_or_else(|| {
            self.fallback
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        })
    }

    fn save(&self, settings: AgentSettings) -> Result<(), String> {
        settings.save(&self.file()).map_err(|e| e.to_string())?;
        *self.fallback.lock().unwrap_or_else(PoisonError::into_inner) = settings;
        Ok(())
    }

    /// Saves the providers of `edited` (the settings sheet's); the default
    /// agent and approval level stay as saved.
    pub fn save_providers(&self, edited: &AgentSettings) -> Result<(), String> {
        let mut settings = self.settings();
        settings.providers = edited.providers.clone();
        self.save(settings)
    }

    /// The approval level as last loaded or saved here (no file read: a
    /// UI may ask on every frame).
    pub fn approval(&self) -> Approval {
        self.fallback
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .approval
    }

    pub fn set_approval(&self, approval: Approval) -> Result<(), String> {
        let mut settings = self.settings();
        settings.approval = approval;
        self.save(settings)
    }

    pub fn acp_agents(&self) -> &[AcpAgentConfig] {
        &self.acp_agents
    }

    /// The model selector's rows.
    pub fn agent_options(&self) -> Vec<AgentOption> {
        agent_options(&self.settings(), &self.acp_agents)
    }

    /// Where a provider's key comes from, and its hint. Blocking.
    pub fn key_status(&self, provider_id: &str) -> Option<(KeySource, String)> {
        let settings = self.settings();
        let provider = settings.provider(provider_id)?;
        secrets::resolve(
            self.secrets.as_ref(),
            &provider.id,
            provider.env_key.as_deref(),
        )
        .map(|(key, source)| (source, key.hint()))
    }

    /// Stores a key in the keychain (empty: removes it). Blocking.
    pub fn set_api_key(&self, provider_id: &str, key: &str) -> Result<(), String> {
        let key = key.trim();
        if key.is_empty() {
            return self.secrets.delete(provider_id).map_err(|e| e.to_string());
        }
        self.secrets
            .set(provider_id, &ApiKey::new(key))
            .map_err(|e| e.to_string())
    }

    /// Adds a custom model (its key, if any, to the keychain) and returns
    /// its provider id. Blocking.
    pub fn add_custom(&self, model: &CustomModel, key: &str) -> Result<String, String> {
        let mut settings = self.settings();
        let id = settings.add_custom(model)?;
        if !key.trim().is_empty() {
            self.set_api_key(&id, key)?;
        }
        if let Err(e) = self.save(settings) {
            self.secrets.delete(&id).ok();
            return Err(e);
        }
        Ok(id)
    }

    /// Removes a custom model and its key. Blocking.
    pub fn remove_custom(&self, provider_id: &str) -> Result<(), String> {
        let mut settings = self.settings();
        if !settings.remove_custom(provider_id) {
            return Err(format!("`{provider_id}` is not a custom model"));
        }
        self.save(settings)?;
        self.secrets.delete(provider_id).map_err(|e| e.to_string())
    }

    /// The provider behind `provider_id`, with its key. Blocking.
    pub fn provider(&self, provider_id: &str) -> Result<Arc<dyn Provider>, String> {
        if custom_models::is_ferrum_ai(provider_id) {
            return Err(format!(
                "{} is coming soon.",
                custom_models::FERRUM_AI_LABEL
            ));
        }
        if let Some(provider) = &self.provider_override {
            return Ok(provider.clone());
        }
        let settings = self.settings();
        let provider = settings
            .provider(provider_id)
            .ok_or_else(|| format!("unknown provider `{provider_id}`"))?;
        provider::build(provider, self.secrets.as_ref()).map_err(|e| ping::describe(&e))
    }

    /// The models the provider lists.
    pub fn fetch_models(
        &self,
        provider_id: &str,
    ) -> impl Future<Output = Result<Vec<ModelInfo>, String>> + Send + use<> {
        let store = self.clone();
        let provider_id = provider_id.to_string();
        spawn(async move {
            let provider = store.provider(&provider_id)?;
            provider.list_models().await.map_err(|e| e.to_string())
        })
    }

    /// Sends `model` of `provider_id` a tiny request ([`ping::ping`]).
    pub fn ping(
        &self,
        provider_id: &str,
        model: &str,
    ) -> impl Future<Output = Result<ping::Pong, String>> + Send + use<> {
        let store = self.clone();
        let (provider_id, model) = (provider_id.to_string(), model.to_string());
        spawn(async move {
            let provider = store.provider(&provider_id)?;
            ping::ping(provider.as_ref(), &model, ping::TIMEOUT).await
        })
    }

    /// Like [`Self::ping`] with `settings` as edited in a form (not yet
    /// saved), its key from the keychain.
    pub fn ping_provider(
        &self,
        settings: ProviderSettings,
        model: &str,
    ) -> impl Future<Output = Result<ping::Pong, String>> + Send + use<> {
        let store = self.clone();
        let model = model.to_string();
        spawn(async move {
            let provider = match &store.provider_override {
                Some(provider) => provider.clone(),
                None => provider::build(&settings, store.secrets.as_ref())
                    .map_err(|e| ping::describe(&e))?,
            };
            ping::ping(provider.as_ref(), &model, ping::TIMEOUT).await
        })
    }

    /// Tests a custom model before it is added, with the form's key.
    pub fn ping_custom(
        &self,
        model: &CustomModel,
        key: &str,
    ) -> impl Future<Output = Result<ping::Pong, String>> + Send + use<> {
        let provider = model.to_provider().map(|settings| {
            let key = Some(key.trim()).filter(|k| !k.is_empty()).map(ApiKey::new);
            match &self.provider_override {
                Some(provider) => provider.clone(),
                None => provider::build_with_key(&settings, key),
            }
        });
        let model = model.model.trim().to_string();
        spawn(async move { ping::ping(provider?.as_ref(), &model, ping::TIMEOUT).await })
    }
}

/// Runs `work` on the core runtime; the returned future works on any
/// executor.
fn spawn<T: Send + 'static>(
    work: impl Future<Output = Result<T, String>> + Send + 'static,
) -> impl Future<Output = Result<T, String>> + Send + 'static {
    let task = crate::runtime::handle().spawn(work);
    async move { task.await.unwrap_or_else(|e| Err(e.to_string())) }
}

/// The model selector's rows: FerrumAI first (disabled, coming soon), the
/// enabled providers' models (display names when set), then external
/// agents.
pub fn agent_options(settings: &AgentSettings, acp_agents: &[AcpAgentConfig]) -> Vec<AgentOption> {
    let mut out = vec![AgentOption {
        choice: AgentChoice::Native {
            provider: custom_models::FERRUM_AI_PROVIDER.into(),
            model: custom_models::FERRUM_AI_MODEL.into(),
        },
        label: custom_models::FERRUM_AI_LABEL.into(),
        group: "Ferrum".into(),
        enabled: false,
        note: Some(custom_models::COMING_SOON.into()),
    }];
    for provider in settings.providers.iter().filter(|p| p.enabled) {
        for model in &provider.models {
            out.push(AgentOption {
                choice: AgentChoice::Native {
                    provider: provider.id.clone(),
                    model: model.clone(),
                },
                label: provider.label(model),
                group: provider.name.clone(),
                enabled: true,
                note: None,
            });
        }
    }
    for agent in acp_agents {
        out.push(AgentOption {
            choice: AgentChoice::Acp {
                agent: agent.id.clone(),
            },
            label: agent.name.clone(),
            group: "External agents (ACP)".into(),
            enabled: true,
            note: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom_models::CustomKind;
    use crate::provider::mock::MockProvider;
    use crate::secrets::MemoryStore;

    fn store(home: &std::path::Path, secrets: &MemoryStore) -> ModelStore {
        ModelStore::new(
            home.to_path_buf(),
            Arc::new(secrets.clone()),
            Arc::new(Mutex::new(AgentSettings::default())),
            None,
            Vec::new(),
        )
    }

    fn gateway() -> CustomModel {
        CustomModel {
            kind: CustomKind::OpenAi,
            name: "Gateway".into(),
            base_url: "https://gw.example.com/v1".into(),
            model: "gw-large".into(),
        }
    }

    #[test]
    fn custom_models_persist_without_their_keys() {
        let home = tempfile::tempdir().expect("home");
        let secrets = MemoryStore::default();
        let models = store(home.path(), &secrets);
        let key = "sk-gw-0123456789abcdef";
        let id = models.add_custom(&gateway(), key).expect("added");
        assert_eq!(id, "custom-gateway");

        // The file has the model but never the key; the keychain has it.
        let text = std::fs::read_to_string(crate::paths::settings_file(home.path()))
            .expect("settings.json");
        assert!(
            text.contains("gw-large") && text.contains("\"custom\": true"),
            "{text}"
        );
        assert!(!text.contains(key), "{text}");
        assert_eq!(
            secrets
                .get(&id)
                .expect("get")
                .map(|k| k.expose().to_string()),
            Some(key.to_string())
        );
        assert_eq!(
            models.key_status(&id),
            Some((KeySource::Keychain, "sk-g…cdef".into()))
        );

        // Another app's store on the same home sees it in its selector.
        let other = store(home.path(), &secrets);
        let option = other
            .agent_options()
            .into_iter()
            .find(|o| o.group == "Gateway")
            .expect("listed");
        assert_eq!(option.choice.key(), "custom-gateway/gw-large");
        assert!(option.enabled);

        // Removing it takes the key too; built-ins can't be removed.
        other.remove_custom(&id).expect("removed");
        assert!(secrets.get(&id).expect("get").is_none());
        assert!(models.settings().provider(&id).is_none());
        assert!(models.remove_custom("anthropic").is_err());
    }

    #[test]
    fn ferrum_ai_is_listed_first_and_disabled() {
        let options = agent_options(&AgentSettings::default(), &crate::acp::builtin_agents());
        let first = &options[0];
        assert_eq!(first.label, "FerrumAI");
        assert!(!first.enabled);
        assert_eq!(first.note.as_deref(), Some("Coming soon"));
        assert!(options[1..].iter().all(|o| o.enabled));
        assert_eq!(options.iter().filter(|o| o.label == "FerrumAI").count(), 1);

        // Nothing can be sent to it, even with a provider override.
        let home = tempfile::tempdir().expect("home");
        let mut models = store(home.path(), &MemoryStore::default());
        models.provider_override = Some(Arc::new(MockProvider::new()));
        let error = models.provider("ferrum").err().expect("refused");
        assert_eq!(error, "FerrumAI is coming soon.");
    }

    #[test]
    fn pings_the_saved_or_the_typed_model() {
        let home = tempfile::tempdir().expect("home");
        let mock = MockProvider::new();
        mock.push_text("pong").push_text("pong");
        let mut models = store(home.path(), &MemoryStore::default());
        models.provider_override = Some(Arc::new(mock.clone()));
        use futures::executor::block_on as run;
        let pong = run(models.ping("anthropic", "claude-haiku-5-5")).expect("pong");
        assert_eq!(pong.reply, "pong");
        let pong = run(models.ping_custom(&gateway(), "sk-typed")).expect("pong");
        assert_eq!(pong.reply, "pong");
        let sent: Vec<String> = mock.requests().into_iter().map(|r| r.model).collect();
        assert_eq!(sent, ["claude-haiku-5-5", "gw-large"]);
        let bad = CustomModel {
            base_url: String::new(),
            ..gateway()
        };
        assert!(
            run(models.ping_custom(&bad, ""))
                .expect_err("an error")
                .contains("base URL")
        );

        // A provider without a key says so instead of sending.
        models.provider_override = None;
        models.fallback.lock().expect("lock").providers[1].env_key =
            Some("FERRUM_TEST_SURELY_UNSET_VAR".into());
        let error = run(models.ping("openai", "gpt-5")).expect_err("an error");
        assert!(error.starts_with("no API key for OpenAI"), "{error}");
    }
}
