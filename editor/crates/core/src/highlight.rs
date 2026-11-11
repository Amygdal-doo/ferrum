//! Tree-sitter syntax highlighting → flat, sorted, non-overlapping spans.
//! Grammars and their highlight queries are compiled into the binary by the
//! grammar crates (`include_str!`); configurations are built lazily once per
//! language. Markdown injects its inline grammar and fenced code languages;
//! HTML injects JavaScript and CSS.

use std::ops::Range;
use std::sync::OnceLock;
use std::sync::atomic::AtomicUsize;

use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

use crate::language::Language;

/// Capture names we recognise, most specific first is not required:
/// tree-sitter-highlight picks the longest dotted prefix match.
pub const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "boolean",
    "character",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "embedded",
    "escape",
    "function",
    "function.builtin",
    "function.macro",
    "function.method",
    "keyword",
    "label",
    "module",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.escape",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
    "text.title",
    "text.literal",
    "text.uri",
    "text.reference",
    "text.emphasis",
    "text.strong",
    "markup.heading",
    "markup.bold",
    "markup.italic",
    "markup.link",
    "markup.raw",
    "markup.list",
    "markup.quote",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub range: Range<usize>,
    /// Index into [`HIGHLIGHT_NAMES`].
    pub kind: usize,
}

impl HighlightSpan {
    pub fn name(&self) -> &'static str {
        HIGHLIGHT_NAMES.get(self.kind).copied().unwrap_or("text")
    }
}

/// Whether the language has a grammar compiled in.
pub fn supports(language: Language) -> bool {
    config(language).is_some()
}

/// Highlights `source`. Returns no spans for plain text or when cancelled /
/// the parse fails. `cancel` is checked by tree-sitter periodically; set it
/// to non-zero to abort.
pub fn highlight(
    language: Language,
    source: &str,
    cancel: Option<&AtomicUsize>,
) -> Vec<HighlightSpan> {
    let Some(config) = config(language) else {
        return Vec::new();
    };
    let mut highlighter = Highlighter::new();
    // The closure narrows `&'static` to the call's lifetime; passing the fn
    // item directly would demand `'static` source and cancel flag.
    #[allow(clippy::redundant_closure)]
    let Ok(events) = highlighter.highlight(config, source.as_bytes(), cancel, |name| {
        injection_config(name)
    }) else {
        return Vec::new();
    };
    let mut spans: Vec<HighlightSpan> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for event in events {
        let Ok(event) = event else {
            return Vec::new();
        };
        match event {
            HighlightEvent::HighlightStart(highlight) => stack.push(highlight.0),
            HighlightEvent::HighlightEnd => {
                stack.pop();
            }
            HighlightEvent::Source { start, end } => {
                let Some(&kind) = stack.last() else { continue };
                if start >= end {
                    continue;
                }
                match spans.last_mut() {
                    Some(last) if last.range.end == start && last.kind == kind => {
                        last.range.end = end
                    }
                    _ => spans.push(HighlightSpan {
                        range: start..end,
                        kind,
                    }),
                }
            }
        }
    }
    spans
}

fn injection_config(name: &str) -> Option<&'static HighlightConfiguration> {
    match name {
        "markdown_inline" | "markdown-inline" => markdown_inline_config(),
        "js" | "javascript" | "jsx" => config(Language::JavaScript),
        _ => config(Language::from_id(name)?),
    }
}

type Slot = OnceLock<Option<HighlightConfiguration>>;

fn slot(language: Language) -> Option<&'static Slot> {
    static SLOTS: [Slot; 14] = [const { OnceLock::new() }; 14];
    let index = Language::ALL.iter().position(|l| *l == language)?;
    SLOTS.get(index)
}

fn config(language: Language) -> Option<&'static HighlightConfiguration> {
    if language == Language::PlainText {
        return None;
    }
    slot(language)?.get_or_init(|| build(language)).as_ref()
}

fn markdown_inline_config() -> Option<&'static HighlightConfiguration> {
    static INLINE: Slot = OnceLock::new();
    INLINE
        .get_or_init(|| {
            make(
                tree_sitter_md::INLINE_LANGUAGE.into(),
                "markdown_inline",
                &[tree_sitter_md::HIGHLIGHT_QUERY_INLINE],
                tree_sitter_md::INJECTION_QUERY_INLINE,
            )
        })
        .as_ref()
}

fn make(
    language: tree_sitter::Language,
    name: &str,
    highlight_queries: &[&str],
    injections: &str,
) -> Option<HighlightConfiguration> {
    let query = highlight_queries.concat();
    let mut config = HighlightConfiguration::new(language.clone(), name, &query, injections, "")
        .or_else(|_| {
            // A combined query can fail on grammar-version skew; the last
            // (language-specific) query alone is better than nothing.
            let last = highlight_queries.last().copied().unwrap_or("");
            HighlightConfiguration::new(language, name, last, injections, "")
        })
        .ok()?;
    config.configure(HIGHLIGHT_NAMES);
    Some(config)
}

fn build(language: Language) -> Option<HighlightConfiguration> {
    use tree_sitter_javascript as js;
    let js_queries = [js::HIGHLIGHT_QUERY, js::JSX_HIGHLIGHT_QUERY];
    match language {
        Language::PlainText => None,
        Language::Rust => make(
            tree_sitter_rust::LANGUAGE.into(),
            "rust",
            &[tree_sitter_rust::HIGHLIGHTS_QUERY],
            tree_sitter_rust::INJECTIONS_QUERY,
        ),
        Language::JavaScript => make(
            js::LANGUAGE.into(),
            "javascript",
            &js_queries,
            js::INJECTIONS_QUERY,
        ),
        Language::TypeScript => make(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "typescript",
            &[
                js::HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            ],
            "",
        ),
        Language::Tsx => make(
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            "tsx",
            &[
                js::HIGHLIGHT_QUERY,
                js::JSX_HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            ],
            "",
        ),
        Language::Json => make(
            tree_sitter_json::LANGUAGE.into(),
            "json",
            &[tree_sitter_json::HIGHLIGHTS_QUERY],
            "",
        ),
        Language::Toml => make(
            tree_sitter_toml_ng::LANGUAGE.into(),
            "toml",
            &[tree_sitter_toml_ng::HIGHLIGHTS_QUERY],
            "",
        ),
        Language::Yaml => make(
            tree_sitter_yaml::LANGUAGE.into(),
            "yaml",
            &[tree_sitter_yaml::HIGHLIGHTS_QUERY],
            "",
        ),
        Language::Markdown => make(
            tree_sitter_md::LANGUAGE.into(),
            "markdown",
            &[tree_sitter_md::HIGHLIGHT_QUERY_BLOCK],
            tree_sitter_md::INJECTION_QUERY_BLOCK,
        ),
        Language::Python => make(
            tree_sitter_python::LANGUAGE.into(),
            "python",
            &[tree_sitter_python::HIGHLIGHTS_QUERY],
            "",
        ),
        Language::Go => make(
            tree_sitter_go::LANGUAGE.into(),
            "go",
            &[tree_sitter_go::HIGHLIGHTS_QUERY],
            "",
        ),
        Language::Html => make(
            tree_sitter_html::LANGUAGE.into(),
            "html",
            &[tree_sitter_html::HIGHLIGHTS_QUERY],
            tree_sitter_html::INJECTIONS_QUERY,
        ),
        Language::Css => make(
            tree_sitter_css::LANGUAGE.into(),
            "css",
            &[tree_sitter_css::HIGHLIGHTS_QUERY],
            "",
        ),
        Language::Bash => make(
            tree_sitter_bash::LANGUAGE.into(),
            "bash",
            &[tree_sitter_bash::HIGHLIGHT_QUERY],
            "",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds_of(language: Language, source: &str, needle: &str) -> Vec<&'static str> {
        let start = source.find(needle).expect("needle in source");
        let range = start..start + needle.len();
        highlight(language, source, None)
            .into_iter()
            .filter(|span| span.range.start < range.end && span.range.end > range.start)
            .map(|span| span.name())
            .collect()
    }

    #[test]
    fn every_language_has_a_working_grammar() {
        for language in Language::ALL {
            if language == Language::PlainText {
                assert!(!supports(language));
            } else {
                assert!(supports(language), "{language:?} failed to build");
            }
        }
    }

    #[test]
    fn rust_spans() {
        let source = "fn main() {\n    // hi\n    let s = \"x\";\n}\n";
        assert_eq!(kinds_of(Language::Rust, source, "fn"), vec!["keyword"]);
        assert_eq!(kinds_of(Language::Rust, source, "// hi"), vec!["comment"]);
        assert_eq!(kinds_of(Language::Rust, source, "\"x\""), vec!["string"]);
        assert!(kinds_of(Language::Rust, source, "main")[0].starts_with("function"));
    }

    #[test]
    fn python_and_json_spans() {
        let py = "def f(x):\n    return 'a'  # c\n";
        assert_eq!(kinds_of(Language::Python, py, "def"), vec!["keyword"]);
        assert_eq!(kinds_of(Language::Python, py, "'a'"), vec!["string"]);
        assert_eq!(kinds_of(Language::Python, py, "# c"), vec!["comment"]);
        let json = "{\"k\": 12, \"b\": true}";
        assert_eq!(kinds_of(Language::Json, json, "12"), vec!["number"]);
        assert!(
            kinds_of(Language::Json, json, "\"k\"")
                .iter()
                .any(|k| *k == "property" || *k == "string")
        );
    }

    #[test]
    fn typescript_and_tsx_spans() {
        let ts = "interface A { x: number }\nconst a: A = { x: 1 };\n";
        assert_eq!(kinds_of(Language::TypeScript, ts, "const"), vec!["keyword"]);
        assert!(!kinds_of(Language::TypeScript, ts, "number").is_empty());
        let tsx = "const el = <div className=\"a\">hi</div>;\n";
        assert_eq!(kinds_of(Language::Tsx, tsx, "const"), vec!["keyword"]);
    }

    #[test]
    fn markdown_injects_inline_and_code() {
        let md = "# Title\n\nSome `code` here.\n\n```rust\nfn x() {}\n```\n";
        assert!(!kinds_of(Language::Markdown, md, "Title").is_empty());
        assert_eq!(kinds_of(Language::Markdown, md, "fn"), vec!["keyword"]);
    }

    #[test]
    fn spans_are_sorted_and_disjoint() {
        let source = "<html><script>let a = 1;</script><style>a { color: red; }</style></html>";
        let spans = highlight(Language::Html, source, None);
        assert!(!spans.is_empty());
        for pair in spans.windows(2) {
            assert!(pair[0].range.end <= pair[1].range.start);
        }
        assert_eq!(kinds_of(Language::Html, source, "let"), vec!["keyword"]);
    }

    #[test]
    fn plain_text_has_no_spans() {
        assert!(highlight(Language::PlainText, "fn main() {}", None).is_empty());
    }
}
