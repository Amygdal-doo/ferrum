//! Imports VS Code colour themes (`*.json`, comments and trailing commas
//! allowed): workbench `colors` map onto our UI and terminal tokens,
//! `tokenColors` (TextMate scopes) onto tree-sitter highlight names (see
//! [`super::textmate`]). `include` (a base theme file next to this one) is
//! followed. Anything without a counterpart comes from the built-in of the
//! same appearance. Font styles (bold / italic) are not imported.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{Color, Section, TERMINAL_TOKENS, Theme, jsonc, textmate};

/// Our UI token ← VS Code workbench keys, first present wins.
const UI_MAP: &[(&str, &[&str])] = &[
    (
        "background",
        &[
            "sideBar.background",
            "activityBar.background",
            "editor.background",
        ],
    ),
    (
        "panel",
        &[
            "sideBar.background",
            "panel.background",
            "editor.background",
        ],
    ),
    (
        "surface",
        &[
            "editorWidget.background",
            "quickInput.background",
            "dropdown.background",
            "menu.background",
        ],
    ),
    ("editor", &["editor.background"]),
    ("gutter", &["editorGutter.background", "editor.background"]),
    (
        "border",
        &[
            "editorGroup.border",
            "panel.border",
            "sideBar.border",
            "activityBar.border",
            "tab.border",
            "contrastBorder",
        ],
    ),
    ("border_focused", &["focusBorder"]),
    ("text", &["editor.foreground", "foreground"]),
    (
        "text_muted",
        &[
            "descriptionForeground",
            "tab.inactiveForeground",
            "sideBar.foreground",
        ],
    ),
    (
        "text_placeholder",
        &["input.placeholderForeground", "editorLineNumber.foreground"],
    ),
    (
        "accent",
        &["button.background", "focusBorder", "textLink.foreground"],
    ),
    (
        "element_hover",
        &["list.hoverBackground", "toolbar.hoverBackground"],
    ),
    (
        "element_selected",
        &[
            "list.activeSelectionBackground",
            "list.inactiveSelectionBackground",
        ],
    ),
    ("tab_active", &["tab.activeBackground", "editor.background"]),
    (
        "tab_inactive",
        &["tab.inactiveBackground", "editorGroupHeader.tabsBackground"],
    ),
    (
        "tab_bar",
        &["editorGroupHeader.tabsBackground", "tab.inactiveBackground"],
    ),
    (
        "status_bar",
        &["statusBar.background", "statusBar.noFolderBackground"],
    ),
    (
        "title_bar",
        &[
            "titleBar.activeBackground",
            "activityBar.background",
            "sideBar.background",
        ],
    ),
    ("scrollbar", &["scrollbarSlider.background"]),
    ("selection", &["editor.selectionBackground"]),
    ("cursor", &["editorCursor.foreground"]),
    ("current_line", &["editor.lineHighlightBackground"]),
    ("line_number", &["editorLineNumber.foreground"]),
    (
        "line_number_active",
        &["editorLineNumber.activeForeground", "editor.foreground"],
    ),
    (
        "indent_guide",
        &[
            "editorIndentGuide.background1",
            "editorIndentGuide.background",
        ],
    ),
    (
        "indent_guide_active",
        &[
            "editorIndentGuide.activeBackground1",
            "editorIndentGuide.activeBackground",
        ],
    ),
    ("bracket_match", &["editorBracketMatch.border"]),
    ("find_match", &["editor.findMatchHighlightBackground"]),
    ("find_match_active", &["editor.findMatchBackground"]),
    (
        "git_added",
        &[
            "editorGutter.addedBackground",
            "gitDecoration.addedResourceForeground",
        ],
    ),
    (
        "git_modified",
        &[
            "editorGutter.modifiedBackground",
            "gitDecoration.modifiedResourceForeground",
        ],
    ),
    (
        "git_deleted",
        &[
            "editorGutter.deletedBackground",
            "gitDecoration.deletedResourceForeground",
        ],
    ),
    ("error", &["editorError.foreground", "errorForeground"]),
    (
        "warning",
        &["editorWarning.foreground", "list.warningForeground"],
    ),
    (
        "success",
        &[
            "gitDecoration.addedResourceForeground",
            "terminal.ansiGreen",
        ],
    ),
];

/// Terminal tokens ← VS Code keys (ANSI keys are `terminal.ansi<Name>`).
fn terminal_keys(token: &str) -> Vec<String> {
    match token {
        "background" => vec![
            "terminal.background".into(),
            "panel.background".into(),
            "editor.background".into(),
        ],
        "foreground" => vec!["terminal.foreground".into(), "editor.foreground".into()],
        "cursor" => vec![
            "terminalCursor.foreground".into(),
            "editorCursor.foreground".into(),
        ],
        "selection" => vec![
            "terminal.selectionBackground".into(),
            "editor.selectionBackground".into(),
        ],
        ansi => {
            let camel: String = ansi
                .split('_')
                .map(|w| {
                    let mut c = w.chars();
                    c.next()
                        .map(|f| f.to_uppercase().chain(c).collect::<String>())
                        .unwrap_or_default()
                })
                .collect();
            vec![format!("terminal.ansi{camel}")]
        }
    }
}

/// Highlight names that fall back to the editor foreground when no rule
/// colours them (the built-in colours would clash with the import).
const NEUTRAL: &[&str] = &[
    "variable",
    "embedded",
    "punctuation",
    "property",
    "text.emphasis",
    "markup.italic",
];

struct Merged {
    colors: BTreeMap<String, String>,
    token_colors: Vec<Value>,
}

fn collect(
    value: &Value,
    include: &dyn Fn(&str) -> Option<String>,
    depth: usize,
    warnings: &mut Vec<String>,
) -> Merged {
    let mut merged = Merged {
        colors: BTreeMap::new(),
        token_colors: Vec::new(),
    };
    if let Some(name) = value.get("include").and_then(Value::as_str) {
        match include(name).map(|s| jsonc::parse(&s)) {
            _ if depth >= 4 => warnings.push(format!("include `{name}`: nested too deeply")),
            Some(Ok(base)) => merged = collect(&base, include, depth + 1, warnings),
            Some(Err(error)) => warnings.push(format!("include `{name}`: {error}")),
            None => warnings.push(format!("include `{name}`: file not found")),
        }
    }
    for (key, color) in value
        .get("colors")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        if let Some(color) = color.as_str() {
            merged.colors.insert(key.clone(), color.to_string());
        }
    }
    match value.get("tokenColors") {
        Some(Value::Array(rules)) => merged.token_colors.extend(rules.iter().cloned()),
        Some(Value::String(path)) => {
            warnings.push(format!(
                "tokenColors from `{path}` (a .tmTheme file) are not supported"
            ));
        }
        _ => {}
    }
    merged
}

/// Imports a VS Code theme. `include` reads a file named by `"include"`.
pub fn import(
    source: &str,
    fallback_name: &str,
    include: &dyn Fn(&str) -> Option<String>,
) -> Result<(Theme, Vec<String>), String> {
    let value = jsonc::parse(source)?;
    if !value.is_object() {
        return Err("a VS Code theme is a JSON object".into());
    }
    let mut warnings = Vec::new();
    let merged = collect(&value, include, 0, &mut warnings);
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .filter(|n| !n.trim().is_empty())
        .unwrap_or(fallback_name)
        .trim()
        .to_string();
    let dark = !matches!(
        value.get("type").and_then(Value::as_str),
        Some("light" | "hc-light")
    );
    let mut theme = Theme::base(&name, dark);
    let mut bad = |key: &str, error: String| warnings.push(format!("colors.{key}: {error}"));
    let mut lookup = |keys: &[&str]| -> Option<Color> {
        keys.iter().find_map(|key| {
            let raw = merged.colors.get(*key)?;
            Color::parse(raw).map_err(|e| bad(key, e)).ok()
        })
    };
    let mut found_text_muted = false;
    for (token, keys) in UI_MAP {
        if let Some(color) = lookup(keys) {
            theme.set(Section::Ui, token, color);
            found_text_muted |= *token == "text_muted";
        }
    }
    for token in TERMINAL_TOKENS {
        let keys = terminal_keys(token);
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        if let Some(color) = lookup(&keys) {
            theme.set(Section::Terminal, token, color);
        }
    }
    let text = theme.ui("text");
    let editor = theme.ui("editor");
    if !found_text_muted {
        theme.set(Section::Ui, "text_muted", text.mix(editor, 0.3));
    }
    let rules = textmate::rules(&Value::Array(merged.token_colors), &mut warnings);
    let colors = textmate::syntax_colors(&rules);
    for neutral in NEUTRAL {
        if !colors.iter().any(|(n, _)| n == neutral) {
            theme.set(Section::Syntax, neutral, text);
        }
    }
    for (name, color) in colors {
        theme.set(Section::Syntax, name, color);
    }
    Ok((theme, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    const THEME: &str = r##"{
        // A JSONC theme like the ones VS Code extensions ship.
        "name": "Night Owl Lite",
        "type": "dark",
        "colors": {
            "editor.background": "#011627",
            "editor.foreground": "#d6deeb",
            "editorCursor.foreground": "#80a4c2",
            "focusBorder": "#122d42",
            "statusBar.background": "#011627",
            "terminal.ansiRed": "#ef5350",
            "terminal.ansiBrightBlue": "#82aaff",
            "editor.selectionBackground": "#1d3b53",
            "badColor": "rgb(1,2,3)",
            "list.hoverBackground": "not a colour",
        },
        "tokenColors": [
            { "scope": ["comment", "punctuation.definition.comment"],
              "settings": { "foreground": "#637777", "fontStyle": "italic" } },
            { "scope": "keyword", "settings": { "foreground": "#c792ea" } },
            { "scope": "string.quoted", "settings": { "foreground": "#ecc48d" } },
            { "scope": "entity.name.function", "settings": { "foreground": "#82aaff" } },
            /* trailing comma below */
        ],
    }"##;

    fn no_include(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn maps_workbench_colours_and_token_scopes() {
        let (theme, warnings) = import(THEME, "night-owl", &no_include).expect("valid");
        assert_eq!(theme.name, "Night Owl Lite");
        assert!(theme.dark);
        assert_eq!(theme.ui("editor").to_hex(), "#011627");
        assert_eq!(theme.ui("text").to_hex(), "#d6deeb");
        assert_eq!(theme.ui("cursor").to_hex(), "#80a4c2");
        assert_eq!(theme.ui("accent").to_hex(), "#122d42");
        assert_eq!(theme.ui("selection").to_hex(), "#1d3b53");
        assert_eq!(theme.terminal("red").to_hex(), "#ef5350");
        assert_eq!(theme.terminal("bright_blue").to_hex(), "#82aaff");
        assert_eq!(theme.terminal("background").to_hex(), "#011627");
        let hex = |name: &str| theme.syntax(name).map(Color::to_hex);
        assert_eq!(hex("comment").as_deref(), Some("#637777"));
        assert_eq!(hex("keyword").as_deref(), Some("#c792ea"));
        assert_eq!(hex("string").as_deref(), Some("#ecc48d"));
        assert_eq!(hex("function.method").as_deref(), Some("#82aaff"));
        assert_eq!(
            hex("variable").as_deref(),
            Some("#d6deeb"),
            "neutral → text"
        );
        // "rgb(…)" on an unmapped key is ignored; a bad mapped key warns.
        assert_eq!(
            warnings,
            vec!["colors.list.hoverBackground: `not a colour`: colours start with #".to_string()]
        );
    }

    #[test]
    fn light_type_and_fallback_name() {
        let (theme, _) = import(r##"{"type": "light"}"##, "plain", &no_include).expect("valid");
        assert_eq!(theme.name, "plain");
        assert!(!theme.dark);
        let light = Theme::builtin("ferrum-light").expect("builtin");
        assert_eq!(theme.ui("editor"), light.ui("editor"));
    }

    #[test]
    fn follows_include_and_rejects_garbage() {
        let base = r##"{ "colors": { "editor.background": "#101010", "editor.foreground": "#eeeeee" },
                        "tokenColors": [{ "scope": "keyword", "settings": { "foreground": "#ff0000" } }] }"##;
        let include = |name: &str| (name == "./base.json").then(|| base.to_string());
        let (theme, warnings) = import(
            r##"{ "include": "./base.json", "colors": { "editor.foreground": "#abcdef" } }"##,
            "child",
            &include,
        )
        .expect("valid");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(theme.ui("editor").to_hex(), "#101010");
        assert_eq!(theme.ui("text").to_hex(), "#abcdef");
        assert_eq!(
            theme.syntax("keyword").map(Color::to_hex).as_deref(),
            Some("#ff0000")
        );
        assert!(import("[1, 2]", "x", &no_include).is_err());
        assert!(import("{ nope", "x", &no_include).is_err());
    }
}
