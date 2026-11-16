//! Logos of the coding agents and model makers (Claude, OpenAI / Codex /
//! ChatGPT, Gemini, OpenCode, Muse, DeepSeek, Kimi, Qwen, GLM, Mistral,
//! Ollama, Cursor, Amp, goose), shown wherever one runs or is chosen
//! (FerrumTerm, FerrumChat, FerrumEngineer, the editor, Settings), and the
//! plain `>_` tile shown for everything else. The 128-pixel PNGs live in
//! `assets/logos/` (see the README there for where each comes from); which
//! program, provider or model gets which logo is in [`mapping`].

mod mapping;
#[cfg(test)]
mod tests;

use std::sync::{Arc, OnceLock};

use gpui::{App, Div, Image, ImageFormat, IntoElement, ObjectFit, Pixels, StyledImage, img};
use gpui::{ParentElement, Styled, div};

use crate::components::{Color, Icon, IconName, IconSize};
use crate::theme::{ActiveTheme, Appearance};

/// A coding agent or model maker with a logo of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentLogo {
    Claude,
    /// OpenAI's Blossom: OpenAI models and ChatGPT.
    OpenAi,
    /// Codex (OpenAI publishes no separate Codex mark: the Blossom).
    Codex,
    Gemini,
    OpenCode,
    Muse,
    DeepSeek,
    Kimi,
    Qwen,
    /// GLM, by Z.ai (Zhipu AI).
    Glm,
    Mistral,
    Ollama,
    Cursor,
    Amp,
    Goose,
}

macro_rules! logo {
    ($file:literal) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/logos/", $file)).as_slice()
    };
}

impl AgentLogo {
    pub const ALL: [Self; 15] = [
        Self::Claude,
        Self::OpenAi,
        Self::Codex,
        Self::Gemini,
        Self::OpenCode,
        Self::Muse,
        Self::DeepSeek,
        Self::Kimi,
        Self::Qwen,
        Self::Glm,
        Self::Mistral,
        Self::Ollama,
        Self::Cursor,
        Self::Amp,
        Self::Goose,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::OpenAi => "OpenAI",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini",
            Self::OpenCode => "OpenCode",
            Self::Muse => "Muse",
            Self::DeepSeek => "DeepSeek",
            Self::Kimi => "Kimi",
            Self::Qwen => "Qwen",
            Self::Glm => "GLM",
            Self::Mistral => "Mistral",
            Self::Ollama => "Ollama",
            Self::Cursor => "Cursor",
            Self::Amp => "Amp",
            Self::Goose => "goose",
        }
    }

    /// PNG bytes for light themes and for dark ones: the same file unless
    /// the mark is black (then its light version on dark themes).
    pub fn png(self) -> (&'static [u8], &'static [u8]) {
        let one = |bytes: &'static [u8]| (bytes, bytes);
        match self {
            Self::Claude => one(logo!("claude.png")),
            Self::OpenAi | Self::Codex => (logo!("openai-light.png"), logo!("openai-dark.png")),
            Self::Gemini => one(logo!("gemini.png")),
            Self::OpenCode => (logo!("opencode-light.png"), logo!("opencode-dark.png")),
            Self::Muse => one(logo!("muse.png")),
            Self::DeepSeek => one(logo!("deepseek.png")),
            Self::Kimi => one(logo!("kimi.png")),
            Self::Qwen => one(logo!("qwen.png")),
            Self::Glm => one(logo!("zai.png")),
            Self::Mistral => one(logo!("mistral.png")),
            Self::Ollama => (logo!("ollama-light.png"), logo!("ollama-dark.png")),
            Self::Cursor => (logo!("cursor-light.png"), logo!("cursor-dark.png")),
            Self::Amp => one(logo!("amp.png")),
            Self::Goose => (logo!("goose-light.png"), logo!("goose-dark.png")),
        }
    }

    fn image(self, dark: bool) -> Arc<Image> {
        static IMAGES: OnceLock<Vec<(Arc<Image>, Arc<Image>)>> = OnceLock::new();
        let images = IMAGES.get_or_init(|| {
            let image =
                |bytes: &[u8]| Arc::new(Image::from_bytes(ImageFormat::Png, bytes.to_vec()));
            Self::ALL
                .iter()
                .map(|logo| {
                    let (light, dark) = logo.png();
                    (image(light), image(dark))
                })
                .collect()
        });
        let index = Self::ALL.iter().position(|l| *l == self).unwrap_or(0);
        let (light, dark_image) = &images[index];
        if dark { dark_image } else { light }.clone()
    }
}

/// The agent's logo, `size` square.
pub fn agent_logo(logo: AgentLogo, size: Pixels, cx: &App) -> impl IntoElement + use<> {
    let dark = cx.theme().appearance() == Appearance::Dark;
    img(logo.image(dark))
        .flex_none()
        .size(size)
        .object_fit(ObjectFit::Contain)
}

/// A plain terminal's mark: `>_` on a small rounded tile.
pub fn terminal_tile(size: Pixels, cx: &App) -> Div {
    let colors = cx.theme().colors();
    div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(size)
        .rounded(size * 0.24)
        .border_1()
        .border_color(colors.text.opacity(0.14))
        .bg(colors.text.opacity(0.08))
        .child(
            Icon::new(IconName::Terminal)
                .size(IconSize::XSmall)
                .color(Color::Muted),
        )
}

/// `logo` at `size`, or the `>_` tile when there is none.
pub fn agent_logo_or_tile(logo: Option<AgentLogo>, size: Pixels, cx: &App) -> gpui::AnyElement {
    match logo {
        Some(logo) => agent_logo(logo, size, cx).into_any_element(),
        None => terminal_tile(size, cx).into_any_element(),
    }
}
