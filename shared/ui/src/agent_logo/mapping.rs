//! Which logo a program, a provider or a model gets. Pure functions: a
//! model served by Ollama or an OpenAI-compatible server shows its maker's
//! logo (`qwen3-coder:30b` → Qwen); the provider's logo is the fallback.

use super::AgentLogo;

/// Package-name endings that don't change which agent it is
/// (`claude-agent-acp`, `gemini-cli`, `qwen-code`).
const SUFFIXES: [&str; 4] = ["-acp", "-cli", "-agent", "-code"];

fn program_exact(name: &str) -> Option<AgentLogo> {
    use AgentLogo::*;
    Some(match name {
        "claude" | "claude-code" | "claude-code-acp" | "claude-agent-acp" => Claude,
        "codex" | "codex-acp" => Codex,
        "chatgpt" | "openai" => OpenAi,
        "gemini" => Gemini,
        "opencode" => OpenCode,
        "muse" => Muse,
        "deepseek" => DeepSeek,
        "kimi" => Kimi,
        "qwen" => Qwen,
        "glm" | "zai" => Glm,
        "mistral" | "mistral-vibe" => Mistral,
        "ollama" => Ollama,
        "cursor" | "cursor-agent" => Cursor,
        "amp" => Amp,
        "goose" => Goose,
        _ => return None,
    })
}

impl AgentLogo {
    /// The agent a program runs, by its name or path (`claude`,
    /// `/usr/bin/codex`, `@zed-industries/codex-acp`, `gemini-cli`).
    /// Agents without a logo of their own (aider, crush…) get `None`.
    #[must_use]
    pub fn for_program(program: &str) -> Option<Self> {
        let name = program.trim().rsplit('/').next().unwrap_or(program);
        let mut name = name.to_ascii_lowercase();
        if let Some(stem) = name
            .strip_suffix(".js")
            .or_else(|| name.strip_suffix(".exe"))
        {
            name = stem.to_owned();
        }
        loop {
            if let Some(logo) = program_exact(&name) {
                return Some(logo);
            }
            let stem = SUFFIXES.iter().find_map(|s| name.strip_suffix(s))?;
            name = stem.to_owned();
        }
    }

    /// The logo of a model provider, by id or name (`anthropic`, `openai`,
    /// `google`, `dashscope`, `z.ai`, `ollama-remote`…). Gateways that serve
    /// anyone's models (OpenRouter, Groq, vLLM, LM Studio, any
    /// `openai-compatible` server) get `None`.
    #[must_use]
    pub fn for_provider(provider: &str) -> Option<Self> {
        let lower = provider.trim().to_ascii_lowercase();
        let normal: String = lower
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        if normal.contains("compatible") {
            return None;
        }
        let first = lower.split(['-', '_', ' ', '.', '(']).next().unwrap_or("");
        provider_exact(&normal).or_else(|| provider_exact(first))
    }

    /// The maker of a model, by id: `claude-sonnet-5-5`, `gpt-5`, `o4-mini`,
    /// `gpt-5-codex`, `gemini-3-pro`, `deepseek-r1:14b`,
    /// `qwen/qwen3-coder`, `hf.co/unsloth/Qwen3-Coder-30B-GGUF:Q4_K_M`,
    /// `kimi-k2`, `glm-4.6`, `devstral`…
    #[must_use]
    pub fn for_model(model: &str) -> Option<Self> {
        let lower = model.trim().to_ascii_lowercase();
        // Ollama's tag (`:30b`) says nothing about the maker.
        let lower = lower.split(':').next().unwrap_or(&lower);
        let mut parts = lower.rsplit('/');
        let name = parts.next().unwrap_or(lower);
        model_name(name)
            // Bedrock's `us.anthropic.claude-…`.
            .or_else(|| {
                name.match_indices('.')
                    .find_map(|(i, _)| model_name(&name[i + 1..]))
            })
            // `deepseek-ai/…`, `moonshotai/…`, `zai-org/…`: the publisher.
            .or_else(|| parts.find_map(Self::for_provider))
    }

    /// The logo for a model chosen from a provider: the model's maker,
    /// else the provider's (an unknown model on Ollama shows Ollama).
    #[must_use]
    pub fn for_provider_model(provider: &str, model: &str) -> Option<Self> {
        Self::for_model(model).or_else(|| Self::for_provider(provider))
    }
}

fn provider_exact(id: &str) -> Option<AgentLogo> {
    use AgentLogo::*;
    Some(match id {
        "anthropic" | "claude" => Claude,
        "openai" | "chatgpt" | "azureopenai" => OpenAi,
        "codex" => Codex,
        "google" | "gemini" | "googleai" | "vertex" | "vertexai" | "aistudio" => Gemini,
        "deepseek" | "deepseekai" => DeepSeek,
        "moonshot" | "moonshotai" | "kimi" => Kimi,
        "qwen" | "dashscope" | "alibaba" | "alibabacloud" | "aliyun" | "modelstudio" => Qwen,
        "zhipu" | "zhipuai" | "zai" | "zaiorg" | "bigmodel" | "glm" | "thudm" => Glm,
        "mistral" | "mistralai" => Mistral,
        "ollama" => Ollama,
        "meta" | "metallama" | "muse" => Muse,
        "opencode" => OpenCode,
        "cursor" => Cursor,
        _ => return None,
    })
}

fn model_name(name: &str) -> Option<AgentLogo> {
    use AgentLogo::*;
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| name.starts_with(p));
    Some(if starts(&["claude"]) {
        Claude
    } else if starts(&["gemini"]) {
        Gemini
    } else if starts(&["deepseek"]) {
        DeepSeek
    } else if starts(&["kimi", "moonshot"]) {
        Kimi
    } else if starts(&["qwen", "qwq", "qvq"]) {
        Qwen
    } else if starts(&["glm", "chatglm", "codegeex"]) {
        Glm
    } else if starts(&[
        "mistral",
        "mixtral",
        "codestral",
        "devstral",
        "magistral",
        "ministral",
        "pixtral",
        "voxtral",
    ]) {
        Mistral
    } else if starts(&["muse", "llama", "codellama"]) {
        Muse
    } else if name.contains("codex") {
        Codex
    } else if starts(&["gpt", "chatgpt", "text-embedding", "davinci", "dall-e"])
        || (name.starts_with('o') && name[1..].starts_with(|c: char| c.is_ascii_digit()))
    {
        OpenAi
    } else {
        return None;
    })
}
