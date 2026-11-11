//! Answers for the editing features beyond completion and hover: rename
//! (`prepareRename`), code actions, signature help and references grouped
//! by file for the results panel. Parsed leniently, like [`super::types`].

use std::ops::Range;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::edit::{WorkspaceEdit, parse_workspace_edit};
use super::position::{LspRange, PositionEncoding, units_to_column};
use super::types::markup_text;
use crate::project_search::{FileMatch, LineMatch, ProjectSearchResult};

/// Whether a server capability is on (`true` or an options object).
pub fn has_capability(capabilities: &Value, key: &str) -> bool {
    !matches!(&capabilities[key], Value::Null | Value::Bool(false))
}

// ----- Rename -------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrepareRename {
    /// Rename this range; the input starts with `placeholder` if given.
    Range {
        range: LspRange,
        placeholder: Option<String>,
    },
    /// The server accepts the position: use the word at the caret.
    DefaultBehavior,
    /// Nothing renameable here.
    Invalid,
}

/// A `textDocument/prepareRename` answer: `Range`, `{range, placeholder}`,
/// `{defaultBehavior}` or `null`.
pub fn parse_prepare_rename(value: &Value) -> PrepareRename {
    if value["defaultBehavior"].as_bool() == Some(true) {
        return PrepareRename::DefaultBehavior;
    }
    if let Some(range) = value
        .get("range")
        .and_then(|r| serde_json::from_value::<LspRange>(r.clone()).ok())
    {
        return PrepareRename::Range {
            range,
            placeholder: value["placeholder"].as_str().map(str::to_string),
        };
    }
    match serde_json::from_value::<LspRange>(value.clone()) {
        Ok(range) => PrepareRename::Range {
            range,
            placeholder: None,
        },
        Err(_) => PrepareRename::Invalid,
    }
}

// ----- Code actions -------------------------------------------------------

/// A server command (`Command`): run with `workspace/executeCommand`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspCommand {
    pub title: String,
    pub command: String,
    pub arguments: Value,
}

impl LspCommand {
    fn parse(value: &Value) -> Option<Self> {
        Some(Self {
            title: value["title"].as_str().unwrap_or_default().to_string(),
            command: value["command"].as_str()?.to_string(),
            arguments: value.get("arguments").cloned().unwrap_or(Value::Null),
        })
    }

    /// `workspace/executeCommand` params.
    pub fn params(&self) -> Value {
        let mut params = json!({ "command": self.command });
        if !self.arguments.is_null() {
            params["arguments"] = self.arguments.clone();
        }
        params
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub preferred: bool,
    pub edit: Option<WorkspaceEdit>,
    pub command: Option<LspCommand>,
    /// The action as sent, for `codeAction/resolve`.
    pub raw: Value,
}

impl CodeAction {
    fn parse(value: &Value) -> Option<Self> {
        let title = value["title"].as_str()?.to_string();
        // A bare `Command` has a string `command`.
        if value["command"].is_string() {
            return Some(Self {
                title,
                kind: None,
                preferred: false,
                edit: None,
                command: LspCommand::parse(value),
                raw: value.clone(),
            });
        }
        if !value["disabled"].is_null() {
            return None;
        }
        Some(Self {
            title,
            kind: value["kind"].as_str().map(str::to_string),
            preferred: value["isPreferred"].as_bool().unwrap_or(false),
            edit: value.get("edit").map(parse_workspace_edit),
            command: LspCommand::parse(&value["command"]),
            raw: value.clone(),
        })
    }

    /// Fills in a `codeAction/resolve` answer.
    pub fn apply_resolved(&mut self, resolved: &Value) {
        if let Some(new) = Self::parse(resolved) {
            if new.edit.is_some() {
                self.edit = new.edit;
            }
            if new.command.is_some() {
                self.command = new.command;
            }
        }
    }

    /// Neither an edit nor a command yet (the server fills them in on
    /// `codeAction/resolve`).
    pub fn needs_resolve(&self) -> bool {
        self.edit.is_none() && self.command.is_none()
    }

    /// Short tag for the list: "quick fix", "refactor", …
    pub fn kind_label(&self) -> &'static str {
        match self.kind.as_deref().unwrap_or_default() {
            k if k.starts_with("quickfix") => "quick fix",
            k if k.starts_with("refactor.extract") => "extract",
            k if k.starts_with("refactor.inline") => "inline",
            k if k.starts_with("refactor.rewrite") => "rewrite",
            k if k.starts_with("refactor") => "refactor",
            k if k.starts_with("source.organizeImports") => "organize imports",
            k if k.starts_with("source") => "source",
            _ => "",
        }
    }
}

/// A `textDocument/codeAction` answer: preferred fixes first, then quick
/// fixes, then the rest in the server's order; disabled actions dropped.
pub fn parse_code_actions(value: &Value) -> Vec<CodeAction> {
    let mut actions: Vec<CodeAction> = value
        .as_array()
        .map(|items| items.iter().filter_map(CodeAction::parse).collect())
        .unwrap_or_default();
    actions.sort_by_key(|a| (!a.preferred, a.kind_label() != "quick fix"));
    actions
}

// ----- Signature help -----------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureHelp {
    /// The active signature, e.g. `fn add(a: i32, b: i32) -> i32`.
    pub label: String,
    /// Byte range of the active parameter within `label`.
    pub active_parameter: Option<Range<usize>>,
    pub documentation: Option<String>,
    /// "2/3" when there are overloads.
    pub overloads: Option<String>,
}

/// A `textDocument/signatureHelp` answer (`None` when empty).
pub fn parse_signature_help(value: &Value) -> Option<SignatureHelp> {
    let signatures = value["signatures"].as_array()?;
    let index = value["activeSignature"].as_u64().unwrap_or(0) as usize;
    let index = index.min(signatures.len().checked_sub(1)?);
    let signature = &signatures[index];
    let label = signature["label"].as_str()?.to_string();
    let parameter_index = signature["activeParameter"]
        .as_u64()
        .or_else(|| value["activeParameter"].as_u64())
        .unwrap_or(0) as usize;
    let active_parameter = signature["parameters"]
        .as_array()
        .and_then(|params| params.get(parameter_index))
        .and_then(|param| parameter_range(&label, &param["label"]));
    let documentation = signature
        .get("documentation")
        .and_then(markup_text)
        .or_else(|| {
            signature["parameters"]
                .get(parameter_index)
                .and_then(|p| p.get("documentation"))
                .and_then(markup_text)
        });
    Some(SignatureHelp {
        label,
        active_parameter,
        documentation,
        overloads: (signatures.len() > 1).then(|| format!("{}/{}", index + 1, signatures.len())),
    })
}

/// A parameter label: a substring of the signature, or `[start, end]` in
/// UTF-16 units.
fn parameter_range(label: &str, param: &Value) -> Option<Range<usize>> {
    match param {
        Value::String(text) if !text.is_empty() => {
            let start = label.find(text.as_str())?;
            Some(start..start + text.len())
        }
        Value::Array(bounds) => {
            let start = bounds.first()?.as_u64()? as u32;
            let end = bounds.get(1)?.as_u64()? as u32;
            let start = units_to_column(label, start, PositionEncoding::Utf16);
            let end = units_to_column(label, end, PositionEncoding::Utf16);
            (start < end).then_some(start..end)
        }
        _ => None,
    }
}

// ----- References ---------------------------------------------------------

/// Longest line shown in the references panel (a window around the match).
const PREVIEW_LEN: usize = 240;

/// References grouped by file (files sorted, lines in order) in the shape
/// of find-in-files results. `lines` gives a file's text by line (from an
/// open buffer or the disk); files it can't read still list their lines.
pub fn group_references(
    locations: &[(PathBuf, LspRange)],
    encoding: PositionEncoding,
    lines: &mut dyn FnMut(&Path) -> Option<Vec<String>>,
) -> ProjectSearchResult {
    let mut sorted: Vec<&(PathBuf, LspRange)> = locations.iter().collect();
    sorted.sort_by(|a, b| (&a.0, a.1.start).cmp(&(&b.0, b.1.start)));
    sorted.dedup();
    let mut result = ProjectSearchResult::default();
    for (path, range) in sorted {
        if result.files.last().is_none_or(|f| f.path != *path) {
            result.files.push(FileMatch {
                path: path.clone(),
                matches: Vec::new(),
            });
        }
        let text = lines(path);
        let line = text
            .as_ref()
            .and_then(|l| l.get(range.start.line as usize))
            .cloned()
            .unwrap_or_default();
        let start = units_to_column(&line, range.start.character, encoding);
        let end = if range.end.line == range.start.line {
            units_to_column(&line, range.end.character, encoding).max(start)
        } else {
            line.len()
        };
        let (line_text, line_offset) = window(&line, start);
        let byte_range = start - line_offset..(end - line_offset).min(line_text.len());
        if let Some(file) = result.files.last_mut() {
            file.matches.push(LineMatch {
                line: range.start.line as usize,
                byte_range,
                line_text,
                line_offset,
            });
        }
        result.total_matches += 1;
    }
    result.files_searched = result.files.len();
    result
}

/// `line`, or a window of it around byte `at` when it is very long.
fn window(line: &str, at: usize) -> (String, usize) {
    if line.len() <= PREVIEW_LEN {
        return (line.to_string(), 0);
    }
    let mut start = at.saturating_sub(PREVIEW_LEN / 3);
    while !line.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (start + PREVIEW_LEN).min(line.len());
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    (line[start..end].to_string(), start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::position::Position;

    fn range(l0: u32, c0: u32, l1: u32, c1: u32) -> Value {
        json!({"start": {"line": l0, "character": c0}, "end": {"line": l1, "character": c1}})
    }

    #[test]
    fn capabilities_accept_true_or_options() {
        let caps = json!({"renameProvider": {"prepareProvider": true}, "hoverProvider": false,
                          "documentFormattingProvider": true});
        assert!(has_capability(&caps, "renameProvider"));
        assert!(has_capability(&caps, "documentFormattingProvider"));
        assert!(!has_capability(&caps, "hoverProvider"));
        assert!(!has_capability(&caps, "referencesProvider"));
    }

    #[test]
    fn prepare_rename_shapes() {
        let plain = parse_prepare_rename(&range(1, 3, 1, 6));
        assert!(
            matches!(plain, PrepareRename::Range { placeholder: None, range }
            if range.start == Position::new(1, 3))
        );
        let with_placeholder =
            parse_prepare_rename(&json!({"range": range(0, 0, 0, 3), "placeholder": "foo"}));
        assert!(
            matches!(with_placeholder, PrepareRename::Range { placeholder: Some(p), .. } if p == "foo")
        );
        assert_eq!(
            parse_prepare_rename(&json!({"defaultBehavior": true})),
            PrepareRename::DefaultBehavior
        );
        assert_eq!(parse_prepare_rename(&Value::Null), PrepareRename::Invalid);
    }

    #[test]
    fn code_actions_sorted_and_commands_parsed() {
        let actions = parse_code_actions(&json!([
            {"title": "Extract into function", "kind": "refactor.extract", "data": {"x": 1}},
            {"title": "Organize imports", "command": "ts.organize", "arguments": ["a.ts"]},
            {"title": "Import `HashMap`", "kind": "quickfix",
             "edit": {"changes": {"file:///a.rs": [{"range": range(0, 0, 0, 0), "newText": "use std::collections::HashMap;\n"}]}}},
            {"title": "Remove unused", "kind": "quickfix", "isPreferred": true,
             "command": {"title": "x", "command": "fix.remove", "arguments": [1]}},
            {"title": "Can't", "disabled": {"reason": "no"}}
        ]));
        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Remove unused",
                "Import `HashMap`",
                "Extract into function",
                "Organize imports"
            ]
        );
        assert_eq!(
            actions[0].command.as_ref().map(|c| c.command.as_str()),
            Some("fix.remove")
        );
        assert_eq!(
            actions[1].edit.as_ref().map(WorkspaceEdit::edit_count),
            Some(1)
        );
        assert_eq!(actions[1].kind_label(), "quick fix");
        assert!(actions[2].needs_resolve());
        assert_eq!(
            actions[3].command.as_ref().map(LspCommand::params),
            Some(json!({"command": "ts.organize", "arguments": ["a.ts"]}))
        );
        let mut extract = actions[2].clone();
        extract.apply_resolved(&json!({"title": "Extract into function",
            "edit": {"changes": {"file:///a.rs": []}}}));
        assert!(!extract.needs_resolve());
    }

    #[test]
    fn signature_help_marks_the_active_parameter() {
        let help = parse_signature_help(&json!({
            "signatures": [
                {"label": "fn add(a: i32, b: i32) -> i32",
                 "parameters": [{"label": "a: i32"}, {"label": "b: i32"}],
                 "documentation": {"kind": "markdown", "value": "Adds."}},
                {"label": "fn add3(a, b, c)", "parameters": [{"label": [9, 10]}, {"label": [12, 13]}]}
            ],
            "activeSignature": 0, "activeParameter": 1
        }))
        .expect("help");
        assert_eq!(
            &help.label[help.active_parameter.clone().expect("param")],
            "b: i32"
        );
        assert_eq!(help.documentation.as_deref(), Some("Adds."));
        assert_eq!(help.overloads.as_deref(), Some("1/2"));
        let offsets = parse_signature_help(&json!({
            "signatures": [{"label": "f(😀, b)", "parameters": [{"label": [2, 4]}, {"label": [6, 7]}],
                            "activeParameter": 1}]
        }))
        .expect("help");
        assert_eq!(
            &offsets.label[offsets.active_parameter.clone().expect("param")],
            "b"
        );
        assert!(parse_signature_help(&json!({"signatures": []})).is_none());
        assert!(parse_signature_help(&Value::Null).is_none());
    }

    #[test]
    fn references_group_by_file_with_previews() {
        let at = |l: u32, c0: u32, c1: u32| LspRange {
            start: Position::new(l, c0),
            end: Position::new(l, c1),
        };
        let a = PathBuf::from("/p/a.rs");
        let b = PathBuf::from("/p/b.rs");
        let locations = vec![
            (b.clone(), at(1, 4, 7)),
            (a.clone(), at(2, 0, 3)),
            (a.clone(), at(0, 3, 6)),
            (a.clone(), at(0, 3, 6)),
        ];
        let mut lines = |path: &Path| {
            (path == a).then(|| vec!["fn foo() {}".into(), String::new(), "foo();".into()])
        };
        let result = group_references(&locations, PositionEncoding::Utf16, &mut lines);
        assert_eq!(result.total_matches, 3);
        assert_eq!(result.files.len(), 2);
        assert_eq!(result.files[0].path, a);
        let first = &result.files[0].matches[0];
        assert_eq!(first.line, 0);
        assert_eq!(&first.line_text[first.byte_range.clone()], "foo");
        assert_eq!(result.files[0].matches[1].line, 2);
        // b.rs couldn't be read: the line is listed without a preview.
        assert_eq!(result.files[1].matches[0].line_text, "");
    }
}
