//! Language server results that span files: applying workspace edits
//! (rename, code actions, the server's `workspace/applyEdit`), the
//! references panel, the code action picker and format on save.
//!
//! Edits to open files go through their editor (one undo step per
//! buffer); files that are not open are edited on disk and the servers are
//! told with `workspace/didChangeWatchedFiles`.

use std::path::{Path, PathBuf};

use ferrum_ui::prelude::*;
use ferrumeditor_core::files::{self, SaveOptions};
use ferrumeditor_core::lsp::edit::apply_to_text;
use ferrumeditor_core::lsp::features::{CodeAction, LspCommand, group_references};
use ferrumeditor_core::lsp::{EditOp, Location, PositionEncoding, WorkspaceEdit, uri_to_path};
use serde_json::{Value, json};

use super::Workspace;
use crate::editor::EditorView;
use crate::lsp_store::LspStore;
use crate::picker::{PickerItem, PickerKind};
use crate::settings::Settings;

/// What a workspace edit changed.
#[derive(Debug, Default)]
pub struct EditSummary {
    pub edits: usize,
    pub files: usize,
    /// Files edited on disk (not open).
    pub on_disk: Vec<PathBuf>,
}

impl Workspace {
    fn editor_for(&self, path: &Path, cx: &App) -> Option<Entity<EditorView>> {
        self.all_editors()
            .into_iter()
            .find(|e| e.read(cx).path.as_deref() == Some(path))
    }

    /// Applies `edit` in order. Stops at the first failure; what was done
    /// before it stays (each open file can be undone on its own).
    pub fn apply_workspace_edit(
        &mut self,
        edit: &WorkspaceEdit,
        encoding: PositionEncoding,
        cx: &mut Context<Self>,
    ) -> Result<EditSummary, String> {
        let mut summary = EditSummary::default();
        let path_of = |uri: &str| uri_to_path(uri).ok_or_else(|| format!("not a file: {uri}"));
        for op in &edit.ops {
            match op {
                EditOp::Text { uri, edits } => {
                    if edits.is_empty() {
                        continue;
                    }
                    let path = path_of(uri)?;
                    if let Some(editor) = self.editor_for(&path, cx) {
                        editor.update(cx, |e, cx| e.apply_lsp_edits(edits, encoding, cx))?;
                    } else {
                        let file =
                            files::load(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                        let text = apply_to_text(&file.text, edits, encoding)?;
                        let options = SaveOptions {
                            line_ending: file.line_ending,
                            bom: file.bom,
                            trim_trailing_whitespace: false,
                            ensure_final_newline: false,
                        };
                        files::write_atomic(&path, &files::encode(&text, options))
                            .map_err(|e| format!("{}: {e}", path.display()))?;
                        summary.on_disk.push(path);
                    }
                    summary.edits += edits.len();
                    summary.files += 1;
                }
                EditOp::Create {
                    uri,
                    overwrite,
                    ignore_if_exists,
                } => {
                    let path = path_of(uri)?;
                    if path.exists() && (*ignore_if_exists || !*overwrite) {
                        continue;
                    }
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    std::fs::write(&path, "").map_err(|e| e.to_string())?;
                    summary.files += 1;
                }
                EditOp::Rename {
                    old_uri,
                    new_uri,
                    overwrite,
                    ignore_if_exists,
                } => {
                    let (old, new) = (path_of(old_uri)?, path_of(new_uri)?);
                    if new.exists() && (*ignore_if_exists || !*overwrite) {
                        continue;
                    }
                    if let Some(parent) = new.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    std::fs::rename(&old, &new).map_err(|e| e.to_string())?;
                    if let Some(editor) = self.editor_for(&old, cx) {
                        editor.update(cx, |e, cx| e.moved_to(new.clone(), cx));
                    }
                    summary.files += 1;
                }
                EditOp::Delete {
                    uri,
                    recursive,
                    ignore_if_not_exists,
                } => {
                    let path = path_of(uri)?;
                    if !path.exists() && *ignore_if_not_exists {
                        continue;
                    }
                    let result = if path.is_dir() && *recursive {
                        std::fs::remove_dir_all(&path)
                    } else if path.is_dir() {
                        std::fs::remove_dir(&path)
                    } else {
                        std::fs::remove_file(&path)
                    };
                    result.map_err(|e| format!("{}: {e}", path.display()))?;
                    summary.files += 1;
                }
            }
        }
        if let Some(store) = LspStore::global(cx) {
            store.read(cx).files_changed(&summary.on_disk);
        }
        cx.notify();
        Ok(summary)
    }

    /// A rename / code action from `editor`: apply its edit, report, then
    /// run its command on the editor's server.
    pub(super) fn apply_editor_edit(
        &mut self,
        editor: &Entity<EditorView>,
        edit: &WorkspaceEdit,
        encoding: PositionEncoding,
        label: &str,
        command: Option<&LspCommand>,
        cx: &mut Context<Self>,
    ) {
        match self.apply_workspace_edit(edit, encoding, cx) {
            Ok(summary) if summary.edits + summary.files > 0 => {
                let plural = |n: usize| if n == 1 { "" } else { "s" };
                let mut message = format!(
                    "{label}: {} edit{} in {} file{}",
                    summary.edits,
                    plural(summary.edits),
                    summary.files,
                    plural(summary.files)
                );
                if !summary.on_disk.is_empty() {
                    message.push_str(&format!(
                        " ({} saved directly, not open)",
                        summary.on_disk.len()
                    ));
                }
                self.info(message, cx);
            }
            Ok(_) => {}
            Err(error) => {
                self.error(format!("{label} failed: {error}"), cx);
                return;
            }
        }
        if let Some(command) = command {
            let command = command.clone();
            editor.update(cx, |e, cx| e.execute_command(command, cx));
        }
    }

    /// `workspace/applyEdit` from a server (usually while it runs a
    /// command): apply and answer.
    pub(super) fn apply_server_edit(
        &mut self,
        server_id: u64,
        request_id: &Value,
        edit: &Value,
        cx: &mut Context<Self>,
    ) {
        let Some(store) = LspStore::global(cx) else {
            return;
        };
        let encoding = store
            .read(cx)
            .client(server_id)
            .map_or(PositionEncoding::Utf16, |c| c.position_encoding());
        let edit = ferrumeditor_core::lsp::edit::parse_workspace_edit(edit);
        let result = match self.apply_workspace_edit(&edit, encoding, cx) {
            Ok(_) => json!({ "applied": true }),
            Err(error) => {
                self.error(format!("Could not apply the server's edit: {error}"), cx);
                json!({ "applied": false, "failureReason": error })
            }
        };
        store.read(cx).reply(server_id, request_id.clone(), result);
    }

    // ----- References -------------------------------------------------------------

    /// Lists references in the find-in-files panel, grouped by file.
    pub(super) fn show_references(
        &mut self,
        locations: &[Location],
        encoding: PositionEncoding,
        symbol: &str,
        cx: &mut Context<Self>,
    ) {
        let places: Vec<(PathBuf, ferrumeditor_core::lsp::LspRange)> = locations
            .iter()
            .filter_map(|l| Some((uri_to_path(&l.uri)?, l.range)))
            .collect();
        // Open buffers first (they may be unsaved), then the disk.
        let open: Vec<(PathBuf, Vec<String>)> = self
            .all_editors()
            .iter()
            .filter_map(|e| {
                let e = e.read(cx);
                let text = e.state.text();
                Some((
                    e.path.clone()?,
                    text.split('\n').map(str::to_string).collect(),
                ))
            })
            .collect();
        let mut lines = |path: &Path| {
            open.iter()
                .find(|(p, _)| p == path)
                .map(|(_, lines)| lines.clone())
                .or_else(|| {
                    files::load(path)
                        .ok()
                        .map(|f| f.text.split('\n').map(str::to_string).collect())
                })
        };
        let result = group_references(&places, encoding, &mut lines);
        let count = result.total_matches;
        let file_count = result.files.len();
        let status = format!(
            "{count} reference{} to `{symbol}` in {file_count} file{}",
            if count == 1 { "" } else { "s" },
            if file_count == 1 { "" } else { "s" }
        );
        self.project_find
            .update(cx, |pf, cx| pf.show_results(result, status, cx));
        self.project_find_visible = true;
        cx.notify();
    }

    // ----- Code actions -------------------------------------------------------------

    /// Shows the actions in a picker; the choice runs on `editor`.
    pub(super) fn pick_code_action(
        &mut self,
        editor: &Entity<EditorView>,
        actions: &[CodeAction],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.code_actions = Some((editor.clone(), actions.to_vec()));
        self.location_items = actions
            .iter()
            .enumerate()
            .map(|(index, action)| {
                PickerItem::new(action.title.clone(), action.kind_label(), index.to_string())
            })
            .collect();
        if self.picker.is_some() {
            self.close_picker(window, cx);
        }
        self.toggle_picker(PickerKind::CodeActions, window, cx);
    }

    pub(super) fn run_code_action_payload(&mut self, payload: &str, cx: &mut Context<Self>) {
        let Some((editor, actions)) = self.code_actions.take() else {
            return;
        };
        if let Some(action) = payload.parse::<usize>().ok().and_then(|i| actions.get(i)) {
            let action = action.clone();
            editor.update(cx, |e, cx| e.run_code_action(action, cx));
        }
    }

    // ----- Format on save -----------------------------------------------------------

    /// `core:save`: formats first when format on save applies to the file
    /// and its server can format, then saves.
    pub(super) fn save_formatted(
        &mut self,
        editor: Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let wanted = {
            let e = editor.read(cx);
            e.path.is_some()
                && cx
                    .global::<Settings>()
                    .config
                    .format_on_save_for(e.language())
        };
        let task = if wanted {
            editor.update(cx, |e, cx| e.format_document(false, cx))
        } else {
            None
        };
        match task {
            None => self.save_editor(editor, false, None, window, cx),
            Some(task) => cx
                .spawn_in(window, async move |this, cx| {
                    task.await;
                    this.update_in(cx, |this, window, cx| {
                        this.save_editor(editor, false, None, window, cx);
                    })
                    .ok();
                })
                .detach(),
        }
    }

    /// `core:save-all`, formatting each file like `core:save`.
    pub(super) fn save_all_formatted(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for editor in self.all_editors() {
            let (dirty, has_path) = {
                let e = editor.read(cx);
                (e.is_dirty(), e.path.is_some())
            };
            if dirty && has_path {
                self.save_formatted(editor, window, cx);
            }
        }
    }
}
