//! Colour themes as named tokens. Two built-ins (`ferrum-dark`,
//! `ferrum-light`) can be overridden token by token from
//! `~/.ferrum/editor/theme.toml`:
//!
//! ```toml
//! [ui]                          # applies to every theme
//! accent = "#e8743b"
//! [syntax]
//! keyword = "#d46be0"
//! [themes.ferrum-light.syntax]  # applies to one theme only
//! comment = "#8a8075"
//! ```

use std::collections::BTreeMap;

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

    pub fn to_hex(self) -> String {
        if self.a == 0xff {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
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

const DARK_UI: &[(&str, &str)] = &[
    ("background", "#21252b"),
    ("panel", "#21252b"),
    ("surface", "#2b3038"),
    ("editor", "#282c33"),
    ("gutter", "#282c33"),
    ("border", "#363c46"),
    ("border_focused", "#e8743b"),
    ("text", "#dce0e5"),
    ("text_muted", "#a9afbc"),
    ("text_placeholder", "#6b717d"),
    ("accent", "#e8743b"),
    ("element_hover", "#2f343e"),
    ("element_selected", "#3a404b"),
    ("tab_active", "#282c33"),
    ("tab_inactive", "#21252b"),
    ("tab_bar", "#21252b"),
    ("status_bar", "#21252b"),
    ("title_bar", "#21252b"),
    ("scrollbar", "#ffffff22"),
    ("selection", "#e8743b38"),
    ("cursor", "#e8743b"),
    ("current_line", "#2d323a"),
    ("line_number", "#5d636f"),
    ("line_number_active", "#d0d4da"),
    ("indent_guide", "#353a43"),
    ("indent_guide_active", "#4f5561"),
    ("bracket_match", "#9aa2b1"),
    ("find_match", "#f0b43c40"),
    ("find_match_active", "#f0b43c90"),
    ("git_added", "#7cc85a"),
    ("git_modified", "#5b8def"),
    ("git_deleted", "#e85a7e"),
    ("error", "#e85a7e"),
    ("warning", "#f0b43c"),
    ("success", "#7cc85a"),
];

const DARK_SYNTAX: &[(&str, &str)] = &[
    ("attribute", "#f0b43c"),
    ("boolean", "#e8935f"),
    ("character", "#a1c181"),
    ("comment", "#6f7683"),
    ("constant", "#e8935f"),
    ("constructor", "#73ade9"),
    ("embedded", "#dce0e5"),
    ("escape", "#3fb8a0"),
    ("function", "#73ade9"),
    ("function.macro", "#3fb8a0"),
    ("keyword", "#d46be0"),
    ("label", "#73ade9"),
    ("module", "#e8743b"),
    ("number", "#e8935f"),
    ("operator", "#9fb4cc"),
    ("property", "#e8a87c"),
    ("punctuation", "#9aa2b1"),
    ("punctuation.special", "#d46be0"),
    ("string", "#a1c181"),
    ("string.special", "#3fb8a0"),
    ("tag", "#5b8def"),
    ("type", "#6ec2c8"),
    ("variable", "#dce0e5"),
    ("variable.builtin", "#e85a7e"),
    ("variable.parameter", "#e8c77c"),
    ("text.title", "#e8743b"),
    ("markup.heading", "#e8743b"),
    ("text.literal", "#a1c181"),
    ("markup.raw", "#a1c181"),
    ("text.uri", "#38bdf8"),
    ("markup.link", "#38bdf8"),
    ("text.reference", "#73ade9"),
    ("text.emphasis", "#dce0e5"),
    ("markup.italic", "#dce0e5"),
    ("text.strong", "#f1e9df"),
    ("markup.bold", "#f1e9df"),
    ("markup.list", "#e8743b"),
    ("markup.quote", "#8b929e"),
];

const LIGHT_UI: &[(&str, &str)] = &[
    ("background", "#f1eee9"),
    ("panel", "#ebe7e1"),
    ("surface", "#ffffff"),
    ("editor", "#fbfaf8"),
    ("gutter", "#fbfaf8"),
    ("border", "#d9d3ca"),
    ("border_focused", "#d0602a"),
    ("text", "#2b241c"),
    ("text_muted", "#6b6259"),
    ("text_placeholder", "#a39a90"),
    ("accent", "#d0602a"),
    ("element_hover", "#e4dfd7"),
    ("element_selected", "#dad2c6"),
    ("tab_active", "#fbfaf8"),
    ("tab_inactive", "#ebe7e1"),
    ("tab_bar", "#ebe7e1"),
    ("status_bar", "#ebe7e1"),
    ("title_bar", "#ebe7e1"),
    ("scrollbar", "#00000024"),
    ("selection", "#e8743b33"),
    ("cursor", "#d0602a"),
    ("current_line", "#f3efe9"),
    ("line_number", "#b3aaa0"),
    ("line_number_active", "#2b241c"),
    ("indent_guide", "#ebe5dd"),
    ("indent_guide_active", "#cfc7bc"),
    ("bracket_match", "#8a8075"),
    ("find_match", "#f0b43c55"),
    ("find_match_active", "#f0b43caa"),
    ("git_added", "#3f9a3a"),
    ("git_modified", "#3b6fd6"),
    ("git_deleted", "#d0405e"),
    ("error", "#c8364f"),
    ("warning", "#b8860b"),
    ("success", "#3f9a3a"),
];

const LIGHT_SYNTAX: &[(&str, &str)] = &[
    ("attribute", "#b76b01"),
    ("boolean", "#b76b01"),
    ("character", "#50a14f"),
    ("comment", "#9a9087"),
    ("constant", "#b76b01"),
    ("constructor", "#4078f2"),
    ("embedded", "#2b241c"),
    ("escape", "#0e8a8a"),
    ("function", "#4078f2"),
    ("function.macro", "#0e8a8a"),
    ("keyword", "#a626a4"),
    ("label", "#4078f2"),
    ("module", "#c0501f"),
    ("number", "#b76b01"),
    ("operator", "#4f4740"),
    ("property", "#9b4f1a"),
    ("punctuation", "#6b6259"),
    ("punctuation.special", "#a626a4"),
    ("string", "#50a14f"),
    ("string.special", "#0e8a8a"),
    ("tag", "#e45649"),
    ("type", "#0f7f86"),
    ("variable", "#2b241c"),
    ("variable.builtin", "#e45649"),
    ("variable.parameter", "#8a5a00"),
    ("text.title", "#c0501f"),
    ("markup.heading", "#c0501f"),
    ("text.literal", "#50a14f"),
    ("markup.raw", "#50a14f"),
    ("text.uri", "#4078f2"),
    ("markup.link", "#4078f2"),
    ("text.reference", "#4078f2"),
    ("text.emphasis", "#2b241c"),
    ("markup.italic", "#2b241c"),
    ("text.strong", "#1a140e"),
    ("markup.bold", "#1a140e"),
    ("markup.list", "#c0501f"),
    ("markup.quote", "#7d746b"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: String,
    pub dark: bool,
    ui: BTreeMap<String, Color>,
    syntax: BTreeMap<String, Color>,
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
        Self {
            name: "ferrum-dark".into(),
            dark: true,
            ui: table(DARK_UI),
            syntax: table(DARK_SYNTAX),
        }
    }

    pub fn builtin(name: &str) -> Option<Self> {
        let (dark, ui, syntax) = match name {
            "ferrum-dark" | "dark" => (true, DARK_UI, DARK_SYNTAX),
            "ferrum-light" | "light" => (false, LIGHT_UI, LIGHT_SYNTAX),
            _ => return None,
        };
        Some(Self {
            name: if dark { "ferrum-dark" } else { "ferrum-light" }.to_string(),
            dark,
            ui: table(ui),
            syntax: table(syntax),
        })
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

    /// Applies `overrides` (validated colours) for this theme.
    pub fn apply(&mut self, overrides: &ThemeOverrides) {
        for tokens in [Some(&overrides.all), overrides.per_theme.get(&self.name)]
            .into_iter()
            .flatten()
        {
            self.ui
                .extend(tokens.ui.iter().map(|(k, v)| (k.clone(), *v)));
            self.syntax
                .extend(tokens.syntax.iter().map(|(k, v)| (k.clone(), *v)));
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemeTokens {
    pub ui: BTreeMap<String, Color>,
    pub syntax: BTreeMap<String, Color>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemeOverrides {
    pub all: ThemeTokens,
    pub per_theme: BTreeMap<String, ThemeTokens>,
}

impl ThemeOverrides {
    /// Parses `theme.toml`. Bad colours and unknown UI tokens are skipped
    /// and reported as warnings.
    pub fn parse(source: &str) -> Result<(Self, Vec<String>), String> {
        let value: toml::Table = toml::from_str(source).map_err(|e| e.message().to_string())?;
        let mut warnings = Vec::new();
        let mut out = Self::default();
        for (key, item) in &value {
            match key.as_str() {
                "ui" | "syntax" => read_section(key, item, &mut out.all, &mut warnings, ""),
                "themes" => {
                    let Some(themes) = item.as_table() else {
                        warnings.push("`themes` must be a table".into());
                        continue;
                    };
                    for (theme, sections) in themes {
                        let tokens = out.per_theme.entry(theme.clone()).or_default();
                        for (section, item) in sections.as_table().into_iter().flatten() {
                            read_section(section, item, tokens, &mut warnings, theme);
                        }
                    }
                }
                other => warnings.push(format!("unknown section `{other}` in theme.toml")),
            }
        }
        Ok((out, warnings))
    }
}

fn read_section(
    section: &str,
    item: &toml::Value,
    tokens: &mut ThemeTokens,
    warnings: &mut Vec<String>,
    theme: &str,
) {
    let Some(table) = item.as_table() else {
        warnings.push(format!("`{section}` must be a table"));
        return;
    };
    let where_ = if theme.is_empty() {
        String::new()
    } else {
        format!(" (theme {theme})")
    };
    for (token, value) in table {
        let parsed = value
            .as_str()
            .ok_or_else(|| "colours are strings".to_string())
            .and_then(Color::parse);
        match (section, parsed) {
            (_, Err(error)) => warnings.push(format!("{section}.{token}{where_}: {error}")),
            ("ui", Ok(_)) if !UI_TOKENS.contains(&token.as_str()) => {
                warnings.push(format!("unknown ui token `{token}`{where_}"));
            }
            ("ui", Ok(color)) => {
                tokens.ui.insert(token.clone(), color);
            }
            ("syntax", Ok(color)) => {
                tokens.syntax.insert(token.clone(), color);
            }
            (other, Ok(_)) => warnings.push(format!("unknown section `{other}`{where_}")),
        }
    }
}

/// Default `theme.toml` written on first launch.
pub const DEFAULT_THEME_TOML: &str = r##"# Ferrum Editor theme overrides. Pick the base theme in config.toml
# (theme = "ferrum-dark" or "ferrum-light"); override any token here.
# Changes apply as soon as you save. Colours: #rgb, #rrggbb or #rrggbbaa.
#
# UI tokens: background panel surface editor gutter border border_focused text
#   text_muted text_placeholder accent element_hover element_selected tab_active
#   tab_inactive tab_bar status_bar title_bar scrollbar selection cursor
#   current_line line_number line_number_active indent_guide indent_guide_active
#   bracket_match find_match find_match_active git_added git_modified git_deleted
#   error warning success
# Syntax tokens are tree-sitter capture names, matched most-specific first:
#   keyword string comment function function.macro type constant number boolean
#   property variable variable.builtin variable.parameter operator punctuation
#   tag attribute module escape label constructor markup.heading markup.link
#   markup.raw markup.bold markup.italic markup.list markup.quote …

[ui]
# accent = "#e8743b"

[syntax]
# keyword = "#d46be0"

# [themes.ferrum-light.ui]
# editor = "#ffffff"
"##;

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
    }

    #[test]
    fn builtins_define_every_ui_token() {
        for name in BUILTIN_THEMES {
            let theme = Theme::builtin(name).expect("builtin");
            for token in UI_TOKENS {
                assert_ne!(
                    theme.ui(token),
                    Color::from_rgba(0xff00ffff),
                    "{name}.{token}"
                );
            }
        }
        assert!(Theme::builtin("ferrum-dark").is_some_and(|t| t.dark));
        assert!(Theme::builtin("ferrum-light").is_some_and(|t| !t.dark));
        assert!(Theme::builtin("solarized").is_none());
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
    fn overrides_apply_globally_and_per_theme() {
        let (overrides, warnings) = ThemeOverrides::parse(
            r##"
            [ui]
            accent = "#ff0000"
            bogus = "#000"
            [syntax]
            keyword = "#00ff00"
            comment = "red"
            [themes.ferrum-light.ui]
            editor = "#ffffff"
            "##,
        )
        .expect("valid toml");
        assert_eq!(warnings.len(), 2);
        let mut dark = Theme::builtin("ferrum-dark").expect("builtin");
        dark.apply(&overrides);
        assert_eq!(dark.ui("accent").to_hex(), "#ff0000");
        assert_eq!(
            dark.syntax("keyword").map(Color::to_hex),
            Some("#00ff00".into())
        );
        assert_ne!(dark.ui("editor").to_hex(), "#ffffff");
        let mut light = Theme::builtin("ferrum-light").expect("builtin");
        light.apply(&overrides);
        assert_eq!(light.ui("editor").to_hex(), "#ffffff");
    }

    #[test]
    fn default_theme_file_parses_cleanly() {
        let (overrides, warnings) = ThemeOverrides::parse(DEFAULT_THEME_TOML).expect("valid");
        assert!(warnings.is_empty());
        assert_eq!(overrides, ThemeOverrides::default());
    }
}
