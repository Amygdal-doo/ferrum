//! Logos of the coding agents (Claude, Codex, Gemini, OpenCode), shown where
//! one runs (FerrumTerm tabs, FerrumAgentic), and the plain `>_` tile shown
//! for everything else. The SVGs live in `assets/logos/` (see the README
//! there for their sources).

use std::sync::{Arc, OnceLock};

use gpui::{App, Div, Image, ImageFormat, IntoElement, ObjectFit, Pixels, StyledImage, img};
use gpui::{ParentElement, Styled, div};

use crate::components::{Color, Icon, IconName, IconSize};
use crate::theme::{ActiveTheme, Appearance};

/// A coding agent with a logo of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentLogo {
    Claude,
    Codex,
    Gemini,
    OpenCode,
}

macro_rules! logo {
    ($file:literal) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/logos/", $file)).as_slice()
    };
}

/// SVG bytes in [`AgentLogo`] order, then OpenCode for dark themes.
const LOGOS: [&[u8]; 5] = [
    logo!("claude.svg"),
    logo!("codex.svg"),
    logo!("gemini.svg"),
    logo!("opencode-light.svg"),
    logo!("opencode-dark.svg"),
];

impl AgentLogo {
    /// The agent a program runs, by its name (`claude`, `/usr/bin/codex`).
    #[must_use]
    pub fn for_program(program: &str) -> Option<Self> {
        let name = program.rsplit('/').next().unwrap_or(program);
        match name.to_ascii_lowercase().as_str() {
            "claude" | "claude-code" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "gemini" => Some(Self::Gemini),
            "opencode" => Some(Self::OpenCode),
            _ => None,
        }
    }

    /// The agent behind a model id (`claude-sonnet-5-5`, `gpt-5-codex`).
    #[must_use]
    pub fn for_model(model: &str) -> Option<Self> {
        let model = model.to_ascii_lowercase();
        if model.starts_with("claude") {
            Some(Self::Claude)
        } else if model.starts_with("gemini") {
            Some(Self::Gemini)
        } else if model.starts_with("gpt")
            || model.contains("codex")
            || (model.starts_with('o') && model[1..].starts_with(|c: char| c.is_ascii_digit()))
        {
            Some(Self::Codex)
        } else {
            None
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini",
            Self::OpenCode => "OpenCode",
        }
    }

    fn image(self, dark: bool) -> Arc<Image> {
        static IMAGES: OnceLock<Vec<Arc<Image>>> = OnceLock::new();
        let images = IMAGES.get_or_init(|| {
            LOGOS
                .iter()
                .map(|bytes| Arc::new(Image::from_bytes(ImageFormat::Svg, bytes.to_vec())))
                .collect()
        });
        let index = match self {
            Self::OpenCode if dark => 4,
            other => other as usize,
        };
        images[index].clone()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_agents_by_program_and_model() {
        assert_eq!(
            AgentLogo::for_program("/opt/bin/claude"),
            Some(AgentLogo::Claude)
        );
        assert_eq!(AgentLogo::for_program("Codex"), Some(AgentLogo::Codex));
        assert_eq!(AgentLogo::for_program("zsh"), None);
        assert_eq!(
            AgentLogo::for_model("claude-sonnet-5-5"),
            Some(AgentLogo::Claude)
        );
        assert_eq!(
            AgentLogo::for_model("gemini-3-pro"),
            Some(AgentLogo::Gemini)
        );
        assert_eq!(AgentLogo::for_model("gpt-5-codex"), Some(AgentLogo::Codex));
    }

    #[test]
    fn logos_are_svg() {
        for bytes in LOGOS {
            let text = std::str::from_utf8(bytes).expect("utf-8");
            assert!(text.starts_with("<svg") && text.contains("viewBox"));
            assert!(!text.contains("<script"));
        }
    }
}
