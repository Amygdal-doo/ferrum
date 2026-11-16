//! Inline agent edits (⌘K): a range of the buffer plus an instruction go to
//! the model chosen for the Agent pane, which answers with the rewritten
//! range only. The editor then shows the answer as a diff to accept or
//! reject. This module builds the request, picks the model, streams the
//! answer on FerrumChat's runtime and cleans it up; no GPUI.

use std::path::PathBuf;

use ferrumagent_core::models::ModelStore;
use ferrumagent_core::provider::{ChatRequest, Message, StopReason, StreamEvent};
use ferrumagent_core::session::AgentChoice;
use futures::StreamExt as _;

use crate::language::Language;

/// Context kept around the range, each side.
const CONTEXT_BYTES: usize = 6_000;
const MAX_TOKENS: u32 = 4_096;

/// What the model sees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineEditRequest {
    pub path: Option<PathBuf>,
    pub language: Language,
    /// Text before and after the range (trimmed to [`CONTEXT_BYTES`]).
    pub before: String,
    pub after: String,
    /// The range to rewrite (may be empty: insert at the caret).
    pub selection: String,
    pub instruction: String,
}

impl InlineEditRequest {
    /// Cuts `text` around `range` (byte offsets on char boundaries).
    pub fn new(
        text: &str,
        range: std::ops::Range<usize>,
        instruction: &str,
        path: Option<PathBuf>,
        language: Language,
    ) -> Self {
        let mut start = range.start.saturating_sub(CONTEXT_BYTES);
        while !text.is_char_boundary(start) {
            start += 1;
        }
        let mut end = (range.end + CONTEXT_BYTES).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Self {
            path,
            language,
            before: text[start..range.start].to_string(),
            after: text[range.end..end].to_string(),
            selection: text[range].to_string(),
            instruction: instruction.trim().to_string(),
        }
    }
}

const SYSTEM: &str = "You are a code editing assistant inside a text editor. \
The user selected a range of a file and asks you to change it. Answer with \
the replacement text for the selected range only: no explanations, no \
Markdown fences, nothing before or after it. Keep the surrounding \
indentation and style. If the selection is empty, answer with the text to \
insert at the cursor.";

/// The provider request for `edit` with `model`.
pub fn chat_request(edit: &InlineEditRequest, model: &str) -> ChatRequest {
    let file = edit.path.as_deref().map_or_else(
        || "an untitled file".to_string(),
        |p| p.display().to_string(),
    );
    let prompt = format!(
        "File: {file} ({language})\n\n\
         <before>\n{before}</before>\n\
         <selection>\n{selection}</selection>\n\
         <after>\n{after}</after>\n\n\
         Instruction: {instruction}\n\n\
         Reply with the new text for <selection> only.",
        language = edit.language.name(),
        before = edit.before,
        selection = edit.selection,
        after = edit.after,
        instruction = edit.instruction,
    );
    ChatRequest {
        model: model.to_string(),
        system: SYSTEM.to_string(),
        messages: vec![Message::user(prompt)],
        tools: Vec::new(),
        max_tokens: MAX_TOKENS,
    }
}

/// The model's answer as replacement text: Markdown fences and stray
/// `<selection>` tags removed; the selection's trailing newline kept or
/// dropped to match it.
pub fn clean_answer(answer: &str, selection: &str) -> String {
    let mut text = answer.trim_matches('\n').to_string();
    let trimmed = text.trim();
    if trimmed.starts_with("```") {
        let body = trimmed
            .split_once('\n')
            .map_or("", |(_, rest)| rest)
            .trim_end();
        text = body.strip_suffix("```").unwrap_or(body).to_string();
        text = text.trim_end_matches('\n').to_string();
    }
    for tag in ["<selection>\n", "<selection>"] {
        if let Some(rest) = text.strip_prefix(tag) {
            text = rest.to_string();
        }
    }
    for tag in ["\n</selection>", "</selection>"] {
        if let Some(rest) = text.strip_suffix(tag) {
            text = rest.to_string();
        }
    }
    if selection.ends_with('\n') && !text.ends_with('\n') && !text.is_empty() {
        text.push('\n');
    }
    text
}

/// The provider and model inline edits use: the Agent pane's choice
/// (`pane_agent`, a thread's `provider/model` key) when it is a model,
/// else the default agent of "Models and API keys" when that is one,
/// else the first Anthropic model. CLI agents (ACP) cannot edit a range,
/// so they fall through.
pub fn pick_model(pane_agent: Option<&str>, store: &ModelStore) -> (String, String) {
    let native = |key: &str| match AgentChoice::parse(key)? {
        AgentChoice::Native { provider, model } => Some((provider, model)),
        AgentChoice::Acp { .. } => None,
    };
    pane_agent
        .and_then(native)
        .or_else(|| store.settings().default_agent.as_deref().and_then(native))
        .unwrap_or_else(|| {
            (
                "anthropic".to_string(),
                ferrumagent_core::settings::ANTHROPIC_MODELS[0].to_string(),
            )
        })
}

/// Streams the answer for `edit` from `provider` / `model` on FerrumChat's
/// runtime; resolves to the cleaned replacement text.
pub fn run(
    store: ModelStore,
    provider: String,
    model: String,
    edit: InlineEditRequest,
) -> impl std::future::Future<Output = Result<String, String>> + Send + 'static {
    let task = ferrumagent_core::runtime::handle().spawn(async move {
        let provider = store.provider(&provider)?;
        let mut stream = provider
            .stream(chat_request(&edit, &model))
            .await
            .map_err(|e| e.to_string())?;
        let mut answer = String::new();
        while let Some(event) = stream.next().await {
            match event.map_err(|e| e.to_string())? {
                StreamEvent::TextDelta(text) => answer.push_str(&text),
                StreamEvent::Stop(StopReason::Error(error)) => return Err(error),
                StreamEvent::Stop(StopReason::Refusal) => {
                    return Err("the model declined this edit".into());
                }
                StreamEvent::Stop(_) => break,
                _ => {}
            }
        }
        Ok(clean_answer(&answer, &edit.selection))
    });
    async move { task.await.map_err(|e| e.to_string())? }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use ferrumagent_core::provider::mock::MockProvider;
    use ferrumagent_core::secrets::MemoryStore;
    use ferrumagent_core::settings::AgentSettings;

    use super::*;

    fn store(provider: Option<MockProvider>, settings: AgentSettings) -> ModelStore {
        let home = std::env::temp_dir().join(format!("ferrumeditor-inline-{}", std::process::id()));
        ModelStore::new(
            home,
            Arc::new(MemoryStore::default()),
            Arc::new(Mutex::new(settings)),
            provider.map(|p| Arc::new(p) as Arc<dyn ferrumagent_core::provider::Provider>),
            Vec::new(),
        )
    }

    #[test]
    fn request_cuts_context_around_the_range() {
        let text = "fn a() {}\nfn b() {\n    1\n}\n";
        let start = text.find("    1").expect("range");
        let edit = InlineEditRequest::new(
            text,
            start..start + 6,
            "  return two ",
            Some("/p/x.rs".into()),
            Language::Rust,
        );
        assert_eq!(edit.selection, "    1\n");
        assert!(edit.before.ends_with("fn b() {\n") && edit.after == "}\n");
        assert_eq!(edit.instruction, "return two");
        let request = chat_request(&edit, "m");
        assert_eq!(request.model, "m");
        assert!(request.tools.is_empty());
        let ferrumagent_core::provider::Part::Text { text: prompt } = &request.messages[0].parts[0]
        else {
            panic!("text prompt");
        };
        assert!(
            prompt.contains("<selection>\n    1\n</selection>"),
            "{prompt}"
        );
        assert!(prompt.contains("/p/x.rs (Rust)"), "{prompt}");
    }

    #[test]
    fn answers_lose_fences_and_match_the_trailing_newline() {
        assert_eq!(clean_answer("```rust\n    2\n```", "    1\n"), "    2\n");
        assert_eq!(clean_answer("    2\n", "    1"), "    2");
        assert_eq!(clean_answer("<selection>\nx\n</selection>", "y"), "x");
        assert_eq!(clean_answer("", "y\n"), "");
    }

    #[test]
    fn picks_the_pane_model_then_the_default() {
        let mut settings = AgentSettings::default();
        let s = store(None, settings.clone());
        assert_eq!(
            pick_model(Some("openai/gpt-x"), &s),
            ("openai".into(), "gpt-x".into())
        );
        assert_eq!(pick_model(Some("acp:claude"), &s).0, "anthropic");
        settings.default_agent = Some("ollama/llama".into());
        let s = store(None, settings);
        assert_eq!(
            pick_model(Some("acp:claude"), &s),
            ("ollama".into(), "llama".into())
        );
    }

    #[test]
    fn runs_against_a_provider() {
        let mock = MockProvider::new();
        mock.push_text("```\nlet two = 2;\n```");
        let s = store(Some(mock.clone()), AgentSettings::default());
        let edit = InlineEditRequest::new("let one = 1;\n", 0..13, "two", None, Language::Rust);
        let answer = futures::executor::block_on(run(s, "anthropic".into(), "m".into(), edit));
        assert_eq!(answer, Ok("let two = 2;\n".to_string()));
        assert_eq!(mock.requests()[0].model, "m");
    }
}
