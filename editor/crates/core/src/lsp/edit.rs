//! Workspace edits (rename, code actions, the server's
//! `workspace/applyEdit`) and text edits (formatting): every shape the
//! protocol allows, parsed leniently, and text edits applied to a buffer or
//! to plain text (files that are not open).

use std::ops::Range;

use serde_json::Value;

use super::position::{PositionEncoding, range_to_offsets};
use super::types::{TextEdit, parse_text_edit};
use crate::buffer::Buffer;

/// One step of a workspace edit, in the order the server gave them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp {
    /// Text edits in one document.
    Text { uri: String, edits: Vec<TextEdit> },
    Create {
        uri: String,
        overwrite: bool,
        ignore_if_exists: bool,
    },
    Rename {
        old_uri: String,
        new_uri: String,
        overwrite: bool,
        ignore_if_exists: bool,
    },
    Delete {
        uri: String,
        recursive: bool,
        ignore_if_not_exists: bool,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceEdit {
    pub ops: Vec<EditOp>,
}

impl WorkspaceEdit {
    pub fn is_empty(&self) -> bool {
        self.ops.iter().all(|op| match op {
            EditOp::Text { edits, .. } => edits.is_empty(),
            _ => false,
        })
    }

    /// Number of text edits across all documents.
    pub fn edit_count(&self) -> usize {
        self.ops
            .iter()
            .map(|op| match op {
                EditOp::Text { edits, .. } => edits.len(),
                _ => 1,
            })
            .sum()
    }

    /// Documents (or files) touched.
    pub fn file_count(&self) -> usize {
        let mut uris: Vec<&str> = self
            .ops
            .iter()
            .map(|op| match op {
                EditOp::Text { uri, .. }
                | EditOp::Create { uri, .. }
                | EditOp::Delete { uri, .. } => uri.as_str(),
                EditOp::Rename { new_uri, .. } => new_uri.as_str(),
            })
            .collect();
        uris.sort_unstable();
        uris.dedup();
        uris.len()
    }
}

fn flag(value: &Value, key: &str) -> bool {
    value["options"][key].as_bool().unwrap_or(false)
}

/// A `WorkspaceEdit`: `documentChanges` (text document edits and file
/// operations, in order) when present, otherwise the `changes` map.
pub fn parse_workspace_edit(value: &Value) -> WorkspaceEdit {
    let mut ops = Vec::new();
    if let Some(changes) = value.get("documentChanges").and_then(Value::as_array) {
        for change in changes {
            let op = match change["kind"].as_str() {
                Some("create") => change["uri"].as_str().map(|uri| EditOp::Create {
                    uri: uri.to_string(),
                    overwrite: flag(change, "overwrite"),
                    ignore_if_exists: flag(change, "ignoreIfExists"),
                }),
                Some("rename") => change["oldUri"]
                    .as_str()
                    .zip(change["newUri"].as_str())
                    .map(|(old, new)| EditOp::Rename {
                        old_uri: old.to_string(),
                        new_uri: new.to_string(),
                        overwrite: flag(change, "overwrite"),
                        ignore_if_exists: flag(change, "ignoreIfExists"),
                    }),
                Some("delete") => change["uri"].as_str().map(|uri| EditOp::Delete {
                    uri: uri.to_string(),
                    recursive: flag(change, "recursive"),
                    ignore_if_not_exists: flag(change, "ignoreIfNotExists"),
                }),
                // A TextDocumentEdit (annotated edits have the same shape).
                _ => change["textDocument"]["uri"]
                    .as_str()
                    .map(|uri| EditOp::Text {
                        uri: uri.to_string(),
                        edits: parse_text_edits(&change["edits"]),
                    }),
            };
            ops.extend(op);
        }
    } else if let Some(changes) = value.get("changes").and_then(Value::as_object) {
        for (uri, edits) in changes {
            ops.push(EditOp::Text {
                uri: uri.clone(),
                edits: parse_text_edits(edits),
            });
        }
    }
    WorkspaceEdit { ops }
}

/// `TextEdit[]` (a formatting result; `null` is no edits).
pub fn parse_text_edits(value: &Value) -> Vec<TextEdit> {
    value
        .as_array()
        .map(|edits| edits.iter().filter_map(parse_text_edit).collect())
        .unwrap_or_default()
}

/// Text edits as buffer ranges, sorted by position (inserts at the same
/// place keep the server's order). Overlapping edits are an error.
pub fn edits_to_offsets(
    buffer: &Buffer,
    edits: &[TextEdit],
    encoding: PositionEncoding,
) -> Result<Vec<(Range<usize>, String)>, String> {
    let mut out: Vec<(Range<usize>, String)> = edits
        .iter()
        .map(|e| {
            (
                range_to_offsets(buffer, e.range, encoding),
                e.new_text.replace("\r\n", "\n"),
            )
        })
        .collect();
    out.sort_by_key(|(r, _)| (r.start, r.end));
    if out.windows(2).any(|w| w[0].0.end > w[1].0.start) {
        return Err("the language server sent overlapping edits".into());
    }
    Ok(out)
}

/// Applies `edits` to `text` (a file that is not open in the editor).
pub fn apply_to_text(
    text: &str,
    edits: &[TextEdit],
    encoding: PositionEncoding,
) -> Result<String, String> {
    let buffer = Buffer::new(text);
    let ranges = edits_to_offsets(&buffer, edits, encoding)?;
    let mut out = buffer.text();
    for (range, new_text) in ranges.into_iter().rev() {
        out.replace_range(range, &new_text);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::position::{LspRange, Position};
    use serde_json::json;

    fn range(l0: u32, c0: u32, l1: u32, c1: u32) -> Value {
        json!({"start": {"line": l0, "character": c0}, "end": {"line": l1, "character": c1}})
    }

    fn edit(l0: u32, c0: u32, l1: u32, c1: u32, text: &str) -> TextEdit {
        TextEdit {
            range: LspRange {
                start: Position::new(l0, c0),
                end: Position::new(l1, c1),
            },
            new_text: text.to_string(),
        }
    }

    #[test]
    fn parses_changes_map() {
        let edit = parse_workspace_edit(&json!({"changes": {
            "file:///a.rs": [{"range": range(0, 3, 0, 6), "newText": "bar"}],
            "file:///b.rs": [{"range": range(1, 0, 1, 3), "newText": "bar"},
                             {"range": range(2, 0, 2, 3), "newText": "bar"}],
        }}));
        assert_eq!(edit.ops.len(), 2);
        assert_eq!(edit.edit_count(), 3);
        assert_eq!(edit.file_count(), 2);
        assert!(!edit.is_empty());
    }

    #[test]
    fn document_changes_win_and_keep_file_operations_in_order() {
        let edit = parse_workspace_edit(&json!({
            "changes": {"file:///ignored.rs": []},
            "documentChanges": [
                {"kind": "create", "uri": "file:///new.rs", "options": {"ignoreIfExists": true}},
                {"textDocument": {"uri": "file:///new.rs", "version": null},
                 "edits": [{"range": range(0, 0, 0, 0), "newText": "mod x;", "annotationId": "a"}]},
                {"kind": "rename", "oldUri": "file:///a.rs", "newUri": "file:///b.rs"},
                {"kind": "delete", "uri": "file:///c.rs", "options": {"recursive": true}},
                {"kind": "bogus"}
            ]
        }));
        assert_eq!(edit.ops.len(), 4);
        assert_eq!(
            edit.ops[0],
            EditOp::Create {
                uri: "file:///new.rs".into(),
                overwrite: false,
                ignore_if_exists: true
            }
        );
        assert!(
            matches!(&edit.ops[1], EditOp::Text { edits, .. } if edits[0].new_text == "mod x;")
        );
        assert!(
            matches!(&edit.ops[2], EditOp::Rename { new_uri, .. } if new_uri == "file:///b.rs")
        );
        assert!(matches!(
            edit.ops[3],
            EditOp::Delete {
                recursive: true,
                ..
            }
        ));
        assert!(parse_workspace_edit(&Value::Null).is_empty());
        assert!(parse_text_edits(&Value::Null).is_empty());
    }

    #[test]
    fn applies_edits_in_position_order() {
        let text = "fn foo() {}\nfn main() { foo(); }\n";
        let edits = vec![
            edit(1, 12, 1, 15, "bar"),
            edit(0, 3, 0, 6, "bar"),
            // Two inserts at one place keep their order.
            edit(0, 0, 0, 0, "// a\n"),
            edit(0, 0, 0, 0, "// b\n"),
        ];
        assert_eq!(
            apply_to_text(text, &edits, PositionEncoding::Utf16).as_deref(),
            Ok("// a\n// b\nfn bar() {}\nfn main() { bar(); }\n")
        );
        let overlapping = vec![edit(0, 0, 0, 5, "x"), edit(0, 3, 0, 6, "y")];
        assert!(apply_to_text(text, &overlapping, PositionEncoding::Utf16).is_err());
    }

    #[test]
    fn utf16_columns_and_crlf_text() {
        // 😀 is two UTF-16 units, four bytes.
        let text = "let 😀x = 1;\n";
        let edits = vec![edit(0, 6, 0, 7, "y\r\nz")];
        assert_eq!(
            apply_to_text(text, &edits, PositionEncoding::Utf16).as_deref(),
            Ok("let 😀y\nz = 1;\n")
        );
        let edits = vec![edit(0, 8, 0, 9, "q")];
        assert_eq!(
            apply_to_text(text, &edits, PositionEncoding::Utf8).as_deref(),
            Ok("let 😀q = 1;\n")
        );
    }
}
