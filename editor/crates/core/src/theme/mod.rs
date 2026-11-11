//! Colour themes as named tokens: UI tokens, syntax tokens (tree-sitter
//! highlight names) and terminal tokens (the integrated terminal's ANSI
//! palette). Two built-ins (`ferrum-dark`, `ferrum-light`); more are loaded
//! from `~/.ferrum/editor/themes/` (see [`registry`]): our own TOML format
//! ([`file`]) or VS Code colour themes ([`vscode`]).
//!
//! Any theme can then be overridden token by token from
//! `~/.ferrum/editor/theme.toml`:
//!
//! ```toml
//! [ui]                          # applies to every theme
//! accent = "#e8743b"
//! [syntax]
//! keyword = "#d46be0"
//! [terminal]
//! red = "#ff5555"
//! [themes.ferrum-light.syntax]  # applies to one theme only
//! comment = "#8a8075"
//! ```

mod builtin;
pub mod file;
mod jsonc;
mod overrides;
pub mod registry;
mod textmate;
pub mod vscode;

use std::collections::BTreeMap;

pub(crate) use overrides::read_section;
pub use overrides::{DEFAULT_THEME_TOML, ThemeOverrides};

use builtin::{DARK_SYNTAX, DARK_TERMINAL, DARK_UI, LIGHT_SYNTAX, LIGHT_TERMINAL, LIGHT_UI};

pub const BUILTIN_THEMES: [&str; 2] = ["ferrum-dark", "ferrum-light"];

/// An sRGB colour with alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`.
    pub fn parse(hex: &str) -> Result<Self, String> {
        let digits = hex
            .trim()
            .strip_prefix('#')
            .ok_or_else(|| format!("`{hex}`: colours start with #"))?;
        let expanded: String = match digits.len() {
            3 | 4 => digits.chars().flat_map(|c| [c, c]).collect(),
            6 | 8 => digits.to_string(),
            _ => {
                return Err(format!(
                    "`{hex}`: expected #rgb, #rgba, #rrggbb or #rrggbbaa"
                ));
            }
        };
        let value =
            u32::from_str_radix(&expanded, 16).map_err(|_| format!("`{hex}`: not a hex colour"))?;
        let value = if expanded.len() == 6 {
            (value << 8) | 0xff
        } else {
            value
        };
        Ok(Self::from_rgba(value))
    }

    pub const fn from_rgba(value: u32) -> Self {
        Self {
            r: (value >> 24) as u8,
            g: (value >> 16) as u8,
            b: (value >> 8) as u8,
            a: value as u8,
        }
    }

    pub const fn to_rgba(self) -> u32 {
        ((self.r as u32) << 24) | ((self.g as u32) << 16) | ((self.b as u32) << 8) | self.a as u32
    }

    /// `0xRRGGBB`, alpha dropped.
    pub const fn to_rgb(self) -> u32 {
        self.to_rgba() >> 8
    }

    pub fn to_hex(self) -> String {
        if self.a == 0xff {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    /// Linear mix towards `other` (`t` = 0 → self, 1 → other), opaque.
    pub fn mix(self, other: Color, t: f32) -> Color {
        let lerp = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        Color {
            r: lerp(self.r, other.r),
            g: lerp(self.g, other.g),
            b: lerp(self.b, other.b),
            a: 0xff,
        }
    }
}

/// UI colour tokens every theme defines.
pub const UI_TOKENS: &[&str] = &[
    "background",
    "panel",
    "surface",
    "editor",
    "gutter",
    "border",
    "border_focused",
    "text",
    "text_muted",
    "text_placeholder",
    "accent",
    "element_hover",
    "element_selected",
    "tab_active",
    "tab_inactive",
    "tab_bar",
    "status_bar",
    "title_bar",
    "scrollbar",
    "selection",
    "cursor",
    "current_line",
    "line_number",
    "line_number_active",
    "indent_guide",
    "indent_guide_active",
    "bracket_match",
    "find_match",
    "find_match_active",
    "git_added",
    "git_modified",
    "git_deleted",
    "error",
    "warning",
    "success",
];

/// Terminal tokens: colours of the integrated terminal. The last sixteen
/// are the ANSI palette, in ANSI order (0–15).
pub const TERMINAL_TOKENS: &[&str] = &[
    "background",
    "foreground",
    "cursor",
    "selection",
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "bright_black",
    "bright_red",
    "bright_green",
    "bright_yellow",
    "bright_blue",
    "bright_magenta",
    "bright_cyan",
    "bright_white",
];

/// The kind of token table a name belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Ui,
    Syntax,
    Terminal,
}

impl Section {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "ui" => Some(Self::Ui),
            "syntax" => Some(Self::Syntax),
            "terminal" => Some(Self::Terminal),
            _ => None,
        }
    }

    /// Whether `token` is valid here (syntax accepts any highlight name).
    pub fn accepts(self, token: &str) -> bool {
        match self {
            Self::Ui => UI_TOKENS.contains(&token),
            Self::Terminal => TERMINAL_TOKENS.contains(&token),
            Self::Syntax => !token.is_empty(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: String,
    pub dark: bool,
    ui: BTreeMap<String, Color>,
    syntax: BTreeMap<String, Color>,
    terminal: BTreeMap<String, Color>,
}

fn table(entries: &[(&str, &str)]) -> BTreeMap<String, Color> {
    entries
        .iter()
        .filter_map(|(k, v)| Color::parse(v).ok().map(|c| (k.to_string(), c)))
        .collect()
}

impl Theme {
    /// `ferrum-dark`, the default and the fallback for unknown names.
    pub fn dark() -> Self {
        Self::builtin("ferrum-dark").unwrap_or_else(|| unreachable!("built-in theme"))
    }

    pub fn builtin(name: &str) -> Option<Self> {
        let (dark, ui, syntax, terminal) = match name {
            "ferrum-dark" | "dark" => (true, DARK_UI, DARK_SYNTAX, DARK_TERMINAL),
            "ferrum-light" | "light" => (false, LIGHT_UI, LIGHT_SYNTAX, LIGHT_TERMINAL),
            _ => return None,
        };
        Some(Self {
            name: if dark { "ferrum-dark" } else { "ferrum-light" }.to_string(),
            dark,
            ui: table(ui),
            syntax: table(syntax),
            terminal: table(terminal),
        })
    }

    /// The built-in matching `dark`, renamed to `name`: the base every
    /// loaded theme starts from, so tokens it leaves out stay defined.
    pub fn base(name: &str, dark: bool) -> Self {
        let mut theme = Self::builtin(if dark { "ferrum-dark" } else { "ferrum-light" })
            .unwrap_or_else(Self::dark);
        theme.name = name.to_string();
        theme
    }

    /// A UI token (falls back to magenta for an unknown token, so mistakes
    /// are visible).
    pub fn ui(&self, token: &str) -> Color {
        self.ui
            .get(token)
            .copied()
            .unwrap_or(Color::from_rgba(0xff00ffff))
    }

    /// Colour of a highlight name, trying `a.b.c`, then `a.b`, then `a`.
    pub fn syntax(&self, name: &str) -> Option<Color> {
        let mut key = name;
        loop {
            if let Some(color) = self.syntax.get(key) {
                return Some(*color);
            }
            key = key.rsplit_once('.')?.0;
        }
    }

    /// A terminal token (unknown tokens fall back to the foreground).
    pub fn terminal(&self, token: &str) -> Color {
        self.terminal
            .get(token)
            .or_else(|| self.terminal.get("foreground"))
            .copied()
            .unwrap_or_else(|| self.ui("text"))
    }

    /// ANSI colours 0–15.
    pub fn ansi(&self) -> [Color; 16] {
        std::array::from_fn(|i| self.terminal(TERMINAL_TOKENS[i + 4]))
    }

    /// Sets one token (no validation; see [`Section::accepts`]).
    pub fn set(&mut self, section: Section, token: &str, color: Color) {
        let map = match section {
            Section::Ui => &mut self.ui,
            Section::Syntax => &mut self.syntax,
            Section::Terminal => &mut self.terminal,
        };
        map.insert(token.to_string(), color);
    }

    /// Every token of one section, sorted by name.
    pub fn tokens(&self, section: Section) -> impl Iterator<Item = (&str, Color)> {
        match section {
            Section::Ui => &self.ui,
            Section::Syntax => &self.syntax,
            Section::Terminal => &self.terminal,
        }
        .iter()
        .map(|(k, v)| (k.as_str(), *v))
    }

    /// Applies `overrides` (validated colours) for this theme.
    pub fn apply(&mut self, overrides: &ThemeOverrides) {
        for tokens in [Some(&overrides.all), overrides.per_theme.get(&self.name)]
            .into_iter()
            .flatten()
        {
            self.apply_tokens(tokens);
        }
    }

    pub fn apply_tokens(&mut self, tokens: &ThemeTokens) {
        self.ui
            .extend(tokens.ui.iter().map(|(k, v)| (k.clone(), *v)));
        self.syntax
            .extend(tokens.syntax.iter().map(|(k, v)| (k.clone(), *v)));
        self.terminal
            .extend(tokens.terminal.iter().map(|(k, v)| (k.clone(), *v)));
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemeTokens {
    pub ui: BTreeMap<String, Color>,
    pub syntax: BTreeMap<String, Color>,
    pub terminal: BTreeMap<String, Color>,
}

impl ThemeTokens {
    fn map_mut(&mut self, section: Section) -> &mut BTreeMap<String, Color> {
        match section {
            Section::Ui => &mut self.ui,
            Section::Syntax => &mut self.syntax,
            Section::Terminal => &mut self.terminal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_colours() {
        assert_eq!(Color::parse("#fff").map(Color::to_rgba), Ok(0xffffffff));
        assert_eq!(
            Color::parse("#12345678").map(Color::to_rgba),
            Ok(0x12345678)
        );
        assert_eq!(
            Color::parse("#e8743b").map(Color::to_hex),
            Ok("#e8743b".into())
        );
        assert!(Color::parse("e8743b").is_err());
        assert!(Color::parse("#12345").is_err());
        assert!(Color::parse("#gggggg").is_err());
        assert_eq!(Color::parse("#e8743b").map(Color::to_rgb), Ok(0xe8743b));
    }

    #[test]
    fn builtins_define_every_token() {
        for name in BUILTIN_THEMES {
            let theme = Theme::builtin(name).expect("builtin");
            for token in UI_TOKENS {
                assert_ne!(
                    theme.ui(token),
                    Color::from_rgba(0xff00ffff),
                    "{name}.{token}"
                );
            }
            for token in TERMINAL_TOKENS {
                assert!(
                    theme.tokens(Section::Terminal).any(|(t, _)| t == *token),
                    "{name}.terminal.{token}"
                );
            }
        }
        assert!(Theme::builtin("ferrum-dark").is_some_and(|t| t.dark));
        assert!(Theme::builtin("ferrum-light").is_some_and(|t| !t.dark));
        assert!(Theme::builtin("solarized").is_none());
        let dark = Theme::dark();
        assert_eq!(dark.ansi()[1], dark.terminal("red"));
        assert_eq!(dark.ansi()[15], dark.terminal("bright_white"));
    }

    #[test]
    fn syntax_lookup_falls_back_to_parent_names() {
        let theme = Theme::builtin("ferrum-dark").expect("builtin");
        assert_eq!(
            theme.syntax("function.method.call"),
            theme.syntax("function")
        );
        assert_eq!(theme.syntax("function.macro"), Color::parse("#3fb8a0").ok());
        assert_eq!(theme.syntax("nonsense"), None);
    }

    #[test]
    fn mixes_colours() {
        let black = Color::from_rgba(0x000000ff);
        let white = Color::from_rgba(0xffffffff);
        assert_eq!(black.mix(white, 0.5).to_hex(), "#808080");
    }
}
