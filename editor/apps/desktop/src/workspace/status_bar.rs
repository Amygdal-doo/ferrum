//! The status bar: file path, git branch, cursor position, selection,
//! language, indentation, line endings, encoding, problem counts, the
//! language server's state, and the terminal toggle in the bottom-right
//! corner.

use ferrum_ui::prelude::*;
use ui::{Icon, IconName, IconSize};

use super::Workspace;

impl Workspace {
    pub(super) fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut left = h_flex().gap_3().min_w_0();
        let mut right = h_flex().gap_3().flex_none();
        if let Some(editor) = self.active_editor() {
            let e = editor.read(cx);
            let path = match (&e.path, &self.root) {
                (Some(path), Some(root)) => path
                    .strip_prefix(root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .to_string(),
                (Some(path), None) => path.to_string_lossy().to_string(),
                (None, _) => "untitled".into(),
            };
            left = left.child(
                Label::new(path)
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .truncate(),
            );
            let buffer = e.state.buffer();
            let head = e.state.primary().head;
            let point = buffer.offset_to_point(head);
            let column = buffer.line(point.row)[..point.col].chars().count() + 1;
            let selections = e.state.selections();
            let selection = if selections.len() > 1 {
                format!("{} selections", selections.len())
            } else if !selections[0].is_empty() {
                let text = buffer.slice_cow(selections[0].range());
                let lines = text.matches('\n').count();
                if lines > 0 {
                    format!("({} chars, {} lines)", text.chars().count(), lines + 1)
                } else {
                    format!("({} chars)", text.chars().count())
                }
            } else {
                String::new()
            };
            let settings = &e.state.settings;
            let indent = if settings.soft_tabs {
                format!("Spaces: {}", settings.tab_size)
            } else {
                format!("Tab Size: {}", settings.tab_size)
            };
            let items = [
                format!("Ln {}, Col {column}", point.row + 1),
                selection,
                e.language().name().to_string(),
                indent,
                e.line_ending.label().to_string(),
                if e.bom {
                    "UTF-8 BOM".into()
                } else {
                    "UTF-8".into()
                },
                if e.soft_wrap {
                    "Wrap".into()
                } else {
                    String::new()
                },
            ];
            for item in items.into_iter().filter(|s| !s.is_empty()) {
                right = right.child(Label::new(item).size(LabelSize::Small));
            }
        } else if let Some(root) = &self.root {
            left = left.child(
                Label::new(root.to_string_lossy().to_string())
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            );
        }
        if let Some(branch) = &self.branch {
            left = left.child(
                h_flex()
                    .gap_1()
                    .flex_none()
                    .child(
                        Icon::new(IconName::GitBranch)
                            .size(IconSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(Label::new(branch.clone()).size(LabelSize::Small)),
            );
        }
        right = right.children(self.render_lsp_status(cx));
        right = right.child(self.render_terminal_button(cx));
        status_bar(cx).child(left).child(right)
    }
}
