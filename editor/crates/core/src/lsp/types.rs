//! The few protocol shapes the editor uses, parsed leniently from JSON:
//! diagnostics, completion items, locations and hover text.

use serde::Deserialize;
use serde_json::Value;

use super::position::LspRange;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

impl Severity {
    fn from_number(n: Option<u64>) -> Self {
        match n {
            Some(2) => Severity::Warning,
            Some(3) => Severity::Information,
            Some(4) => Severity::Hint,
            // Servers may omit it; the spec says to treat that as an error.
            _ => Severity::Error,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Information => "info",
            Severity::Hint => "hint",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: LspRange,
    pub severity: Severity,
    pub message: String,
    pub source: Option<String>,
    pub code: Option<String>,
}

impl Diagnostic {
    /// "message (source code)" for popups and lists.
    pub fn full_message(&self) -> String {
        let origin = match (&self.source, &self.code) {
            (Some(s), Some(c)) => format!(" ({s} {c})"),
            (Some(s), None) => format!(" ({s})"),
            (None, Some(c)) => format!(" ({c})"),
            (None, None) => String::new(),
        };
        format!("{}{origin}", self.message.trim_end())
    }
}

/// `textDocument/publishDiagnostics` params: (uri, diagnostics sorted by
/// position).
pub fn parse_publish_diagnostics(params: &Value) -> Option<(String, Vec<Diagnostic>)> {
    let uri = params["uri"].as_str()?.to_string();
    let mut list: Vec<Diagnostic> = params["diagnostics"]
        .as_array()
        .map(|items| items.iter().filter_map(parse_diagnostic).collect())
        .unwrap_or_default();
    list.sort_by_key(|d| (d.range.start, d.severity));
    Some((uri, list))
}

fn parse_diagnostic(value: &Value) -> Option<Diagnostic> {
    let range: LspRange = serde_json::from_value(value["range"].clone()).ok()?;
    let code = match &value["code"] {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    };
    Some(Diagnostic {
        range,
        severity: Severity::from_number(value["severity"].as_u64()),
        message: value["message"].as_str().unwrap_or_default().to_string(),
        source: value["source"].as_str().map(str::to_string),
        code,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub range: LspRange,
    pub new_text: String,
}

fn parse_text_edit(value: &Value) -> Option<TextEdit> {
    // `InsertReplaceEdit` has `insert` / `replace` instead of `range`.
    let range = value
        .get("range")
        .or_else(|| value.get("replace"))
        .and_then(|r| serde_json::from_value(r.clone()).ok())?;
    Some(TextEdit {
        range,
        new_text: value["newText"].as_str()?.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: Option<u64>,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub filter_text: String,
    pub sort_text: String,
    /// Text to insert (or the edit's new text).
    pub insert_text: String,
    pub is_snippet: bool,
    /// Range the server wants replaced, if it gave one.
    pub edit_range: Option<LspRange>,
    /// Edits elsewhere, such as an import.
    pub additional_edits: Vec<TextEdit>,
    pub preselect: bool,
    /// The item as the server sent it (sent back for `completionItem/resolve`).
    pub raw: Value,
}

impl CompletionItem {
    /// Fills in what `completionItem/resolve` added: docs, detail and
    /// edits such as auto-imports (typescript-language-server sends those
    /// only here).
    pub fn apply_resolved(&mut self, resolved: &Value) {
        let parsed = parse_completion(&Value::Array(vec![resolved.clone()])).0;
        let Some(new) = parsed.into_iter().next() else {
            return;
        };
        if new.documentation.is_some() {
            self.documentation = new.documentation;
        }
        if new.detail.is_some() {
            self.detail = new.detail;
        }
        if !new.additional_edits.is_empty() {
            self.additional_edits = new.additional_edits;
        }
        if resolved.get("textEdit").is_some() || resolved.get("insertText").is_some() {
            self.insert_text = new.insert_text;
            self.is_snippet = new.is_snippet;
            if new.edit_range.is_some() {
                self.edit_range = new.edit_range;
            }
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RawItem {
    label: String,
    kind: Option<u64>,
    detail: Option<String>,
    documentation: Option<Value>,
    filter_text: Option<String>,
    sort_text: Option<String>,
    insert_text: Option<String>,
    insert_text_format: Option<u64>,
    text_edit: Option<Value>,
    additional_text_edits: Option<Vec<Value>>,
    preselect: Option<bool>,
    label_details: Option<Value>,
}

/// A completion response (`CompletionItem[]` or `CompletionList`):
/// the items and whether the list is incomplete.
pub fn parse_completion(value: &Value) -> (Vec<CompletionItem>, bool) {
    let (items, incomplete) = match value {
        Value::Array(items) => (items.as_slice(), false),
        Value::Object(list) => (
            list.get("items")
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice),
            list.get("isIncomplete")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        _ => (&[][..], false),
    };
    let items = items
        .iter()
        .filter_map(|item| {
            serde_json::from_value::<RawItem>(item.clone())
                .ok()
                .map(|raw| (raw, item))
        })
        .filter(|(raw, _)| !raw.label.is_empty())
        .map(|(raw, original)| {
            let edit = raw.text_edit.as_ref().and_then(parse_text_edit);
            let insert_text = edit
                .as_ref()
                .map(|e| e.new_text.clone())
                .or(raw.insert_text)
                .unwrap_or_else(|| raw.label.clone());
            // rust-analyzer puts the signature in labelDetails.
            let detail = raw.detail.or_else(|| {
                raw.label_details
                    .as_ref()
                    .and_then(|d| d["description"].as_str().or(d["detail"].as_str()))
                    .map(str::to_string)
            });
            CompletionItem {
                filter_text: raw.filter_text.unwrap_or_else(|| raw.label.clone()),
                sort_text: raw.sort_text.unwrap_or_else(|| raw.label.clone()),
                label: raw.label,
                kind: raw.kind,
                detail,
                documentation: raw.documentation.as_ref().and_then(markup_text),
                insert_text,
                is_snippet: raw.insert_text_format == Some(2),
                edit_range: edit.map(|e| e.range),
                additional_edits: raw
                    .additional_text_edits
                    .unwrap_or_default()
                    .iter()
                    .filter_map(parse_text_edit)
                    .collect(),
                preselect: raw.preselect.unwrap_or(false),
                raw: original.clone(),
            }
        })
        .collect();
    (items, incomplete)
}

/// Short tag for a `CompletionItemKind`.
pub fn kind_label(kind: Option<u64>) -> &'static str {
    match kind {
        Some(2) => "method",
        Some(3) => "fn",
        Some(4) => "new",
        Some(5) => "field",
        Some(6) => "var",
        Some(7) => "class",
        Some(8) => "interface",
        Some(9) => "module",
        Some(10) => "property",
        Some(11) => "unit",
        Some(12) => "value",
        Some(13) => "enum",
        Some(14) => "keyword",
        Some(15) => "snippet",
        Some(16) => "color",
        Some(17) => "file",
        Some(18) => "ref",
        Some(19) => "folder",
        Some(20) => "variant",
        Some(21) => "const",
        Some(22) => "struct",
        Some(23) => "event",
        Some(24) => "operator",
        Some(25) => "type",
        _ => "text",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub uri: String,
    pub range: LspRange,
}

/// A definition response: `Location`, `Location[]` or `LocationLink[]`.
pub fn parse_locations(value: &Value) -> Vec<Location> {
    let one = |v: &Value| -> Option<Location> {
        // LocationLink: targetUri + targetSelectionRange.
        let uri = v["uri"].as_str().or(v["targetUri"].as_str())?;
        let range = v
            .get("range")
            .or_else(|| v.get("targetSelectionRange"))
            .or_else(|| v.get("targetRange"))?;
        Some(Location {
            uri: uri.to_string(),
            range: serde_json::from_value(range.clone()).ok()?,
        })
    };
    match value {
        Value::Array(items) => items.iter().filter_map(one).collect(),
        Value::Object(_) => one(value).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// Text of `MarkupContent`, `MarkedString` or an array of them, with code
/// fences removed (the popup shows plain text).
pub fn markup_text(value: &Value) -> Option<String> {
    let text = match value {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(markup_text)
            .collect::<Vec<_>>()
            .join("\n\n"),
        Value::Object(o) => o
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        _ => return None,
    };
    let cleaned: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim_start().starts_with("```"))
        .collect();
    let cleaned = cleaned.join("\n");
    let cleaned = cleaned.trim();
    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

/// A hover response's text.
pub fn parse_hover(value: &Value) -> Option<String> {
    markup_text(value.get("contents")?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::position::Position;
    use serde_json::json;

    fn range(l0: u32, c0: u32, l1: u32, c1: u32) -> Value {
        json!({"start": {"line": l0, "character": c0}, "end": {"line": l1, "character": c1}})
    }

    #[test]
    fn parses_diagnostics_sorted() {
        let params = json!({
            "uri": "file:///a.rs",
            "diagnostics": [
                {"range": range(3, 0, 3, 4), "severity": 2, "message": "unused", "source": "rustc", "code": "W1"},
                {"range": range(1, 2, 1, 5), "message": "mismatched types", "code": 308},
                {"bogus": true}
            ]
        });
        let (uri, list) = parse_publish_diagnostics(&params).expect("test value");
        assert_eq!(uri, "file:///a.rs");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].severity, Severity::Error);
        assert_eq!(list[0].code.as_deref(), Some("308"));
        assert_eq!(list[1].full_message(), "unused (rustc W1)");
        assert_eq!(list[0].range.start, Position::new(1, 2));
    }

    #[test]
    fn parses_completion_lists() {
        let value = json!({
            "isIncomplete": true,
            "items": [
                {"label": "push", "kind": 2, "detail": "fn(&mut self, T)",
                 "textEdit": {"range": range(0, 4, 0, 6), "newText": "push(${1:value})"},
                 "insertTextFormat": 2,
                 "additionalTextEdits": [{"range": range(0, 0, 0, 0), "newText": "use x;\n"}]},
                {"label": "len", "insertText": "len()", "documentation": {"kind": "markdown", "value": "```rust\nfn len()\n```\nLength."}},
                {"label": "rep", "textEdit": {"insert": range(0, 1, 0, 2), "replace": range(0, 1, 0, 3), "newText": "repeat"},
                 "labelDetails": {"description": "fn(n)"}}
            ]
        });
        let (items, incomplete) = parse_completion(&value);
        assert!(incomplete);
        assert_eq!(items.len(), 3);
        assert!(items[0].is_snippet);
        assert_eq!(items[0].insert_text, "push(${1:value})");
        assert_eq!(items[0].additional_edits.len(), 1);
        assert_eq!(kind_label(items[0].kind), "method");
        assert_eq!(items[1].insert_text, "len()");
        assert_eq!(items[1].documentation.as_deref(), Some("fn len()\nLength."));
        assert_eq!(
            items[2].edit_range.expect("test value").end,
            Position::new(0, 3)
        );
        assert_eq!(items[2].detail.as_deref(), Some("fn(n)"));
        let (plain, incomplete) = parse_completion(&json!([{"label": "a"}]));
        assert!(!incomplete);
        assert_eq!(plain[0].insert_text, "a");
    }

    #[test]
    fn resolve_adds_imports_and_docs() {
        let (mut items, _) =
            parse_completion(&json!([{"label": "useState", "kind": 3, "data": {"id": 7}}]));
        let item = &mut items[0];
        assert_eq!(item.raw["data"]["id"], 7);
        item.apply_resolved(&json!({
            "label": "useState", "detail": "function useState<S>()",
            "documentation": "Returns a stateful value.",
            "additionalTextEdits": [{"range": range(0, 0, 0, 0), "newText": "import { useState } from \"react\";\n"}]
        }));
        assert_eq!(item.detail.as_deref(), Some("function useState<S>()"));
        assert_eq!(
            item.documentation.as_deref(),
            Some("Returns a stateful value.")
        );
        assert_eq!(item.additional_edits.len(), 1);
        assert_eq!(item.insert_text, "useState");
    }

    #[test]
    fn parses_every_location_shape() {
        let single = json!({"uri": "file:///a", "range": range(1, 0, 1, 3)});
        assert_eq!(parse_locations(&single).len(), 1);
        let links = json!([{"targetUri": "file:///b", "targetRange": range(0, 0, 9, 0),
                            "targetSelectionRange": range(2, 4, 2, 8)}]);
        let parsed = parse_locations(&links);
        assert_eq!(parsed[0].uri, "file:///b");
        assert_eq!(parsed[0].range.start, Position::new(2, 4));
        assert!(parse_locations(&Value::Null).is_empty());
    }

    #[test]
    fn hover_text_drops_fences() {
        let hover = json!({"contents": {"kind": "markdown", "value": "```rust\nfn main()\n```\n\nEntry point."}});
        assert_eq!(
            parse_hover(&hover).as_deref(),
            Some("fn main()\n\nEntry point.")
        );
        let marked = json!({"contents": [{"language": "ts", "value": "let x: number"}, "doc"]});
        assert_eq!(
            parse_hover(&marked).as_deref(),
            Some("let x: number\n\ndoc")
        );
        assert_eq!(parse_hover(&json!({"contents": ""})), None);
    }
}
