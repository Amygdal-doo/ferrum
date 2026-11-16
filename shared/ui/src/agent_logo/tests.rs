use super::AgentLogo::{self, *};

fn check(f: fn(&str) -> Option<AgentLogo>, cases: &[(&str, Option<AgentLogo>)]) {
    for (input, want) in cases {
        assert_eq!(f(input), *want, "{input}");
    }
}

#[test]
fn programs() {
    check(
        AgentLogo::for_program,
        &[
            ("claude", Some(Claude)),
            ("/opt/bin/claude", Some(Claude)),
            ("claude-code", Some(Claude)),
            ("@agentclientprotocol/claude-agent-acp", Some(Claude)),
            ("@zed-industries/claude-code-acp", Some(Claude)),
            ("Codex", Some(Codex)),
            ("@openai/codex", Some(Codex)),
            ("@zed-industries/codex-acp", Some(Codex)),
            ("chatgpt", Some(OpenAi)),
            ("gemini", Some(Gemini)),
            ("@google/gemini-cli", Some(Gemini)),
            ("opencode", Some(OpenCode)),
            ("qwen", Some(Qwen)),
            ("@qwen-code/qwen-code", Some(Qwen)),
            ("kimi", Some(Kimi)),
            ("kimi-cli", Some(Kimi)),
            ("cursor-agent", Some(Cursor)),
            ("amp", Some(Amp)),
            ("goose", Some(Goose)),
            ("ollama", Some(Ollama)),
            ("muse", Some(Muse)),
            ("deepseek", Some(DeepSeek)),
            ("cli.js", None),
            ("aider", None),
            ("crush", None),
            ("zsh", None),
            ("", None),
        ],
    );
}

#[test]
fn providers() {
    check(
        AgentLogo::for_provider,
        &[
            ("anthropic", Some(Claude)),
            ("Anthropic", Some(Claude)),
            ("openai", Some(OpenAi)),
            ("OpenAI", Some(OpenAi)),
            ("google", Some(Gemini)),
            ("gemini", Some(Gemini)),
            ("vertex-ai", Some(Gemini)),
            ("deepseek", Some(DeepSeek)),
            ("moonshot", Some(Kimi)),
            ("moonshotai", Some(Kimi)),
            ("kimi", Some(Kimi)),
            ("zhipu", Some(Glm)),
            ("z.ai", Some(Glm)),
            ("Z.ai", Some(Glm)),
            ("zai-org", Some(Glm)),
            ("glm", Some(Glm)),
            ("qwen", Some(Qwen)),
            ("dashscope", Some(Qwen)),
            ("alibaba-cloud", Some(Qwen)),
            ("mistral", Some(Mistral)),
            ("ollama", Some(Ollama)),
            ("Ollama (local)", Some(Ollama)),
            ("ollama-remote", Some(Ollama)),
            ("openai-compatible", None),
            ("openrouter", None),
            ("groq", None),
            ("lmstudio", None),
            ("", None),
        ],
    );
}

#[test]
fn models() {
    check(
        AgentLogo::for_model,
        &[
            ("claude-sonnet-5-5", Some(Claude)),
            ("claude-opus-5-5", Some(Claude)),
            ("anthropic/claude-haiku-5-5", Some(Claude)),
            ("us.anthropic.claude-sonnet-4-5-v1:0", Some(Claude)),
            ("gpt-5", Some(OpenAi)),
            ("gpt-5-mini", Some(OpenAi)),
            ("gpt-oss:20b", Some(OpenAi)),
            ("o3", Some(OpenAi)),
            ("o4-mini", Some(OpenAi)),
            ("chatgpt-4o-latest", Some(OpenAi)),
            ("gpt-5-codex", Some(Codex)),
            ("codex-mini-latest", Some(Codex)),
            ("gemini-3-pro", Some(Gemini)),
            ("models/gemini-2.5-flash", Some(Gemini)),
            ("deepseek-v4", Some(DeepSeek)),
            ("deepseek-r1:14b", Some(DeepSeek)),
            ("deepseek-ai/DeepSeek-V3.2", Some(DeepSeek)),
            ("qwen2.5-coder", Some(Qwen)),
            ("qwen3-coder:30b", Some(Qwen)),
            ("qwen/qwen3-coder-plus", Some(Qwen)),
            ("Qwen/Qwen3-Coder-480B-A35B-Instruct", Some(Qwen)),
            (
                "hf.co/unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF:Q4_K_M",
                Some(Qwen),
            ),
            ("qwq:32b", Some(Qwen)),
            ("kimi-k3", Some(Kimi)),
            ("kimi-k2:1t-cloud", Some(Kimi)),
            ("moonshot-v1-8k", Some(Kimi)),
            ("moonshotai/Kimi-K2-Instruct", Some(Kimi)),
            ("glm-4.6", Some(Glm)),
            ("glm-4.5-air:latest", Some(Glm)),
            ("zai-org/GLM-4.6", Some(Glm)),
            ("devstral:24b", Some(Mistral)),
            ("mistral-large-latest", Some(Mistral)),
            ("codestral", Some(Mistral)),
            ("llama3.2", Some(Muse)),
            ("muse-spark", Some(Muse)),
            ("olmo2", None),
            ("phi4", None),
            ("", None),
        ],
    );
}

#[test]
fn provider_model_prefers_the_maker() {
    assert_eq!(
        AgentLogo::for_provider_model("ollama", "qwen3-coder:30b"),
        Some(Qwen)
    );
    assert_eq!(
        AgentLogo::for_provider_model("ollama", "my-finetune"),
        Some(Ollama)
    );
    assert_eq!(
        AgentLogo::for_provider_model("openrouter", "deepseek/deepseek-chat"),
        Some(DeepSeek)
    );
    assert_eq!(AgentLogo::for_provider_model("openrouter", "x"), None);
}

#[test]
fn every_logo_has_light_and_dark_png() {
    for logo in AgentLogo::ALL {
        let (light, dark) = logo.png();
        for bytes in [light, dark] {
            assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "{}", logo.name());
            // 128×128: the IHDR width and height.
            assert_eq!(
                &bytes[16..24],
                &[0, 0, 0, 128, 0, 0, 0, 128],
                "{}",
                logo.name()
            );
        }
    }
    let mut names: Vec<_> = AgentLogo::ALL.iter().map(|l| l.name()).collect();
    names.dedup();
    assert_eq!(names.len(), AgentLogo::ALL.len());
}
