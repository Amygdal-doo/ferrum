//! Ferrum's own theme file format (`~/.ferrum/editor/themes/<name>.toml`):
//!
//! ```toml
//! name = "my-theme"        # optional; defaults to the file name
//! appearance = "dark"      # "dark" or "light": the built-in that fills gaps
//!
//! [ui]                     # window chrome and editor surface
//! accent = "#e8743b"
//! [syntax]                 # tree-sitter highlight names
//! keyword = "#d46be0"
//! [terminal]               # integrated terminal + ANSI palette
//! red = "#e85a7e"
//! ```
//!
//! Every section is optional; a token a theme leaves out comes from
//! `ferrum-dark` or `ferrum-light` (by `appearance`).

use super::{Section, TERMINAL_TOKENS, Theme, ThemeTokens, UI_TOKENS, read_section};

/// Parses a theme file. `fallback_name` (the file stem) names a theme
/// without `name`. Bad tokens become warnings; malformed TOML is an error.
pub fn parse(source: &str, fallback_name: &str) -> Result<(Theme, Vec<String>), String> {
    let table: toml::Table = toml::from_str(source).map_err(|e| e.message().to_string())?;
    let mut warnings = Vec::new();
    let name = match table.get("name") {
        None => fallback_name.to_string(),
        Some(toml::Value::String(name)) if !name.trim().is_empty() => name.trim().to_string(),
        Some(_) => {
            warnings.push("`name` must be a non-empty string".into());
            fallback_name.to_string()
        }
    };
    let dark = match table.get("appearance").and_then(|v| v.as_str()) {
        None | Some("dark") => true,
        Some("light") => false,
        Some(other) => {
            warnings.push(format!("appearance `{other}` (use dark or light)"));
            true
        }
    };
    let mut tokens = ThemeTokens::default();
    for (key, item) in &table {
        match key.as_str() {
            "name" | "appearance" => {}
            "ui" | "syntax" | "terminal" => read_section(key, item, &mut tokens, &mut warnings, ""),
            other => warnings.push(format!("unknown key `{other}`")),
        }
    }
    let mut theme = Theme::base(&name, dark);
    theme.apply_tokens(&tokens);
    Ok((theme, warnings))
}

fn section_doc(section: Section) -> &'static str {
    match section {
        Section::Ui => "# Window chrome and the editor surface.",
        Section::Syntax => {
            "# Tree-sitter highlight names, matched most-specific first\n\
             # (`function.method` falls back to `function`)."
        }
        Section::Terminal => {
            "# The integrated terminal: background, foreground, cursor, selection,\n\
             # then the 16-colour ANSI palette (black … white, bright_black … bright_white)."
        }
    }
}

/// Writes `theme` in this format, documented, with every token.
pub fn to_toml(theme: &Theme) -> String {
    to_toml_with_header(
        theme,
        &format!(
            "# Ferrum Editor theme — a complete copy of the built-in `{}`.\n\
             # Copy this file to a new name (e.g. my-theme.toml), change `name`, edit the\n\
             # colours, then pick it with \"Theme: Select\". Files ending in\n\
             # .example.toml are not loaded. Colours: #rgb, #rrggbb or #rrggbbaa.\n\
             # Leave out any token to inherit it from the built-in named by `appearance`.\n",
            theme.name
        ),
    )
}

/// [`to_toml`] with another comment at the top (e.g. an import's origin).
pub fn to_toml_with_header(theme: &Theme, header: &str) -> String {
    let mut out = String::new();
    out.push_str(header);
    out.push('\n');
    let name = toml::Value::String(theme.name.clone());
    out.push_str(&format!("name = {name}\n"));
    out.push_str(&format!(
        "appearance = \"{}\"\n",
        if theme.dark { "dark" } else { "light" }
    ));
    for (section, header, order) in [
        (Section::Ui, "ui", Some(UI_TOKENS)),
        (Section::Syntax, "syntax", None),
        (Section::Terminal, "terminal", Some(TERMINAL_TOKENS)),
    ] {
        out.push_str(&format!("\n{}\n[{header}]\n", section_doc(section)));
        let tokens: Vec<(&str, super::Color)> = match order {
            Some(order) => order
                .iter()
                .filter_map(|t| theme.tokens(section).find(|(k, _)| k == t))
                .collect(),
            None => theme.tokens(section).collect(),
        };
        for (token, color) in tokens {
            let key = if token.contains('.') {
                format!("\"{token}\"")
            } else {
                token.to_string()
            };
            out.push_str(&format!("{key} = \"{}\"\n", color.to_hex()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tokens_over_the_appearance_base() {
        let (theme, warnings) = parse(
            r##"
            name = "Paper"
            appearance = "light"
            [ui]
            accent = "#112233"
            [syntax]
            "function.macro" = "#445566"
            [terminal]
            red = "#ff0000"
            "##,
            "paper-file",
        )
        .expect("valid");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(theme.name, "Paper");
        assert!(!theme.dark);
        assert_eq!(theme.ui("accent").to_hex(), "#112233");
        assert_eq!(
            theme.syntax("function.macro").map(|c| c.to_hex()),
            Some("#445566".into())
        );
        assert_eq!(theme.terminal("red").to_hex(), "#ff0000");
        let light = Theme::builtin("ferrum-light").expect("builtin");
        assert_eq!(theme.ui("editor"), light.ui("editor"), "inherited");
    }

    #[test]
    fn name_defaults_to_file_and_bad_tokens_warn() {
        let (theme, warnings) = parse(
            "appearance = \"sepia\"\nwat = 1\n[ui]\nnope = \"#000\"\ntext = \"blue\"\n[colors]\n",
            "mine",
        )
        .expect("valid");
        assert_eq!(theme.name, "mine");
        assert!(theme.dark);
        assert_eq!(warnings.len(), 5, "{warnings:?}");
        assert!(parse("[ui", "x").is_err());
    }

    #[test]
    fn builtins_round_trip_through_the_format() {
        for name in super::super::BUILTIN_THEMES {
            let theme = Theme::builtin(name).expect("builtin");
            let (parsed, warnings) = parse(&to_toml(&theme), "x").expect("valid");
            assert!(warnings.is_empty(), "{warnings:?}");
            assert_eq!(parsed, theme);
        }
    }
}
