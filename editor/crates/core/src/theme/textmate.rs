//! VS Code `tokenColors` (TextMate scope rules) → tree-sitter highlight
//! names. Each highlight name lists TextMate scopes in preference order;
//! the first scope some rule matches decides its colour. A rule selector
//! matches a scope when it is a dot-segment prefix of it (`keyword`
//! matches `keyword.control`); the longest selector wins, later rules win
//! ties. Descendant (`a b`) and exclusion (`a - b`) selectors are skipped.

use serde_json::Value;

use super::Color;

/// Highlight name → TextMate scopes, most specific first.
pub(super) const SCOPE_MAP: &[(&str, &[&str])] = &[
    (
        "attribute",
        &["entity.other.attribute-name", "meta.attribute"],
    ),
    (
        "boolean",
        &["constant.language.boolean", "constant.language"],
    ),
    ("character", &["constant.character", "string"]),
    ("comment", &["comment.line", "comment.block", "comment"]),
    (
        "constant",
        &["variable.other.constant", "constant.other", "constant"],
    ),
    ("constant.builtin", &["constant.language"]),
    (
        "constructor",
        &[
            "entity.name.function.constructor",
            "entity.name.type.class",
            "support.class",
        ],
    ),
    ("embedded", &["meta.embedded", "source"]),
    ("escape", &["constant.character.escape"]),
    (
        "function",
        &[
            "entity.name.function",
            "support.function",
            "meta.function-call",
        ],
    ),
    (
        "function.builtin",
        &["support.function.builtin", "support.function"],
    ),
    (
        "function.macro",
        &[
            "entity.name.function.macro",
            "support.function.macro",
            "entity.name.function",
        ],
    ),
    (
        "function.method",
        &[
            "entity.name.function.member",
            "entity.name.function.method",
            "entity.name.function",
        ],
    ),
    (
        "keyword",
        &[
            "keyword.control",
            "keyword",
            "storage.type",
            "storage.modifier",
        ],
    ),
    ("label", &["entity.name.label", "entity.name.tag"]),
    (
        "module",
        &[
            "entity.name.namespace",
            "entity.name.module",
            "entity.name.type.module",
            "entity.name.type",
        ],
    ),
    ("number", &["constant.numeric"]),
    ("operator", &["keyword.operator"]),
    (
        "property",
        &[
            "variable.other.property",
            "variable.other.member",
            "support.type.property-name",
            "meta.object-literal.key",
        ],
    ),
    ("punctuation", &["punctuation"]),
    (
        "punctuation.bracket",
        &["punctuation.section", "punctuation.brackets", "punctuation"],
    ),
    (
        "punctuation.delimiter",
        &[
            "punctuation.separator",
            "punctuation.terminator",
            "punctuation",
        ],
    ),
    (
        "punctuation.special",
        &[
            "punctuation.section.embedded",
            "punctuation.definition.template-expression",
            "punctuation",
        ],
    ),
    ("string", &["string.quoted", "string"]),
    ("string.escape", &["constant.character.escape"]),
    (
        "string.special",
        &["string.regexp", "string.other", "string"],
    ),
    ("tag", &["entity.name.tag"]),
    (
        "type",
        &["entity.name.type", "support.type", "storage.type"],
    ),
    (
        "type.builtin",
        &[
            "support.type.primitive",
            "storage.type.primitive",
            "support.type",
        ],
    ),
    (
        "variable",
        &["variable.other.readwrite", "variable.other", "variable"],
    ),
    ("variable.builtin", &["variable.language"]),
    ("variable.parameter", &["variable.parameter"]),
    ("text.title", &["markup.heading", "entity.name.section"]),
    ("markup.heading", &["markup.heading", "entity.name.section"]),
    (
        "text.literal",
        &["markup.inline.raw", "markup.fenced_code", "markup.raw"],
    ),
    (
        "markup.raw",
        &["markup.inline.raw", "markup.fenced_code", "markup.raw"],
    ),
    ("text.uri", &["markup.underline.link"]),
    (
        "markup.link",
        &["markup.underline.link", "string.other.link"],
    ),
    ("text.reference", &["string.other.link", "markup.link"]),
    ("text.emphasis", &["markup.italic"]),
    ("markup.italic", &["markup.italic"]),
    ("text.strong", &["markup.bold"]),
    ("markup.bold", &["markup.bold"]),
    (
        "markup.list",
        &["punctuation.definition.list", "markup.list"],
    ),
    ("markup.quote", &["markup.quote"]),
];

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Rule {
    selector: String,
    color: Color,
}

/// Flattens `tokenColors` into one rule per simple selector.
pub(super) fn rules(token_colors: &Value, warnings: &mut Vec<String>) -> Vec<Rule> {
    let mut out = Vec::new();
    for entry in token_colors.as_array().into_iter().flatten() {
        let Some(foreground) = entry
            .pointer("/settings/foreground")
            .and_then(Value::as_str)
        else {
            continue;
        };
        let color = match Color::parse(foreground) {
            Ok(color) => color,
            Err(error) => {
                warnings.push(format!("tokenColors: {error}"));
                continue;
            }
        };
        let selectors: Vec<&str> = match entry.get("scope") {
            Some(Value::String(s)) => s.split(',').collect(),
            Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect(),
            _ => continue,
        };
        for selector in selectors {
            let selector = selector.trim();
            if selector.is_empty() || selector.contains(' ') {
                continue;
            }
            out.push(Rule {
                selector: selector.to_string(),
                color,
            });
        }
    }
    out
}

/// Whether `selector` is a dot-segment prefix of `scope`.
fn matches(selector: &str, scope: &str) -> bool {
    scope == selector
        || scope
            .strip_prefix(selector)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// The colour TextMate would give `scope`.
pub(super) fn color_for(rules: &[Rule], scope: &str) -> Option<Color> {
    rules
        .iter()
        .enumerate()
        .filter(|(_, r)| matches(&r.selector, scope))
        .max_by_key(|(i, r)| (r.selector.split('.').count(), *i))
        .map(|(_, r)| r.color)
}

/// Highlight-name colours derived from `rules`.
pub(super) fn syntax_colors(rules: &[Rule]) -> Vec<(&'static str, Color)> {
    SCOPE_MAP
        .iter()
        .filter_map(|(name, scopes)| {
            scopes
                .iter()
                .find_map(|scope| color_for(rules, scope))
                .map(|color| (*name, color))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Rule> {
        let json = serde_json::json!([
            { "scope": "keyword", "settings": { "foreground": "#111111" } },
            { "scope": ["keyword.control", "storage.type"], "settings": { "foreground": "#222222" } },
            { "scope": "comment, punctuation.definition.comment", "settings": { "foreground": "#333333", "fontStyle": "italic" } },
            { "scope": "source.rust entity.name.function", "settings": { "foreground": "#444444" } },
            { "scope": "string", "settings": { "fontStyle": "bold" } },
            { "settings": { "foreground": "#999999" } },
            { "scope": "constant.numeric", "settings": { "foreground": "nope" } }
        ]);
        let mut warnings = Vec::new();
        let rules = rules(&json, &mut warnings);
        assert_eq!(warnings.len(), 1);
        rules
    }

    #[test]
    fn longest_prefix_selector_wins() {
        let rules = sample();
        assert_eq!(
            color_for(&rules, "keyword.control.rust").map(Color::to_hex),
            Some("#222222".into())
        );
        assert_eq!(
            color_for(&rules, "keyword.operator").map(Color::to_hex),
            Some("#111111".into())
        );
        assert_eq!(color_for(&rules, "keywords"), None);
        assert_eq!(
            color_for(&rules, "entity.name.function"),
            None,
            "descendant selector skipped"
        );
    }

    #[test]
    fn maps_onto_highlight_names() {
        let colors = syntax_colors(&sample());
        let get = |name: &str| {
            colors
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, c)| c.to_hex())
        };
        assert_eq!(get("keyword").as_deref(), Some("#222222"));
        assert_eq!(get("comment").as_deref(), Some("#333333"));
        assert_eq!(
            get("type").as_deref(),
            Some("#222222"),
            "storage.type fallback"
        );
        assert_eq!(get("operator").as_deref(), Some("#111111"));
        assert_eq!(get("string"), None);
    }
}
