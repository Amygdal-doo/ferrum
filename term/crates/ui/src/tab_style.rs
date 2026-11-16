//! The icon and colour of a tab's chip (chosen from the chip's menu, kept
//! per window in `gui-state`). Only the chip is coloured, not the tab.

use crate::sidebar::agents::program_name;
use ferrum_ui::components::IconName;
use ferrum_ui::gpui::{self, Hsla};
use ferrumterm_protocol::WindowInfo;

/// Icons offered for a tab: (key, label, icon).
pub const ICONS: [(&str, &str, IconName); 20] = [
    ("terminal", "Terminal", IconName::Terminal),
    ("code", "Code", IconName::Code),
    ("server", "Server", IconName::Server),
    ("git", "Git", IconName::GitBranch),
    ("agent", "Agent", IconName::Sparkle),
    ("database", "Database", IconName::DatabaseZap),
    ("box", "Container", IconName::Box),
    ("build", "Build", IconName::Hammer),
    ("debug", "Debug", IconName::Debug),
    ("web", "Web", IconName::Public),
    ("rocket", "Deploy", IconName::Rocket),
    ("test", "Tests", IconName::Flask),
    ("cpu", "System", IconName::Cpu),
    ("zap", "Fast", IconName::Zap),
    ("star", "Star", IconName::Star),
    ("flame", "Hot", IconName::Flame),
    ("cloud", "Cloud", IconName::Cloud),
    ("monitor", "Monitor", IconName::Monitor),
    ("heart", "Favourite", IconName::Heart),
    ("logs", "Logs", IconName::File),
];

/// Colours offered for a tab: (key, label, `0xRRGGBB`).
pub const COLORS: [(&str, &str, u32); 9] = [
    ("gray", "Gray", 0x8b8b90),
    ("blue", "Blue", 0x4c8bf5),
    ("green", "Green", 0x3dd68c),
    ("yellow", "Yellow", 0xf0b43c),
    ("orange", "Orange", 0xe8743b),
    ("red", "Red", 0xf2555a),
    ("purple", "Purple", 0xa970ff),
    ("teal", "Teal", 0x3fb8a0),
    ("pink", "Pink", 0xf06ba8),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabStyle {
    /// A key of [`ICONS`].
    pub icon: String,
    /// A key of [`COLORS`].
    pub color: String,
}

impl Default for TabStyle {
    fn default() -> Self {
        Self {
            icon: "terminal".into(),
            color: "gray".into(),
        }
    }
}

impl TabStyle {
    /// The logo of the agent a tab runs (Claude, Codex, Gemini, Qwen…): the
    /// one the server sees in its foreground, else the program it started.
    #[must_use]
    pub fn agent_logo(window: &WindowInfo) -> Option<ferrum_ui::AgentLogo> {
        let block = window
            .blocks
            .iter()
            .find(|b| b.id == window.focused)
            .or(window.blocks.first())?;
        block
            .agent
            .as_deref()
            .and_then(ferrum_ui::AgentLogo::for_program)
            .or_else(|| program_name(&block.command).and_then(ferrum_ui::AgentLogo::for_program))
    }

    /// The default for a tab nobody styled: an agent gets ✳ in orange, an
    /// SSH session a server in blue, a shell `>_` in gray.
    #[must_use]
    pub fn auto(window: &WindowInfo) -> Self {
        let block = window
            .blocks
            .iter()
            .find(|b| b.id == window.focused)
            .or(window.blocks.first());
        let (icon, color) = match block {
            Some(b) if crate::sidebar::agents::runs_agent(b) => ("agent", "orange"),
            Some(b) if program_name(&b.command) == Some("ssh") => ("server", "blue"),
            _ => ("terminal", "gray"),
        };
        Self {
            icon: icon.into(),
            color: color.into(),
        }
    }

    /// From saved keys; unknown keys fall back to the defaults.
    #[must_use]
    pub fn parse(icon: Option<&str>, color: Option<&str>) -> Option<Self> {
        let d = Self::default();
        Some(Self {
            icon: icon
                .filter(|i| ICONS.iter().any(|(k, ..)| k == i))
                .map_or(d.icon, str::to_owned),
            color: color
                .filter(|c| COLORS.iter().any(|(k, ..)| k == c))
                .map_or(d.color, str::to_owned),
        })
    }

    #[must_use]
    pub fn icon_name(&self) -> IconName {
        ICONS
            .iter()
            .find(|(k, ..)| *k == self.icon)
            .map_or(IconName::Terminal, |(.., i)| *i)
    }

    #[must_use]
    pub fn hsla(&self) -> Hsla {
        let rgb = COLORS
            .iter()
            .find(|(k, ..)| *k == self.color)
            .map_or(0x8b8b90, |(.., c)| *c);
        gpui::rgb(rgb).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_keys_fall_back() {
        let s = TabStyle::parse(Some("nope"), Some("red")).expect("style");
        assert_eq!(s.icon, "terminal");
        assert_eq!(s.color, "red");
        assert_eq!(s.icon_name(), IconName::Terminal);
    }
}
