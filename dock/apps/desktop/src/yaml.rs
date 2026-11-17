//! YAML syntax highlighting for the compose file editor: tree-sitter's YAML
//! grammar and highlight query, flattened into sorted, non-overlapping spans
//! named like theme syntax styles (`property`, `string`, `comment`…).

use std::ops::Range;
use std::sync::OnceLock;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

/// Capture names of the YAML query that get a colour.
const NAMES: [&str; 13] = [
    "boolean",
    "comment",
    "constant",
    "constant.builtin",
    "label",
    "number",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "type",
];

fn config() -> Option<&'static HighlightConfiguration> {
    static CONFIG: OnceLock<Option<HighlightConfiguration>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let mut config = HighlightConfiguration::new(
                tree_sitter_yaml::LANGUAGE.into(),
                "yaml",
                tree_sitter_yaml::HIGHLIGHTS_QUERY,
                "",
                "",
            )
            .ok()?;
            config.configure(&NAMES);
            Some(config)
        })
        .as_ref()
}

/// Highlighted byte ranges of `text` with their style name; none when the
/// text cannot be parsed.
pub fn spans(text: &str) -> Vec<(Range<usize>, &'static str)> {
    let Some(config) = config() else {
        return Vec::new();
    };
    let mut highlighter = Highlighter::new();
    let Ok(events) = highlighter.highlight(config, text.as_bytes(), None, |_| None) else {
        return Vec::new();
    };
    let mut spans: Vec<(Range<usize>, &'static str)> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for event in events {
        match event {
            Ok(HighlightEvent::HighlightStart(h)) => stack.push(h.0),
            Ok(HighlightEvent::HighlightEnd) => {
                stack.pop();
            }
            Ok(HighlightEvent::Source { start, end }) => {
                let Some(name) = stack.last().and_then(|i| NAMES.get(*i)) else {
                    continue;
                };
                match spans.last_mut() {
                    Some((range, last)) if range.end == start && last == name => range.end = end,
                    _ if start < end => spans.push((start..end, name)),
                    _ => {}
                }
            }
            Err(_) => return Vec::new(),
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_keys_values_and_comments() {
        let text = "# shop\nservices:\n  web:\n    image: \"nginx:1.27\"\n    ports: [8080]\n    restart: true\n";
        let spans = spans(text);
        let named = |name: &str| -> Vec<&str> {
            spans
                .iter()
                .filter(|(_, n)| *n == name)
                .map(|(r, _)| &text[r.clone()])
                .collect()
        };
        assert_eq!(named("comment"), ["# shop"]);
        assert!(named("property").contains(&"services"), "{spans:?}");
        assert!(named("property").contains(&"image"));
        assert!(named("string").contains(&"\"nginx:1.27\""));
        assert!(named("number").contains(&"8080"));
        assert!(named("boolean").contains(&"true"));
        // Sorted and non-overlapping, as the text field requires.
        assert!(spans.windows(2).all(|w| w[0].0.end <= w[1].0.start));
    }
}
