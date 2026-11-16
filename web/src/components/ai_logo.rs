//! Logos of the coding agents and model makers the apps work with (the same
//! 128-px PNGs as `shared/ui/assets/logos`, copied to `assets/img/ai/`).
//! Black marks have a light twin; CSS shows the one for the theme.
//! The names and logos are their owners' trademarks, shown only to say
//! what Ferrum works with ([`NOTICE`]).

use maud::{Markup, html};

use crate::routes::assets;

/// Shown under a strip of logos.
pub const NOTICE: &str = "Names and logos are trademarks of their owners, shown only to identify \
                          compatible services. No endorsement is implied.";

/// (name, file, file for dark themes when the mark is black).
const LOGOS: &[(&str, &str, Option<&str>)] = &[
    ("Claude", "claude", None),
    ("Anthropic", "claude", None),
    ("Claude Code", "claude", None),
    ("OpenAI", "openai-light", Some("openai-dark")),
    ("Codex", "openai-light", Some("openai-dark")),
    ("ChatGPT", "openai-light", Some("openai-dark")),
    ("Gemini", "gemini", None),
    ("Gemini CLI", "gemini", None),
    ("OpenCode", "opencode-light", Some("opencode-dark")),
    ("DeepSeek", "deepseek", None),
    ("Kimi", "kimi", None),
    ("Qwen", "qwen", None),
    ("Qwen Code", "qwen", None),
    ("GLM", "zai", None),
    ("Mistral", "mistral", None),
    ("Ollama", "ollama-light", Some("ollama-dark")),
    ("Cursor Agent", "cursor-light", Some("cursor-dark")),
    ("Amp", "amp", None),
    ("goose", "goose-light", Some("goose-dark")),
];

fn find(name: &str) -> Option<(&'static str, Option<&'static str>)> {
    LOGOS
        .iter()
        .find(|(n, ..)| *n == name)
        .map(|(_, light, dark)| (*light, *dark))
}

fn src(file: &str) -> String {
    assets::url(&format!("img/ai/{file}.png"))
}

/// `name`'s logo, decorative (the name is always written next to it);
/// nothing for a name without one.
pub fn logo(name: &str) -> Markup {
    let Some((light, dark)) = find(name) else {
        return html! {};
    };
    html! {
        @match dark {
            Some(dark) => {
                img class="ai-logo ai-logo--light-only" src=(src(light)) alt="" width="16" height="16" loading="lazy";
                img class="ai-logo ai-logo--dark-only" src=(src(dark)) alt="" width="16" height="16" loading="lazy";
            }
            None => {
                img class="ai-logo" src=(src(light)) alt="" width="16" height="16" loading="lazy";
            }
        }
    }
}

/// Labelled rows of logo + name chips, then the trademark notice.
pub fn strips(groups: &[(&str, &[&str])]) -> Markup {
    html! {
        div class="ai-strips" {
            @for (label, names) in groups {
                p class="app-groups__label" { (label) }
                ul class="ai-strip" {
                    @for name in *names {
                        li class="ai-strip__item" { (logo(name)) span { (name) } }
                    }
                }
            }
            p class="ai-strips__notice" { (NOTICE) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_logo_file_is_an_asset() {
        for (name, light, dark) in LOGOS {
            for file in std::iter::once(light).chain(dark.iter()) {
                let path = format!("{}/assets/img/ai/{file}.png", env!("CARGO_MANIFEST_DIR"));
                assert!(std::path::Path::new(&path).exists(), "{name}: {path}");
            }
        }
    }

    #[test]
    fn black_marks_get_both_themes_and_unknown_names_nothing() {
        let html = logo("Ollama").into_string();
        assert!(html.contains("ollama-light.png") && html.contains("ollama-dark.png"));
        assert_eq!(logo("aider").into_string(), "");
        let strip = strips(&[("Agents", &["Claude Code", "aider"])]).into_string();
        assert!(strip.contains("claude.png") && strip.contains(">aider<"));
        assert!(strip.contains("trademarks of their owners"));
    }
}
