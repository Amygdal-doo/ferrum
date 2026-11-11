//! Line-wise commands: delete / duplicate / move / join lines, toggle
//! comment, and case conversion.

use std::ops::Range;

use super::{Change, EditorState, Mark, Plan};
use crate::history::EditKind;
use crate::selection::Selection;

impl EditorState {
    /// Row ranges covered by the selections, merged when they touch. A
    /// multi-row selection ending at column 0 does not include that row.
    pub(crate) fn selected_row_blocks(&self) -> Vec<Range<usize>> {
        let buffer = self.buffer();
        let mut blocks: Vec<Range<usize>> = Vec::new();
        for s in self.selections() {
            let first = buffer.row_of(s.start());
            let mut last = buffer.row_of(s.end());
            if last > first && buffer.line_start(last) == s.end() {
                last -= 1;
            }
            match blocks.last_mut() {
                Some(block) if first <= block.end => block.end = block.end.max(last + 1),
                _ => blocks.push(first..last + 1),
            }
        }
        blocks
    }

    /// Byte range of whole rows including their trailing newline (or the
    /// preceding one for the last row).
    fn rows_range_with_newline(&self, rows: &Range<usize>) -> Range<usize> {
        let buffer = self.buffer();
        let start = buffer.line_start(rows.start);
        let end_row = rows.end - 1;
        if end_row + 1 < buffer.line_count() {
            start..buffer.line_start(end_row + 1)
        } else {
            start.saturating_sub(usize::from(start > 0))..buffer.len()
        }
    }

    /// Ctrl+Shift+K: delete the selected lines.
    pub fn delete_lines(&mut self) {
        let changes = self
            .selected_row_blocks()
            .iter()
            .map(|rows| Change::new(self.rows_range_with_newline(rows), ""))
            .collect();
        self.apply_transforming(changes, EditKind::Other);
    }

    /// Duplicates the selected lines below; the cursors move to the copy.
    pub fn duplicate_lines(&mut self) {
        let buffer = self.buffer();
        let changes = self
            .selected_row_blocks()
            .into_iter()
            .map(|rows| {
                let start = buffer.line_start(rows.start);
                let end = buffer.line_end(rows.end - 1);
                Change::insert(start, format!("{}\n", buffer.slice(start..end)))
            })
            .collect();
        // The copy is inserted above, so the cursors end up on the lower
        // of the two identical blocks.
        self.apply_transforming(changes, EditKind::Other);
    }

    pub fn move_lines_up(&mut self) {
        self.move_lines(true);
    }

    pub fn move_lines_down(&mut self) {
        self.move_lines(false);
    }

    fn move_lines(&mut self, up: bool) {
        let blocks = self.selected_row_blocks();
        let buffer = self.buffer();
        let rows = buffer.line_count();
        if blocks.is_empty()
            || (up && blocks[0].start == 0)
            || (!up && blocks.last().is_some_and(|b| b.end >= rows))
        {
            return;
        }
        let mut changes = Vec::new();
        let mut shifts: Vec<(Range<usize>, isize)> = Vec::new();
        for block in &blocks {
            let start = buffer.line_start(block.start);
            let end = buffer.line_end(block.end - 1);
            let text = buffer.slice(start..end);
            if up {
                let above = buffer.line(block.start - 1).into_owned();
                let from = buffer.line_start(block.start - 1);
                changes.push(Change::new(from..end, format!("{text}\n{above}")));
                shifts.push((start..end, -((above.len() + 1) as isize)));
            } else {
                let below = buffer.line(block.end).into_owned();
                let to = buffer.line_end(block.end);
                changes.push(Change::new(start..to, format!("{below}\n{text}")));
                shifts.push((start..end, (below.len() + 1) as isize));
            }
        }
        let moved: Vec<Selection> = self
            .selections()
            .iter()
            .map(|s| {
                let shift = |offset: usize| {
                    shifts
                        .iter()
                        .find(|(range, _)| range.start <= offset && offset <= range.end)
                        .map_or(offset, |(_, d)| (offset as isize + d) as usize)
                };
                Selection::new(shift(s.anchor), shift(s.head))
            })
            .collect();
        // The block swaps keep total length per change, so offsets outside
        // the changes are unaffected.
        self.apply_transforming(changes, EditKind::Other);
        self.replace_selections_after_edit(moved);
    }

    /// Cmd+J: join the next line (or the selected lines) with single spaces.
    pub fn join_lines(&mut self) {
        let buffer = self.buffer();
        let mut changes = Vec::new();
        for rows in self.selected_row_blocks() {
            let last = if rows.len() == 1 {
                rows.start + 1
            } else {
                rows.end - 1
            };
            for row in rows.start..last.min(buffer.line_count() - 1) {
                let end = buffer.line_end(row);
                let next = buffer.line(row + 1);
                let skip = next.len() - next.trim_start().len();
                let trailing = buffer.line(row).len() - buffer.line(row).trim_end().len();
                let joiner = if next.trim().is_empty() { "" } else { " " };
                changes.push(Change::new(end - trailing..end + 1 + skip, joiner));
            }
        }
        self.apply_transforming(changes, EditKind::Other);
    }

    /// Cmd+/: comment or uncomment the selected lines with the language's
    /// line comment, or wrap them in its block comment.
    pub fn toggle_comment(&mut self) {
        let buffer = self.buffer();
        let mut changes = Vec::new();
        for rows in self.selected_row_blocks() {
            if let Some(token) = self.language.line_comment() {
                changes.extend(super::comment::line_comment_changes(buffer, rows, token));
            } else if let Some((open, close)) = self.language.block_comment() {
                changes.extend(super::comment::block_comment_changes(
                    buffer, rows, open, close,
                ));
            }
        }
        self.apply_transforming(changes, EditKind::Other);
    }

    pub fn upper_case(&mut self) {
        self.map_selected_text(|s| s.to_uppercase());
    }

    pub fn lower_case(&mut self) {
        self.map_selected_text(|s| s.to_lowercase());
    }

    fn map_selected_text(&mut self, f: impl Fn(&str) -> String) {
        self.apply_plans(EditKind::Other, |state, sel| {
            let range = if sel.is_empty() {
                crate::text::word_range_at(state.buffer(), sel.head)
            } else {
                sel.range()
            };
            let text = f(&state.buffer().slice(range.clone()));
            let len = text.len();
            let mut plan = Plan::replace(range, text, len);
            plan.anchor = Mark {
                change: 0,
                delta: if sel.is_empty() { len } else { 0 },
            };
            if sel.reversed() {
                std::mem::swap(&mut plan.anchor, &mut plan.head);
            }
            plan
        });
    }
}
