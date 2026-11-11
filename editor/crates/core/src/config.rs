//! `~/.ferrum/editor/config.toml`: editor settings with per-language
//! overrides. Unknown keys are reported as warnings, never fatal.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::editor::EditSettings;
use crate::language::Language;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub font_family: String,
    pub font_size: f32,
    /// Line height as a multiple of the font size.
    pub line_height: f32,
    pub theme: String,
    pub tab_size: usize,
    pub soft_tabs: bool,
    pub soft_wrap: bool,
    pub show_line_numbers: bool,
    pub show_indent_guides: bool,
    pub highlight_current_line: bool,
    pub auto_close_brackets: bool,
    pub auto_indent: bool,
    pub trim_trailing_whitespace: bool,
    pub ensure_final_newline: bool,
    pub scroll_past_end: bool,
    pub restore_session: bool,
    /// Ask before reloading a file that changed on disk while it has
    /// unsaved edits (clean files always reload silently).
    pub confirm_reload_dirty: bool,
    pub languages: BTreeMap<String, LanguageConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LanguageConfig {
    pub tab_size: Option<usize>,
    pub soft_tabs: Option<bool>,
    pub soft_wrap: Option<bool>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font_family: ".ZedMono".into(),
            font_size: 13.0,
            line_height: 1.5,
            theme: "ferrum-dark".into(),
            tab_size: 4,
            soft_tabs: true,
            soft_wrap: false,
            show_line_numbers: true,
            show_indent_guides: true,
            highlight_current_line: true,
            auto_close_brackets: true,
            auto_indent: true,
            trim_trailing_whitespace: false,
            ensure_final_newline: false,
            scroll_past_end: true,
            restore_session: true,
            confirm_reload_dirty: true,
            languages: BTreeMap::new(),
        }
    }
}

const KNOWN_KEYS: &[&str] = &[
    "font_family",
    "font_size",
    "line_height",
    "theme",
    "tab_size",
    "soft_tabs",
    "soft_wrap",
    "show_line_numbers",
    "show_indent_guides",
    "highlight_current_line",
    "auto_close_brackets",
    "auto_indent",
    "trim_trailing_whitespace",
    "ensure_final_newline",
    "scroll_past_end",
    "restore_session",
    "confirm_reload_dirty",
    "languages",
];

impl Config {
    /// Parses and validates; returns the config plus warnings. Values out of
    /// range are clamped (with a warning).
    pub fn parse(source: &str) -> Result<(Self, Vec<String>), String> {
        let table: toml::Table = toml::from_str(source).map_err(|e| e.message().to_string())?;
        let mut warnings: Vec<String> = table
            .keys()
            .filter(|k| !KNOWN_KEYS.contains(&k.as_str()))
            .map(|k| format!("unknown setting `{k}`"))
            .collect();
        let mut config: Config = toml::Value::Table(table)
            .try_into()
            .map_err(|e: toml::de::Error| e.message().to_string())?;
        for language in config.languages.keys() {
            if Language::from_id(language).is_none() {
                warnings.push(format!("unknown language `{language}` in [languages]"));
            }
        }
        if !(6.0..=72.0).contains(&config.font_size) {
            warnings.push(format!("font_size {} out of range 6–72", config.font_size));
            config.font_size = config.font_size.clamp(6.0, 72.0);
        }
        if !(1.0..=3.0).contains(&config.line_height) {
            warnings.push(format!(
                "line_height {} out of range 1–3",
                config.line_height
            ));
            config.line_height = config.line_height.clamp(1.0, 3.0);
        }
        if !(1..=16).contains(&config.tab_size) {
            warnings.push(format!("tab_size {} out of range 1–16", config.tab_size));
            config.tab_size = config.tab_size.clamp(1, 16);
        }
        Ok((config, warnings))
    }

    fn language(&self, language: Language) -> Option<&LanguageConfig> {
        self.languages
            .iter()
            .find(|(k, _)| Language::from_id(k) == Some(language))
            .map(|(_, v)| v)
    }

    pub fn soft_wrap_for(&self, language: Language) -> bool {
        self.language(language)
            .and_then(|l| l.soft_wrap)
            .unwrap_or(self.soft_wrap)
    }

    /// Editing settings for a buffer in `language`.
    pub fn edit_settings(&self, language: Language) -> EditSettings {
        let lang = self.language(language);
        EditSettings {
            tab_size: lang
                .and_then(|l| l.tab_size)
                .unwrap_or(self.tab_size)
                .clamp(1, 16),
            soft_tabs: lang.and_then(|l| l.soft_tabs).unwrap_or(self.soft_tabs),
            auto_close: self.auto_close_brackets,
            auto_indent: self.auto_indent,
        }
    }
}

/// Default `config.toml` written on first launch.
pub const DEFAULT_CONFIG_TOML: &str = r#"# Ferrum Editor settings. Changes apply as soon as you save this file.
# Run "Application: Open Keymap" / "Open Snippets" / "Open Theme" from the
# command palette (Cmd+Shift+P) to customise the rest.

font_family = ".ZedMono"     # any installed monospace font, e.g. "Menlo"
font_size = 13
line_height = 1.5            # multiple of font_size
theme = "ferrum-dark"        # or "ferrum-light"

tab_size = 4
soft_tabs = true             # insert spaces for Tab
soft_wrap = false
show_line_numbers = true
show_indent_guides = true
highlight_current_line = true
auto_close_brackets = true
auto_indent = true
trim_trailing_whitespace = false
ensure_final_newline = false
scroll_past_end = true
restore_session = true
confirm_reload_dirty = true

# Per-language overrides (ids: rust javascript typescript tsx json toml yaml
# markdown python go html css shell text).
[languages.markdown]
soft_wrap = true

[languages.go]
soft_tabs = false

[languages.yaml]
tab_size = 2
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_file_parses_without_warnings() {
        let (config, warnings) = Config::parse(DEFAULT_CONFIG_TOML).expect("valid");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(config.font_size, 13.0);
        assert!(config.soft_wrap_for(Language::Markdown));
        assert!(!config.soft_wrap_for(Language::Rust));
        assert!(!config.edit_settings(Language::Go).soft_tabs);
        assert_eq!(config.edit_settings(Language::Yaml).tab_size, 2);
        assert_eq!(config.edit_settings(Language::Rust).tab_size, 4);
    }

    #[test]
    fn empty_file_gives_defaults() {
        let (config, warnings) = Config::parse("").expect("valid");
        assert_eq!(config, Config::default());
        assert!(warnings.is_empty());
    }

    #[test]
    fn unknown_keys_and_ranges_warn() {
        let (config, warnings) = Config::parse(
            "tab_size = 99\nfont_size = 2\nwat = 1\n[languages.klingon]\ntab_size = 2",
        )
        .expect("valid");
        assert_eq!(config.tab_size, 16);
        assert_eq!(config.font_size, 6.0);
        assert_eq!(warnings.len(), 4);
    }

    #[test]
    fn type_errors_are_fatal() {
        assert!(Config::parse("tab_size = \"four\"").is_err());
        assert!(Config::parse("this is not toml").is_err());
    }

    #[test]
    fn language_aliases_resolve() {
        let (config, _) = Config::parse("[languages.py]\ntab_size = 2").expect("valid");
        assert_eq!(config.edit_settings(Language::Python).tab_size, 2);
    }
}
