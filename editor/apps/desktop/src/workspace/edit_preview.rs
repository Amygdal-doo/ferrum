//! The rename preview: every file a workspace edit touches, with its
//! changed lines before and after, in a dialog. Enter (or *Apply*) applies
//! the edit; Escape (or *Cancel*) drops it.

use std::path::Path;

use ferrum_ui::Modal;
use ferrum_ui::components::{Button, ButtonCommon, ButtonStyle, Clickable};
use ferrum_ui::prelude::*;
use ferrumeditor_core::files;
use ferrumeditor_core::lsp::preview::{FileChangeKind, FilePreview, preview_workspace_edit};
use ferrumeditor_core::lsp::{PositionEncoding, WorkspaceEdit};
use gpui::{Hsla, KeyDownEvent};

use super::{Dialog, Workspace};
use crate::editor::EditorView;

/// A workspace edit waiting for Apply / Cancel.
pub struct EditPreview {
    editor: Entity<EditorView>,
    edit: WorkspaceEdit,
    encoding: PositionEncoding,
    label: String,
    files: Vec<FilePreview>,
    focus: FocusHandle,
}

impl Workspace {
    /// Shows `edit` (from a rename in `editor`) for review; nothing changes
    /// until it is applied.
    pub(super) fn preview_edit(
        &mut self,
        editor: &Entity<EditorView>,
        edit: &WorkspaceEdit,
        encoding: PositionEncoding,
        label: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Open buffers first (they may be unsaved), then the disk.
        let open: Vec<(std::path::PathBuf, String)> = self
            .all_editors()
            .iter()
            .filter_map(|e| {
                let e = e.read(cx);
                Some((e.path.clone()?, e.state.text()))
            })
            .collect();
        let text_of = |path: &Path| {
            open.iter()
                .find(|(p, _)| p == path)
                .map(|(_, text)| text.clone())
                .or_else(|| files::load(path).ok().map(|f| f.text))
        };
        let files = match preview_workspace_edit(edit, encoding, text_of) {
            Ok(files) => files,
            Err(error) => {
                self.error(format!("{label} failed: {error}"), cx);
                return;
            }
        };
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.dialog = Some(Dialog::EditPreview(Box::new(EditPreview {
            editor: editor.clone(),
            edit: edit.clone(),
            encoding,
            label: label.to_string(),
            files,
            focus,
        })));
        cx.notify();
    }

    /// Applies the previewed edit.
    pub(super) fn accept_edit_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::EditPreview(preview)) = self.dialog.take() else {
            return;
        };
        let EditPreview {
            editor,
            edit,
            encoding,
            label,
            ..
        } = *preview;
        self.apply_editor_edit(&editor, &edit, encoding, &label, None, cx);
        editor.update(cx, |e, cx| e.focus(window, cx));
        cx.notify();
    }

    pub(super) fn cancel_edit_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Dialog::EditPreview(preview)) = self.dialog.take() {
            preview.editor.update(cx, |e, cx| e.focus(window, cx));
            cx.notify();
        }
    }

    pub(super) fn render_edit_preview(
        &self,
        preview: &EditPreview,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let edits = preview.edit.edit_count();
        let file_count = preview.files.len();
        let plural = |n: usize| if n == 1 { "" } else { "s" };
        let summary = format!(
            "{edits} change{} in {file_count} file{}",
            plural(edits),
            plural(file_count)
        );
        let mut body = v_flex()
            .id("edit-preview")
            .track_focus(&preview.focus)
            .gap_2()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "enter" => this.accept_edit_preview(window, cx),
                    "escape" => this.cancel_edit_preview(window, cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(
                Label::new(summary)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            );
        for file in &preview.files {
            let (detail, lines, more) = match &file.kind {
                FileChangeKind::Edit { edits, lines, more } => (
                    format!("{edits} edit{}", plural(*edits)),
                    lines.as_slice(),
                    *more,
                ),
                FileChangeKind::Create => ("new file".to_string(), &[][..], 0),
                FileChangeKind::Rename { to } => {
                    (format!("→ {}", self.relative_path(to)), &[][..], 0)
                }
                FileChangeKind::Delete => ("deleted".to_string(), &[][..], 0),
            };
            let mut block = v_flex().gap_0p5().child(
                h_flex()
                    .gap_2()
                    .child(Label::new(self.relative_path(&file.path)).size(LabelSize::Small))
                    .child(
                        Label::new(detail)
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            );
            for change in lines {
                let row = |sign: &'static str, text: &str, color: Hsla| {
                    h_flex()
                        .gap_2()
                        .px_1()
                        .rounded_sm()
                        .bg(color.opacity(0.12))
                        .child(
                            div().w(px(44.)).flex_none().child(
                                Label::new(format!("{sign}{}", change.line + 1))
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            ),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .font_buffer(cx)
                                .text_ui_sm(cx)
                                .child(text.trim_start().to_string()),
                        )
                };
                if let Some(before) = &change.before {
                    block = block.child(row("−", before, colors.version_control_deleted));
                }
                if let Some(after) = &change.after {
                    block = block.child(row("+", after, colors.version_control_added));
                }
            }
            if more > 0 {
                block = block.child(
                    Label::new(format!("… {more} more changed line{}", plural(more)))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                );
            }
            body = body.child(block);
        }
        Modal::new("Preview Changes")
            .width(px(720.))
            .on_dismiss(cx.listener(|this, _, window, cx| this.cancel_edit_preview(window, cx)))
            .child(body)
            .footer(
                Button::new("edit-preview-cancel", "Cancel").on_click(
                    cx.listener(|this, _, window, cx| this.cancel_edit_preview(window, cx)),
                ),
            )
            .footer(
                Button::new("edit-preview-apply", "Apply")
                    .style(ButtonStyle::Filled)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.accept_edit_preview(window, cx)),
                    ),
            )
            .into_any_element()
    }

    /// `path` relative to the project folder.
    fn relative_path(&self, path: &Path) -> String {
        self.root
            .as_deref()
            .and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    }
}
