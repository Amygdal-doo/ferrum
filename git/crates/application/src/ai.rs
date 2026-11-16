//! Drafts a commit message from the staged diff, or a pull request title
//! and description from the branch's commits, with a model from the shared
//! agent settings (`~/.ferrum/agent`: the same providers, models and
//! keychain keys as every Ferrum app's chat; FerrumAI is listed there but
//! disabled, so it is never offered). Drafts only: nothing is committed or
//! posted here, the user edits and confirms.

use crate::{AppError, AppResult, AppServices};
use ferrumagent_core::models::ModelStore;
use ferrumagent_core::provider::{ChatRequest, Message, Provider, StopReason, StreamEvent, ping};
use ferrumagent_core::session::AgentChoice;
use ferrumagent_core::settings::AgentSettings;
use futures::StreamExt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Most of a diff or log sent to the model.
pub const MAX_PROMPT_BYTES: usize = 60_000;
/// How long a draft may take.
pub const DRAFT_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_TOKENS: u32 = 1_024;

/// A model that can draft (a native provider's model, enabled).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiModel {
    /// `provider/model`.
    pub key: String,
    pub label: String,
    /// Provider name.
    pub group: String,
}

/// A drafted message: first line and the rest.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Draft {
    pub title: String,
    pub body: String,
}

/// The shared agent model store (settings file + OS keychain).
#[must_use]
pub fn default_model_store() -> ModelStore {
    ModelStore::new(
        ferrumagent_core::paths::agent_home(),
        ferrumagent_core::secrets::default_store(),
        Arc::new(Mutex::new(AgentSettings::default())),
        None,
        Vec::new(),
    )
}

/// Enabled native models from the model selector's rows (FerrumAI and
/// external ACP agents cannot draft).
#[must_use]
pub fn models(store: &ModelStore) -> Vec<AiModel> {
    store
        .agent_options()
        .into_iter()
        .filter(|o| o.enabled && !o.choice.is_coming_soon())
        .filter_map(|o| match o.choice {
            AgentChoice::Native { provider, model } => Some(AiModel {
                key: format!("{provider}/{model}"),
                label: o.label,
                group: o.group,
            }),
            AgentChoice::Acp { .. } => None,
        })
        .collect()
}

/// `wanted` if it is available, else the agent's default model, else the first.
#[must_use]
pub fn pick_model(store: &ModelStore, wanted: Option<&str>) -> Option<AiModel> {
    let all = models(store);
    let default = store.settings().default_agent;
    [wanted, default.as_deref()]
        .into_iter()
        .flatten()
        .find_map(|k| all.iter().find(|m| m.key == k).cloned())
        .or_else(|| all.first().cloned())
}

const COMMIT_SYSTEM: &str = "You write git commit messages. Answer with the message only: \
a summary line in the imperative mood of at most 72 characters, a blank line, then a short \
body (wrapped at 72 columns) explaining what changed and why, only when that is not obvious \
from the summary. No Markdown headings, no code fences, no quotes around the message.";

const PR_SYSTEM: &str = "You write pull request descriptions. Answer with the title on the \
first line (plain text, at most 72 characters, no prefix), a blank line, then a Markdown \
description: a short summary paragraph and a bullet list of the notable changes. Mention \
anything reviewers should check. No code fences around the whole answer.";

/// The prompt for a commit message from the staged changes.
#[must_use]
pub fn commit_prompt(stat: &str, patch: &str) -> String {
    format!("Write the commit message for these staged changes.\n\nFiles:\n{stat}\nDiff:\n{patch}")
}

/// The prompt for a pull request from the branch's commits.
#[must_use]
pub fn pr_prompt(head: &str, base: &str, log: &str, stat: &str) -> String {
    format!(
        "Write the pull request for merging `{head}` into `{base}`.\n\n\
         Commits (oldest first):\n{log}\nFiles changed:\n{stat}"
    )
}

/// Splits a model's answer into title and body, dropping code fences,
/// quotes and `Title:`-style prefixes models sometimes add.
#[must_use]
pub fn parse_draft(answer: &str) -> Draft {
    let text = answer.trim();
    let text = text
        .strip_prefix("```")
        .map(|t| t.split_once('\n').map_or("", |(_, rest)| rest))
        .map_or(text, |t| t.trim_end().trim_end_matches("```"))
        .trim();
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    let mut title = first.trim().trim_start_matches('#').trim();
    for prefix in [
        "Title:", "Summary:", "Subject:", "title:", "summary:", "subject:",
    ] {
        title = title.strip_prefix(prefix).unwrap_or(title).trim();
    }
    let title = title.trim_matches(['"', '`', '*']).trim().to_owned();
    Draft {
        title,
        body: rest.trim().to_owned(),
    }
}

/// Sends one prompt to `model` of `provider` and collects the answer.
pub async fn ask(
    provider: &dyn Provider,
    model: &str,
    system: &str,
    prompt: String,
) -> Result<String, String> {
    let request = ChatRequest {
        model: model.to_owned(),
        system: system.to_owned(),
        messages: vec![Message::user(prompt)],
        tools: Vec::new(),
        max_tokens: MAX_TOKENS,
    };
    let answer = async {
        let mut stream = provider
            .stream(request)
            .await
            .map_err(|e| ping::describe(&e))?;
        let mut text = String::new();
        while let Some(event) = stream.next().await {
            match event.map_err(|e| ping::describe(&e))? {
                StreamEvent::TextDelta(t) => text.push_str(&t),
                StreamEvent::Stop(StopReason::Error(message)) => return Err(message),
                StreamEvent::Stop(StopReason::Refusal) => {
                    return Err("The model declined to answer.".to_owned());
                }
                StreamEvent::Stop(_) => break,
                _ => {}
            }
        }
        Ok(text)
    };
    match tokio::time::timeout(DRAFT_TIMEOUT, answer).await {
        Err(_) => Err(format!("No answer within {} s.", DRAFT_TIMEOUT.as_secs())),
        Ok(result) => result,
    }
}

impl AppServices {
    /// Drafts use `store` instead of the shared agent settings (tests, demos).
    #[must_use]
    pub fn with_model_store(mut self, store: ModelStore) -> Self {
        self.models = Arc::new(std::sync::OnceLock::from(store));
        self
    }

    /// The shared agent model store, opened on first use.
    fn model_store(&self) -> ModelStore {
        self.models.get_or_init(default_model_store).clone()
    }

    /// The models drafts can use. Reads the settings file.
    pub async fn ai_models(&self) -> AppResult<Vec<AiModel>> {
        let store = self.model_store();
        tokio::task::spawn_blocking(move || models(&store))
            .await
            .map_err(|e| AppError::Invalid(e.to_string()))
    }

    /// Sends `prompt` to the chosen (or default) model.
    async fn draft(&self, model: Option<String>, system: &str, prompt: String) -> AppResult<Draft> {
        let store = self.model_store();
        // Settings file and keychain reads block.
        let (chosen, provider) = tokio::task::spawn_blocking(move || {
            let chosen = pick_model(&store, model.as_deref()).ok_or_else(|| {
                "No AI model is set up: add a provider key in the agent's Models and API keys."
                    .to_owned()
            })?;
            let (provider_id, _) = chosen.key.split_once('/').unwrap_or((&chosen.key, ""));
            let provider = store.provider(provider_id)?;
            Ok::<_, String>((chosen, provider))
        })
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))?
        .map_err(AppError::Invalid)?;
        let (_, model_id) = chosen.key.split_once('/').unwrap_or(("", &chosen.key));
        tracing::info!(model = %chosen.key, "drafting with AI");
        let answer = ask(provider.as_ref(), model_id, system, prompt)
            .await
            .map_err(AppError::Invalid)?;
        let draft = parse_draft(&answer);
        if draft.title.is_empty() {
            return Err(AppError::Invalid(
                "The model returned an empty draft.".into(),
            ));
        }
        Ok(draft)
    }

    /// A commit message for the staged changes of the repository at `path`.
    pub async fn draft_commit_message(
        &self,
        path: &str,
        model: Option<String>,
    ) -> AppResult<Draft> {
        let repo = self.repo(path).await?;
        let (stat, patch) = repo.staged_patch(MAX_PROMPT_BYTES).await?;
        if patch.trim().is_empty() {
            return Err(AppError::Invalid(
                "Stage the changes to describe first.".into(),
            ));
        }
        self.draft(model, COMMIT_SYSTEM, commit_prompt(&stat, &patch))
            .await
    }

    /// A pull request title and description for the current branch against
    /// `base` (`<remote>/<base>` when that exists, so local `base` may lag).
    pub async fn draft_pull_request(
        &self,
        path: &str,
        remote: &str,
        base: &str,
        model: Option<String>,
    ) -> AppResult<Draft> {
        let repo = self.repo(path).await?;
        let remote_base = format!("{remote}/{base}");
        let from = if repo.rev_exists(&remote_base).await {
            remote_base
        } else if repo.rev_exists(base).await {
            base.to_owned()
        } else {
            return Err(AppError::Invalid(format!("unknown base branch {base:?}")));
        };
        let head = repo
            .status()
            .await?
            .branch
            .head
            .unwrap_or_else(|| "HEAD".into());
        let (log, stat) = repo.branch_summary(&from, MAX_PROMPT_BYTES).await?;
        if log.trim().is_empty() {
            return Err(AppError::Invalid(format!(
                "{head} has no commits that are not on {from}."
            )));
        }
        self.draft(model, PR_SYSTEM, pr_prompt(&head, base, &log, &stat))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_titles_and_bodies() {
        assert_eq!(
            parse_draft("Add login form\n\nWith validation.\n"),
            Draft {
                title: "Add login form".into(),
                body: "With validation.".into()
            }
        );
        assert_eq!(
            parse_draft("```\nFix crash on start\n\nGuard the config.\n```").title,
            "Fix crash on start"
        );
        assert_eq!(
            parse_draft("```text\nTitle: \"Speed up search\"\n```").title,
            "Speed up search"
        );
        assert_eq!(parse_draft("# Refactor parser").title, "Refactor parser");
        assert_eq!(parse_draft("  ").title, "");
    }

    #[test]
    fn prompts_carry_the_changes() {
        let p = commit_prompt(" a.rs | 2 +-\n", "diff --git a/a.rs b/a.rs\n");
        assert!(p.contains("a.rs | 2") && p.contains("diff --git"));
        let p = pr_prompt("feature", "main", "- Add x\n", " x.rs | 1 +\n");
        assert!(p.contains("`feature` into `main`") && p.contains("- Add x"));
    }
}
