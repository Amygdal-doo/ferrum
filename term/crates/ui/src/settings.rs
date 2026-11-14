//! FerrumTerm's terminal settings (Settings › Terminal), kept in
//! `~/.ferrum/ferrumterm/settings.json` and shared by every place a
//! terminal is shown: the FerrumTerm window and the panes embedded in
//! Ferrum Editor and FerrumAgentic.
//!
//! Held in a process-wide cell so non-UI code (key translation, spawning)
//! reads them without an `App`.

use std::sync::RwLock;

use ferrum_ui::prefs;
use ferrumterm_protocol::CursorShape;
use serde::{Deserialize, Serialize};

/// Cursor shape: the program's choice, or one forced for every program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorStyle {
    #[default]
    Block,
    Beam,
    Underline,
}

impl CursorStyle {
    pub const ALL: [Self; 3] = [Self::Block, Self::Beam, Self::Underline];

    pub fn label(self) -> &'static str {
        match self {
            Self::Block => "Block",
            Self::Beam => "Bar",
            Self::Underline => "Underline",
        }
    }

    fn shape(self) -> CursorShape {
        match self {
            Self::Block => CursorShape::Block,
            Self::Beam => CursorShape::Beam,
            Self::Underline => CursorShape::Underline,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Blink {
    #[default]
    Off,
    Slow,
    Normal,
    Fast,
}

impl Blink {
    pub const ALL: [Self; 4] = [Self::Off, Self::Slow, Self::Normal, Self::Fast];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Slow => "Slow",
            Self::Normal => "Normal",
            Self::Fast => "Fast",
        }
    }

    /// Half period (on or off time), when blinking.
    pub fn interval_ms(self) -> Option<u64> {
        match self {
            Self::Off => None,
            Self::Slow => Some(800),
            Self::Normal => Some(530),
            Self::Fast => Some(300),
        }
    }
}

/// Where the tabs are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabPosition {
    #[default]
    Top,
    Sidebar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TermSettings {
    /// Tabs across the top, or listed in the sidebar.
    pub tab_position: TabPosition,
    /// Show the Agents section in the sidebar.
    pub sidebar_agents: bool,
    /// Show the SSH hosts in the sidebar.
    pub sidebar_hosts: bool,
    pub cursor_style: CursorStyle,
    /// Ignore programs asking for another cursor shape.
    pub force_cursor_style: bool,
    pub cursor_blink: Blink,
    /// Points; the window's default size (⌘0 returns to it).
    pub font_size: f32,
    /// Monospace font family; `None`: the bundled Lilex.
    pub font_family: Option<String>,
    /// Copy the selection to the clipboard when the drag ends.
    pub copy_on_select: bool,
    /// Option sends Meta (Esc-prefixed keys) instead of typing special characters.
    pub option_as_meta: bool,
    /// Program for new terminals; empty: the login shell. Quotes group arguments.
    pub startup_command: String,
    /// Show agent status (working / blocked / done) on tabs.
    pub activity_in_tabs: bool,
}

impl Default for TermSettings {
    fn default() -> Self {
        Self {
            tab_position: TabPosition::Top,
            sidebar_agents: true,
            sidebar_hosts: true,
            cursor_style: CursorStyle::Block,
            force_cursor_style: false,
            cursor_blink: Blink::Off,
            font_size: crate::app::DEFAULT_FONT_PT,
            font_family: None,
            copy_on_select: false,
            option_as_meta: true,
            startup_command: String::new(),
            activity_in_tabs: true,
        }
    }
}

impl TermSettings {
    /// The cursor to draw for a program asking for `requested`.
    pub fn cursor_shape(&self, requested: CursorShape) -> CursorShape {
        if self.force_cursor_style {
            self.cursor_style.shape()
        } else if requested == CursorShape::Block {
            // Programs that don't ask get the user's style.
            self.cursor_style.shape()
        } else {
            requested
        }
    }

    /// `startup_command` split into arguments (empty: login shell).
    pub fn startup_argv(&self) -> Vec<String> {
        split_args(&self.startup_command)
    }
}

static SETTINGS: RwLock<Option<TermSettings>> = RwLock::new(None);

fn file() -> Option<std::path::PathBuf> {
    prefs::path("ferrumterm/settings.json")
}

/// The current settings (read from disk the first time).
pub fn get() -> TermSettings {
    if let Ok(guard) = SETTINGS.read()
        && let Some(settings) = guard.as_ref()
    {
        return settings.clone();
    }
    let loaded: TermSettings = file().map(|f| prefs::read(&f)).unwrap_or_default();
    if let Ok(mut guard) = SETTINGS.write() {
        *guard = Some(loaded.clone());
    }
    loaded
}

/// Changes the settings with `f` and saves them.
pub fn update(f: impl FnOnce(&mut TermSettings)) -> TermSettings {
    let mut settings = get();
    f(&mut settings);
    if let Ok(mut guard) = SETTINGS.write() {
        *guard = Some(settings.clone());
    }
    if let Some(file) = file()
        && let Err(error) = prefs::write(&file, &settings)
    {
        tracing::warn!(%error, "could not save terminal settings");
    }
    settings
}

/// Shell-like word splitting: whitespace separates, single or double
/// quotes group, backslash escapes the next character.
pub fn split_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => {
                quote = Some(c);
                in_word = true;
            }
            (q, '\\') if q != Some('\'') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                    in_word = true;
                }
            }
            (None, c) if c.is_whitespace() => {
                if in_word {
                    args.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            (_, c) => {
                current.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        args.push(current);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_like_a_shell() {
        assert!(split_args("  ").is_empty());
        assert_eq!(split_args("zsh -l"), ["zsh", "-l"]);
        assert_eq!(
            split_args(r#"fish -C "echo hi there" 'a b' c\ d"#),
            ["fish", "-C", "echo hi there", "a b", "c d"]
        );
        assert_eq!(split_args(r#""""#), [""]);
    }

    #[test]
    fn cursor_override() {
        let mut s = TermSettings {
            cursor_style: CursorStyle::Beam,
            ..TermSettings::default()
        };
        assert_eq!(s.cursor_shape(CursorShape::Block), CursorShape::Beam);
        assert_eq!(
            s.cursor_shape(CursorShape::Underline),
            CursorShape::Underline
        );
        s.force_cursor_style = true;
        assert_eq!(s.cursor_shape(CursorShape::Underline), CursorShape::Beam);
    }

    #[test]
    fn older_files_load_with_defaults() {
        let s: TermSettings = serde_json::from_str(r#"{"copy_on_select":true}"#).expect("parse");
        assert!(s.copy_on_select && s.option_as_meta);
        assert_eq!(s.font_size, crate::app::DEFAULT_FONT_PT);
    }
}
