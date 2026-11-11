//! User snippets (`~/.ferrum/editor/snippets.toml`) and body expansion.
//!
//! ```toml
//! [rust.main]                # [<language id or "*">.<snippet name>]
//! prefix = "main"
//! body = "fn main() {\n    $0\n}"
//! description = "main function"
//! ```
//! Bodies support `$1`, `${1}`, `${1:placeholder}`, `$0` (final caret)
//! and `\$` for a literal dollar.

use std::collections::BTreeMap;
use std::ops::Range;

use serde::Deserialize;

use crate::language::Language;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    pub name: String,
    pub prefix: String,
    pub body: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawSnippet {
    prefix: String,
    body: String,
    #[serde(default)]
    description: Option<String>,
}

/// All snippets, keyed by language id (`"*"` = every language).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnippetSet {
    by_language: BTreeMap<String, Vec<Snippet>>,
}

impl SnippetSet {
    pub fn parse(source: &str) -> Result<Self, String> {
        let raw: BTreeMap<String, BTreeMap<String, RawSnippet>> =
            toml::from_str(source).map_err(|e| e.message().to_string())?;
        let mut by_language = BTreeMap::new();
        for (language, snippets) in raw {
            let key = if language == "*" {
                language
            } else {
                Language::from_id(&language)
                    .map(|l| l.id().to_string())
                    .ok_or_else(|| format!("unknown language `{language}` in snippets"))?
            };
            let list: &mut Vec<Snippet> = by_language.entry(key).or_default();
            for (name, raw) in snippets {
                list.push(Snippet {
                    name,
                    prefix: raw.prefix,
                    body: raw.body,
                    description: raw.description,
                });
            }
        }
        Ok(Self { by_language })
    }

    /// Snippets for `language`, language-specific ones first.
    pub fn for_language(&self, language: Language) -> Vec<Snippet> {
        let mut out = self
            .by_language
            .get(language.id())
            .cloned()
            .unwrap_or_default();
        out.extend(self.by_language.get("*").cloned().unwrap_or_default());
        out
    }

    pub fn len(&self) -> usize {
        self.by_language.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabStop {
    pub index: u32,
    pub range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expansion {
    pub text: String,
    /// In visiting order: `$1`, `$2`, … then `$0` (implicit at the end).
    pub stops: Vec<TabStop>,
}

/// Expands a body; lines after the first get `indent` prepended.
pub fn expand(body: &str, indent: &str) -> Expansion {
    let mut text = String::new();
    let mut stops: Vec<TabStop> = Vec::new();
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && matches!(chars.get(i + 1), Some('$' | '}' | '\\')) {
            text.push(chars[i + 1]);
            i += 2;
        } else if c == '\n' {
            text.push('\n');
            text.push_str(indent);
            i += 1;
        } else if c == '$' {
            if let Some((index, placeholder, next)) = parse_stop(&chars, i) {
                let start = text.len();
                for p in placeholder.chars() {
                    text.push(p);
                    if p == '\n' {
                        text.push_str(indent);
                    }
                }
                stops.push(TabStop {
                    index,
                    range: start..text.len(),
                });
                i = next;
            } else {
                text.push('$');
                i += 1;
            }
        } else {
            text.push(c);
            i += 1;
        }
    }
    if !stops.iter().any(|s| s.index == 0) {
        stops.push(TabStop {
            index: 0,
            range: text.len()..text.len(),
        });
    }
    stops.sort_by_key(|s| (s.index == 0, s.index, s.range.start));
    Expansion { text, stops }
}

/// Parses `$N`, `${N}` or `${N:placeholder}` at `chars[at]` (`$`).
fn parse_stop(chars: &[char], at: usize) -> Option<(u32, String, usize)> {
    let mut i = at + 1;
    let braced = chars.get(i) == Some(&'{');
    if braced {
        i += 1;
    }
    let digits_start = i;
    while chars.get(i).is_some_and(char::is_ascii_digit) {
        i += 1;
    }
    if i == digits_start {
        return None;
    }
    let index: u32 = chars[digits_start..i]
        .iter()
        .collect::<String>()
        .parse()
        .ok()?;
    if !braced {
        return Some((index, String::new(), i));
    }
    let mut placeholder = String::new();
    if chars.get(i) == Some(&':') {
        i += 1;
        let mut depth = 0;
        while let Some(&c) = chars.get(i) {
            match c {
                '\\' if matches!(chars.get(i + 1), Some('}' | '$' | '\\')) => {
                    placeholder.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                '{' => depth += 1,
                '}' if depth == 0 => break,
                '}' => depth -= 1,
                _ => {}
            }
            placeholder.push(c);
            i += 1;
        }
    }
    (chars.get(i) == Some(&'}')).then_some((index, placeholder, i + 1))
}

/// Default `snippets.toml` written on first launch.
pub const DEFAULT_SNIPPETS_TOML: &str = r##"# Ferrum Editor snippets. Type a prefix and press Tab to expand; Tab /
# Shift+Tab move between $1, $2, … and $0 (the final caret). ${1:text}
# pre-fills a placeholder. Changes apply as soon as you save.
#
# Sections are [<language>.<name>]; use ["*".<name>] for every language.
# Languages: rust javascript typescript tsx json toml yaml markdown python
# go html css shell text

[rust.fn]
prefix = "fn"
body = "fn ${1:name}($2) {\n    $0\n}"
description = "Function"

[rust.test]
prefix = "test"
body = "#[test]\nfn ${1:name}() {\n    $0\n}"
description = "Unit test"

[python.def]
prefix = "def"
body = "def ${1:name}($2):\n    ${0:pass}"

[javascript.log]
prefix = "log"
body = "console.log($0);"

[typescript.log]
prefix = "log"
body = "console.log($0);"

[markdown.link]
prefix = "link"
body = "[${1:text}](${2:url})$0"

["*".todo]
prefix = "todo"
body = "TODO: $0"
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_snippet_file() {
        let set = SnippetSet::parse(
            r#"
            [rust.main]
            prefix = "main"
            body = "fn main() {\n    $0\n}"
            description = "main fn"

            ["*".todo]
            prefix = "todo"
            body = "TODO: $1"

            [py.def]
            prefix = "def"
            body = "def ${1:name}():\n    $0"
            "#,
        )
        .expect("valid");
        assert_eq!(set.len(), 3);
        let rust = set.for_language(Language::Rust);
        assert_eq!(
            rust.iter().map(|s| s.prefix.as_str()).collect::<Vec<_>>(),
            vec!["main", "todo"]
        );
        assert_eq!(set.for_language(Language::Python)[0].prefix, "def");
        assert!(SnippetSet::parse("[klingon.x]\nprefix='a'\nbody='b'").is_err());
    }

    #[test]
    fn expands_stops_in_order_with_final_caret() {
        let e = expand("fn ${1:name}($2) {\n    $0\n}", "  ");
        assert_eq!(e.text, "fn name() {\n      \n  }");
        assert_eq!(
            e.stops[0],
            TabStop {
                index: 1,
                range: 3..7
            }
        );
        assert_eq!(
            e.stops[1],
            TabStop {
                index: 2,
                range: 8..8
            }
        );
        assert_eq!(
            e.stops[2],
            TabStop {
                index: 0,
                range: 18..18
            }
        );
    }

    #[test]
    fn implicit_final_stop_and_escapes() {
        let e = expand(r"cost: \$5 $1", "");
        assert_eq!(e.text, "cost: $5 ");
        assert_eq!(e.stops.len(), 2);
        assert_eq!(
            e.stops[1],
            TabStop {
                index: 0,
                range: 9..9
            }
        );
    }

    #[test]
    fn mirrored_stops_and_literal_dollar() {
        let e = expand("${1:a} = $1; $x", "");
        assert_eq!(e.text, "a = ; $x");
        let ones: Vec<_> = e.stops.iter().filter(|s| s.index == 1).collect();
        assert_eq!(ones.len(), 2);
    }

    #[test]
    fn nested_braces_in_placeholder() {
        let e = expand("${1:{x}}", "");
        assert_eq!(e.text, "{x}");
    }
}
