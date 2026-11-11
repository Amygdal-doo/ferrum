//! Tab / Shift+Tab and indenting or outdenting lines.

use super::{Change, EditorState, Plan};
use crate::display::visual_col;
use crate::history::EditKind;

impl EditorState {
    /// Tab: snippet tab stop, snippet expansion, indenting a multi-line
    /// selection, or inserting one indent.
    pub fn tab(&mut self, snippets: &[crate::snippets::Snippet]) {
        if self.next_tab_stop() || self.expand_snippet(snippets) {
            return;
        }
        let buffer = self.buffer();
        let multiline = self
            .selections()
            .iter()
            .any(|s| buffer.row_of(s.start()) != buffer.row_of(s.end()));
        if multiline {
            self.indent_lines();
            return;
        }
        let tab = self.settings.tab_size.max(1);
        let soft = self.settings.soft_tabs;
        self.apply_plans(EditKind::Insert, |state, sel| {
            let buffer = state.buffer();
            let row = buffer.row_of(sel.start());
            let line = buffer.line(row);
            let col = sel.start() - buffer.line_start(row);
            let unit = if soft {
                " ".repeat(tab - visual_col(&line, col, tab) % tab)
            } else {
                "\t".to_string()
            };
            let len = unit.len();
            Plan::replace(sel.range(), unit, len)
        });
    }

    /// Shift+Tab: previous snippet tab stop, or outdent.
    pub fn shift_tab(&mut self) {
        if !self.previous_tab_stop() {
            self.outdent_lines();
        }
    }

    pub fn indent_lines(&mut self) {
        let unit = self.settings.indent_unit();
        let buffer = self.buffer();
        let changes = self
            .selected_row_blocks()
            .into_iter()
            .flat_map(|rows| {
                let single = rows.len() == 1;
                rows.filter(move |row| single || !buffer.is_blank_line(*row))
                    .map(|row| Change::insert(buffer.line_start(row), unit.clone()))
            })
            .collect();
        self.apply_transforming(changes, EditKind::Other);
    }

    pub fn outdent_lines(&mut self) {
        let tab = self.settings.tab_size.max(1);
        let buffer = self.buffer();
        let changes = self
            .selected_row_blocks()
            .into_iter()
            .flatten()
            .filter_map(|row| {
                let line = buffer.line(row);
                let remove = if line.starts_with('\t') {
                    1
                } else {
                    line.bytes().take(tab).take_while(|b| *b == b' ').count()
                };
                let start = buffer.line_start(row);
                (remove > 0).then(|| Change::new(start..start + remove, ""))
            })
            .collect();
        self.apply_transforming(changes, EditKind::Other);
    }
}
