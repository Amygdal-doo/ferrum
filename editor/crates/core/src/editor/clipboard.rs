//! Copy / cut / paste across multiple cursors.

use super::{EditorState, Plan};
use crate::history::EditKind;
use crate::selection::Selection;

/// Text copied from the editor; `line_mode` when whole lines were copied
/// from carets (pasting then inserts above the caret's line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardText {
    pub text: String,
    pub line_mode: bool,
}

impl EditorState {
    /// Text for Copy. With only carets, copies their whole lines.
    pub fn copy(&self) -> ClipboardText {
        if self.selections().iter().all(Selection::is_empty) {
            let mut rows: Vec<usize> = self
                .selections()
                .iter()
                .map(|s| self.buffer().row_of(s.head))
                .collect();
            rows.dedup();
            let text = rows
                .iter()
                .map(|row| format!("{}\n", self.buffer().line(*row)))
                .collect();
            return ClipboardText {
                text,
                line_mode: true,
            };
        }
        let parts: Vec<String> = self
            .selections()
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| self.buffer().slice(s.range()))
            .collect();
        ClipboardText {
            text: parts.join("\n"),
            line_mode: false,
        }
    }

    pub fn cut(&mut self) -> ClipboardText {
        let copied = self.copy();
        if copied.line_mode {
            self.delete_lines();
        } else {
            self.apply_plans(EditKind::Other, |_, sel| Plan::replace(sel.range(), "", 0));
        }
        copied
    }

    /// Pastes; one line per cursor when the line count matches the cursor
    /// count. Line-mode text is inserted above the caret's line.
    pub fn paste(&mut self, text: &str, line_mode: bool) {
        let text = crate::buffer::normalize_newlines(text).into_owned();
        let count = self.selections().len();
        let mut pieces: Vec<String> = if line_mode {
            text.split_inclusive('\n').map(str::to_string).collect()
        } else {
            text.split('\n').map(str::to_string).collect()
        };
        if count == 1 || pieces.len() != count {
            pieces = vec![text.clone(); count];
        }
        let mut index = 0;
        self.apply_plans(EditKind::Other, |state, sel| {
            let piece = pieces.get(index).cloned().unwrap_or_default();
            index += 1;
            if line_mode && sel.is_empty() && piece.ends_with('\n') {
                let start = state.buffer().line_start(state.buffer().row_of(sel.head));
                return Plan::replace(start..start, piece.clone(), piece.len() + sel.head - start);
            }
            let len = piece.len();
            Plan::replace(sel.range(), piece, len)
        });
    }
}
