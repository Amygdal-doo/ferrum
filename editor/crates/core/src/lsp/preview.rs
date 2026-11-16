//! Previews of workspace edits (rename): per file, the lines that change,
//! before and after, so the user can check a rename before it is applied.

use std::path::PathBuf;

use similar::{ChangeTag, TextDiff};

use super::edit::{EditOp, WorkspaceEdit, apply_to_text};
use super::position::PositionEncoding;
use super::uri_to_path;

/// Most changed lines listed per file; the rest are counted.
const LINES_PER_FILE: usize = 50;

/// One changed line pair (a removed line, an added line, or both).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineChange {
    /// 0-based line in the file before the edit.
    pub line: usize,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// What an edit does to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChangeKind {
    /// Text edits; `lines` is capped at [`LINES_PER_FILE`].
    Edit {
        edits: usize,
        lines: Vec<LineChange>,
        more: usize,
    },
    Create,
    Rename {
        to: PathBuf,
    },
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreview {
    pub path: PathBuf,
    pub kind: FileChangeKind,
}

/// Previews `edit`. `text_of` gives a file's current text (the open
/// buffer, else the disk). Fails like applying would (overlapping edits,
/// unreadable files), so a preview that succeeds can be applied.
pub fn preview_workspace_edit(
    edit: &WorkspaceEdit,
    encoding: PositionEncoding,
    mut text_of: impl FnMut(&std::path::Path) -> Option<String>,
) -> Result<Vec<FilePreview>, String> {
    let path_of = |uri: &str| uri_to_path(uri).ok_or_else(|| format!("not a file: {uri}"));
    let mut out = Vec::new();
    for op in &edit.ops {
        let preview = match op {
            EditOp::Text { uri, edits } => {
                if edits.is_empty() {
                    continue;
                }
                let path = path_of(uri)?;
                let before = text_of(&path)
                    .ok_or_else(|| format!("{}: can't read the file", path.display()))?;
                let after = apply_to_text(&before, edits, encoding)?;
                let mut lines = changed_lines(&before, &after);
                let more = lines.len().saturating_sub(LINES_PER_FILE);
                lines.truncate(LINES_PER_FILE);
                FilePreview {
                    path,
                    kind: FileChangeKind::Edit {
                        edits: edits.len(),
                        lines,
                        more,
                    },
                }
            }
            EditOp::Create { uri, .. } => FilePreview {
                path: path_of(uri)?,
                kind: FileChangeKind::Create,
            },
            EditOp::Rename {
                old_uri, new_uri, ..
            } => FilePreview {
                path: path_of(old_uri)?,
                kind: FileChangeKind::Rename {
                    to: path_of(new_uri)?,
                },
            },
            EditOp::Delete { uri, .. } => FilePreview {
                path: path_of(uri)?,
                kind: FileChangeKind::Delete,
            },
        };
        out.push(preview);
    }
    Ok(out)
}

/// Changed lines of `before` → `after`, removed and added lines of one
/// hunk paired up in order.
pub fn changed_lines(before: &str, after: &str) -> Vec<LineChange> {
    let diff = TextDiff::from_lines(before, after);
    let mut out = Vec::new();
    for ops in diff.grouped_ops(0) {
        for op in ops {
            let mut removed = Vec::new();
            let mut added = Vec::new();
            for change in diff.iter_changes(&op) {
                let text = change.value().trim_end_matches(['\n', '\r']).to_string();
                match change.tag() {
                    ChangeTag::Delete => removed.push((change.old_index().unwrap_or(0), text)),
                    ChangeTag::Insert => added.push(text),
                    ChangeTag::Equal => {}
                }
            }
            let start = op.old_range().start;
            for i in 0..removed.len().max(added.len()) {
                out.push(LineChange {
                    line: removed.get(i).map_or(start + i, |(line, _)| *line),
                    before: removed.get(i).map(|(_, text)| text.clone()),
                    after: added.get(i).cloned(),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::edit::parse_workspace_edit;
    use serde_json::json;

    fn range(l0: u32, c0: u32, l1: u32, c1: u32) -> serde_json::Value {
        json!({"start": {"line": l0, "character": c0}, "end": {"line": l1, "character": c1}})
    }

    #[test]
    fn pairs_changed_lines() {
        let lines = changed_lines("fn foo() {}\nfoo();\nkeep\n", "fn bar() {}\nbar();\nkeep\n");
        assert_eq!(
            lines,
            vec![
                LineChange {
                    line: 0,
                    before: Some("fn foo() {}".into()),
                    after: Some("fn bar() {}".into()),
                },
                LineChange {
                    line: 1,
                    before: Some("foo();".into()),
                    after: Some("bar();".into()),
                },
            ]
        );
        let inserted = changed_lines("a\nb\n", "a\nx\nb\n");
        assert_eq!(inserted.len(), 1);
        assert_eq!((inserted[0].line, inserted[0].before.as_deref()), (1, None));
        assert_eq!(inserted[0].after.as_deref(), Some("x"));
    }

    #[test]
    fn previews_rename_across_files() {
        let (a, b) = if cfg!(windows) {
            (r"C:\p\a.rs", r"C:\p\b.rs")
        } else {
            ("/p/a.rs", "/p/b.rs")
        };
        let (ua, ub) = (
            crate::lsp::path_to_uri(a.as_ref()),
            crate::lsp::path_to_uri(b.as_ref()),
        );
        let edit = parse_workspace_edit(&json!({"documentChanges": [
            {"textDocument": {"uri": ua, "version": 1},
             "edits": [{"range": range(0, 3, 0, 6), "newText": "bar"}]},
            {"textDocument": {"uri": ub, "version": null},
             "edits": [{"range": range(1, 0, 1, 3), "newText": "bar"},
                       {"range": range(2, 4, 2, 7), "newText": "bar"}]},
            {"kind": "rename", "oldUri": ub, "newUri": ua},
        ]}));
        let preview = preview_workspace_edit(&edit, PositionEncoding::Utf16, |path| {
            Some(if path.ends_with("a.rs") {
                "fn foo() {}\n".to_string()
            } else {
                "use x;\nfoo();\nlet foo = 1;\n".to_string()
            })
        })
        .expect("preview");
        assert_eq!(preview.len(), 3);
        let FileChangeKind::Edit { edits, lines, more } = &preview[1].kind else {
            panic!("expected an edit: {preview:?}");
        };
        assert_eq!((*edits, lines.len(), *more), (2, 2, 0));
        assert_eq!(lines[1].after.as_deref(), Some("let bar = 1;"));
        assert!(matches!(preview[2].kind, FileChangeKind::Rename { .. }));
    }

    #[test]
    fn unreadable_or_overlapping_edits_fail() {
        let uri =
            crate::lsp::path_to_uri(if cfg!(windows) { r"C:\x.rs" } else { "/x.rs" }.as_ref());
        let edit = parse_workspace_edit(&json!({"changes": {uri.clone(): [
            {"range": range(0, 0, 0, 2), "newText": "a"},
        ]}}));
        assert!(preview_workspace_edit(&edit, PositionEncoding::Utf8, |_| None).is_err());
        let overlapping = parse_workspace_edit(&json!({"changes": {uri: [
            {"range": range(0, 0, 0, 3), "newText": "a"},
            {"range": range(0, 1, 0, 2), "newText": "b"},
        ]}}));
        assert!(
            preview_workspace_edit(&overlapping, PositionEncoding::Utf8, |_| Some(
                "abcdef".into()
            ))
            .is_err()
        );
    }
}
