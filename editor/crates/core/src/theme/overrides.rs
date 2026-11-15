//! `~/.ferrum/editor/theme.toml`: token overrides applied on top of
//! whichever theme is selected — for every theme (`[ui]`, `[syntax]`,
//! `[terminal]`) or one theme (`[themes.<name>.ui]` …).

use std::collections::BTreeMap;

use super::{Color, Section, ThemeTokens};

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
                "ui" | "syntax" | "terminal" => {
                    read_section(key, item, &mut out.all, &mut warnings, "")
                }
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

/// Reads one `[ui]` / `[syntax]` / `[terminal]` table into `tokens`.
pub(crate) fn read_section(
    section: &str,
    item: &toml::Value,
    tokens: &mut ThemeTokens,
    warnings: &mut Vec<String>,
    theme: &str,
) {
    let where_ = if theme.is_empty() {
        String::new()
    } else {
        format!(" (theme {theme})")
    };
    let Some(kind) = Section::parse(section) else {
        warnings.push(format!("unknown section `{section}`{where_}"));
        return;
    };
    let Some(table) = item.as_table() else {
        warnings.push(format!("`{section}` must be a table"));
        return;
    };
    for (token, value) in table {
        let parsed = value
            .as_str()
            .ok_or_else(|| "colours are strings".to_string())
            .and_then(Color::parse);
        match parsed {
            Err(error) => warnings.push(format!("{section}.{token}{where_}: {error}")),
            Ok(_) if !kind.accepts(token) => {
                warnings.push(format!("unknown {section} token `{token}`{where_}"));
            }
            Ok(color) => {
                tokens.map_mut(kind).insert(token.clone(), color);
            }
        }
    }
}

/// Default `theme.toml` written on first launch.
pub const DEFAULT_THEME_TOML: &str = r##"# FerrumIDE theme overrides. Pick the theme in config.toml (theme = "…")
# or with "Theme: Select"; override any token of it here. Whole themes live
# in the themes/ folder next to this file (see ferrum-dark.example.toml).
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
# Terminal tokens: background foreground cursor selection, and the ANSI
#   palette black red green yellow blue magenta cyan white plus bright_<name>.

[ui]
# accent = "#e8743b"

[syntax]
# keyword = "#d46be0"

[terminal]
# red = "#ff5f87"

# [themes.ferrum-light.ui]
# editor = "#ffffff"
"##;

#[cfg(test)]
mod tests {
    use super::super::Theme;
    use super::*;

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
            [terminal]
            red = "#ff1111"
            pink = "#ff00ff"
            [themes.ferrum-light.ui]
            editor = "#ffffff"
            "##,
        )
        .expect("valid toml");
        assert_eq!(warnings.len(), 3, "{warnings:?}");
        let mut dark = Theme::builtin("ferrum-dark").expect("builtin");
        dark.apply(&overrides);
        assert_eq!(dark.ui("accent").to_hex(), "#ff0000");
        assert_eq!(dark.terminal("red").to_hex(), "#ff1111");
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
