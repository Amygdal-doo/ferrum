//! A diff flattened into rows (file headers, hunk headers, lines) for a
//! virtualized list, and how one row is drawn.

use ferrumgit_domain::{FileDiff, LineKind};
use gpui::{Hsla, SharedString};

use crate::theme_apply::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    File,
    Hunk,
    Context,
    Added,
    Removed,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffRow {
    pub kind: RowKind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: SharedString,
}

/// Rows for every file of a diff.
pub fn rows(files: &[FileDiff]) -> Vec<DiffRow> {
    let mut out = Vec::new();
    for file in files {
        let name = match &file.old_path {
            Some(old) => format!("{old} → {}", file.path),
            None => file.path.clone(),
        };
        out.push(DiffRow {
            kind: RowKind::File,
            old: None,
            new: None,
            text: format!("{name}   +{} −{}", file.additions, file.deletions).into(),
        });
        if file.binary {
            out.push(note("Binary file"));
            continue;
        }
        if file.hunks.is_empty() {
            out.push(note("No content changes"));
        }
        for hunk in &file.hunks {
            out.push(DiffRow {
                kind: RowKind::Hunk,
                old: None,
                new: None,
                text: hunk.header.clone().into(),
            });
            for line in &hunk.lines {
                let kind = match line.kind {
                    LineKind::Context => RowKind::Context,
                    LineKind::Added => RowKind::Added,
                    LineKind::Removed => RowKind::Removed,
                    LineKind::NoNewline => RowKind::Note,
                };
                let text = if line.kind == LineKind::NoNewline {
                    "No newline at end of file".to_string()
                } else {
                    line.content.replace('\t', "    ")
                };
                out.push(DiffRow {
                    kind,
                    old: line.old_no,
                    new: line.new_no,
                    text: text.into(),
                });
            }
        }
    }
    out
}

fn note(text: &str) -> DiffRow {
    DiffRow {
        kind: RowKind::Note,
        old: None,
        new: None,
        text: SharedString::from(text.to_string()),
    }
}

/// (background, text) colours of a row.
pub fn colors(kind: RowKind, theme: &ferrumeditor_core::theme::Theme) -> (Option<Hsla>, Hsla) {
    let text = ui(theme, "text");
    match kind {
        RowKind::File => (Some(ui(theme, "panel")), text),
        RowKind::Hunk => (Some(ui(theme, "element_hover")), ui(theme, "text_muted")),
        RowKind::Added => (Some(ui(theme, "git_added").opacity(0.14)), text),
        RowKind::Removed => (Some(ui(theme, "git_deleted").opacity(0.14)), text),
        RowKind::Context => (None, text),
        RowKind::Note => (None, ui(theme, "text_placeholder")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumgit_domain::{ChangeKind, DiffLine, Hunk};

    #[test]
    fn flattens_files_hunks_and_lines() {
        let file = FileDiff {
            path: "src/main.rs".into(),
            change: ChangeKind::Modified,
            additions: 1,
            deletions: 1,
            hunks: vec![Hunk {
                header: "@@ -1,2 +1,2 @@".into(),
                lines: vec![
                    DiffLine {
                        kind: LineKind::Removed,
                        content: "\told".into(),
                        old_no: Some(1),
                        new_no: None,
                    },
                    DiffLine {
                        kind: LineKind::Added,
                        content: "new".into(),
                        old_no: None,
                        new_no: Some(1),
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let rows = rows(&[file]);
        let kinds: Vec<RowKind> = rows.iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds,
            vec![
                RowKind::File,
                RowKind::Hunk,
                RowKind::Removed,
                RowKind::Added
            ]
        );
        assert_eq!(rows[0].text.as_ref(), "src/main.rs   +1 −1");
        assert_eq!(rows[2].text.as_ref(), "    old");
        assert_eq!(rows[3].new, Some(1));
    }
}
