//! `{{variable}}` scanning and substitution.
//!
//! Substitution is a single pass: a value that itself contains `{{x}}` is not
//! expanded again (avoids cycles). Unknown variables are left verbatim.

use crate::environment::Variable;
use std::collections::HashMap;
use std::ops::Range;

/// A `{{name}}` occurrence; `range` covers the braces (byte offsets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    pub name: String,
    pub range: Range<usize>,
}

/// Text split for highlighting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Text(String),
    Var { name: String, resolved: bool },
}

/// Every `{{name}}` in `s`, in order. Empty or brace-containing names are skipped.
#[must_use]
pub fn placeholders(s: &str) -> Vec<Placeholder> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(open) = s[from..].find("{{").map(|i| i + from) {
        let Some(close) = s[open + 2..].find("}}").map(|i| i + open + 2) else {
            break;
        };
        let inner = &s[open + 2..close];
        let name = inner.trim();
        if name.is_empty() || name.contains('{') || name.contains('}') {
            from = open + 1;
            continue;
        }
        out.push(Placeholder {
            name: name.to_owned(),
            range: open..close + 2,
        });
        from = close + 2;
    }
    out
}

/// Resolved variables, built from layers (later layers win).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VariableSet {
    values: HashMap<String, String>,
}

impl VariableSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Typical order: globals, collection variables, active environment.
    /// Disabled and empty-key variables are skipped.
    #[must_use]
    pub fn from_layers(layers: &[&[Variable]]) -> Self {
        let mut set = Self::new();
        for layer in layers {
            for v in layer
                .iter()
                .filter(|v| v.enabled && !v.key.trim().is_empty())
            {
                set.insert(v.key.trim(), v.value.clone());
            }
        }
        set
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.values.insert(key.into(), value.into());
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Replaces every known `{{name}}`; unknown ones stay as written.
    #[must_use]
    pub fn resolve(&self, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut last = 0;
        for p in placeholders(s) {
            if let Some(v) = self.get(&p.name) {
                out.push_str(&s[last..p.range.start]);
                out.push_str(v);
                last = p.range.end;
            }
        }
        out.push_str(&s[last..]);
        out
    }

    /// Names referenced in `s` that have no value, deduplicated, in order.
    #[must_use]
    pub fn unresolved(&self, s: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for p in placeholders(s) {
            if self.get(&p.name).is_none() && !out.contains(&p.name) {
                out.push(p.name);
            }
        }
        out
    }

    /// Splits `s` into plain text and variable segments for highlighting.
    #[must_use]
    pub fn segments(&self, s: &str) -> Vec<Segment> {
        let mut out = Vec::new();
        let mut last = 0;
        for p in placeholders(s) {
            if p.range.start > last {
                out.push(Segment::Text(s[last..p.range.start].to_owned()));
            }
            let resolved = self.get(&p.name).is_some();
            out.push(Segment::Var {
                name: p.name,
                resolved,
            });
            last = p.range.end;
        }
        if last < s.len() {
            out.push(Segment::Text(s[last..].to_owned()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_placeholders() {
        let p = placeholders("{{base}}/users/{{ id }}?x={{}}&y={{a{b}}");
        let names: Vec<_> = p.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["base", "id"]);
        assert_eq!(p[0].range, 0..8);
    }

    #[test]
    fn layers_resolve_and_report_unresolved() {
        let globals = [
            Variable::new("host", "global.example"),
            Variable::new("v", "1"),
        ];
        let mut off = Variable::new("v", "disabled");
        off.enabled = false;
        let env = [Variable::new("host", "env.example"), off];
        let set = VariableSet::from_layers(&[&globals, &env]);
        assert_eq!(
            set.resolve("https://{{host}}/v{{v}}/{{missing}}"),
            "https://env.example/v1/{{missing}}"
        );
        assert_eq!(
            set.unresolved("{{missing}} {{host}} {{missing}}"),
            ["missing"]
        );
    }

    #[test]
    fn single_pass_no_recursion() {
        let mut set = VariableSet::new();
        set.insert("a", "{{a}}");
        assert_eq!(set.resolve("{{a}}"), "{{a}}");
    }

    #[test]
    fn segments_for_highlighting() {
        let mut set = VariableSet::new();
        set.insert("h", "x");
        let seg = set.segments("a{{h}}b{{n}}");
        assert_eq!(seg.len(), 4);
        assert_eq!(
            seg[3],
            Segment::Var {
                name: "n".into(),
                resolved: false
            }
        );
    }
}
