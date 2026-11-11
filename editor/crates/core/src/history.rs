//! Undo / redo. Each [`Transaction`] holds the edit operations of one
//! command (all cursors) plus the selections before and after it.
//! Consecutive typing or deleting within [`GROUP_INTERVAL_MS`] merges into
//! one undo step unless the history was sealed (e.g. by moving the cursor).

use std::ops::Range;

use crate::buffer::Buffer;
use crate::selection::Selection;

pub const GROUP_INTERVAL_MS: u64 = 1000;

/// One replacement, with `range` in the text as it was just before this op.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditOp {
    pub range: Range<usize>,
    pub old_text: String,
    pub new_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    /// Typing characters — merges with adjacent typing.
    Insert,
    /// Backspace / delete — merges with adjacent deletes.
    Delete,
    /// Everything else; never merges.
    Other,
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub id: u64,
    pub ops: Vec<EditOp>,
    pub selections_before: Vec<Selection>,
    pub selections_after: Vec<Selection>,
    pub kind: EditKind,
    pub timestamp_ms: u64,
}

#[derive(Debug, Default, Clone)]
pub struct History {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    next_id: u64,
    sealed: bool,
    saved_id: u64,
}

/// Most undo steps kept per buffer.
const MAX_UNDO: usize = 10_000;

impl History {
    /// Id of the state the buffer is in (0 = as loaded).
    pub fn current_id(&self) -> u64 {
        self.undo.last().map_or(0, |t| t.id)
    }

    pub fn mark_saved(&mut self) {
        self.saved_id = self.current_id();
        self.sealed = true;
    }

    pub fn is_dirty(&self) -> bool {
        self.current_id() != self.saved_id
    }

    /// Ends the current group: the next edit starts a new undo step.
    pub fn seal(&mut self) {
        self.sealed = true;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Records a transaction, merging it into the previous one when both
    /// are the same typing/deleting run.
    pub fn push(
        &mut self,
        ops: Vec<EditOp>,
        selections_before: Vec<Selection>,
        selections_after: Vec<Selection>,
        kind: EditKind,
        timestamp_ms: u64,
    ) -> bool {
        if ops.is_empty() {
            return false;
        }
        self.redo.clear();
        self.next_id += 1;
        let id = self.next_id;
        if let Some(last) = self.undo.last_mut()
            && !self.sealed
            && kind != EditKind::Other
            && last.kind == kind
            && timestamp_ms.saturating_sub(last.timestamp_ms) <= GROUP_INTERVAL_MS
            && last.selections_after == selections_before
        {
            last.ops.extend(ops);
            last.selections_after = selections_after;
            last.timestamp_ms = timestamp_ms;
            last.id = id;
            return true;
        }
        self.sealed = false;
        self.undo.push(Transaction {
            id,
            ops,
            selections_before,
            selections_after,
            kind,
            timestamp_ms,
        });
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        true
    }

    /// Fixes up the selections recorded after the latest transaction.
    pub fn set_last_selections_after(&mut self, selections: Vec<Selection>) {
        if let Some(last) = self.undo.last_mut() {
            last.selections_after = selections;
        }
    }

    /// Reverts the last transaction on `buffer`; returns the selections to
    /// restore.
    pub fn undo(&mut self, buffer: &mut Buffer) -> Option<Vec<Selection>> {
        let transaction = self.undo.pop()?;
        for op in transaction.ops.iter().rev() {
            let end = op.range.start + op.new_text.len();
            buffer.edit(op.range.start..end, &op.old_text);
        }
        let selections = transaction.selections_before.clone();
        self.redo.push(transaction);
        self.sealed = true;
        Some(selections)
    }

    pub fn redo(&mut self, buffer: &mut Buffer) -> Option<Vec<Selection>> {
        let transaction = self.redo.pop()?;
        for op in &transaction.ops {
            buffer.edit(op.range.clone(), &op.new_text);
        }
        let selections = transaction.selections_after.clone();
        self.undo.push(transaction);
        self.sealed = true;
        Some(selections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(buffer: &mut Buffer, range: Range<usize>, text: &str) -> EditOp {
        let old_text = buffer.slice(range.clone());
        buffer.edit(range.clone(), text);
        EditOp {
            range,
            old_text,
            new_text: text.to_string(),
        }
    }

    fn sel(offset: usize) -> Vec<Selection> {
        vec![Selection::caret(offset)]
    }

    #[test]
    fn undo_and_redo_restore_text_and_selections() {
        let mut buffer = Buffer::new("abc");
        let mut history = History::default();
        let edit = op(&mut buffer, 1..2, "XYZ");
        history.push(vec![edit], sel(2), sel(4), EditKind::Other, 0);
        assert_eq!(buffer.text(), "aXYZc");
        assert_eq!(history.undo(&mut buffer), Some(sel(2)));
        assert_eq!(buffer.text(), "abc");
        assert_eq!(history.redo(&mut buffer), Some(sel(4)));
        assert_eq!(buffer.text(), "aXYZc");
        assert!(history.redo(&mut buffer).is_none());
    }

    #[test]
    fn multi_op_transactions_undo_in_reverse() {
        let mut buffer = Buffer::new("a b");
        let mut history = History::default();
        let first = op(&mut buffer, 2..3, "BB");
        let second = op(&mut buffer, 0..1, "AA");
        history.push(vec![first, second], sel(0), sel(0), EditKind::Other, 0);
        assert_eq!(buffer.text(), "AA BB");
        history.undo(&mut buffer);
        assert_eq!(buffer.text(), "a b");
    }

    #[test]
    fn typing_within_interval_groups() {
        let mut buffer = Buffer::new("");
        let mut history = History::default();
        let a = op(&mut buffer, 0..0, "a");
        history.push(vec![a], sel(0), sel(1), EditKind::Insert, 0);
        let b = op(&mut buffer, 1..1, "b");
        history.push(vec![b], sel(1), sel(2), EditKind::Insert, 300);
        // too late: new group
        let c = op(&mut buffer, 2..2, "c");
        history.push(
            vec![c],
            sel(2),
            sel(3),
            EditKind::Insert,
            300 + GROUP_INTERVAL_MS + 1,
        );
        assert_eq!(buffer.text(), "abc");
        history.undo(&mut buffer);
        assert_eq!(buffer.text(), "ab");
        history.undo(&mut buffer);
        assert_eq!(buffer.text(), "");
    }

    #[test]
    fn seal_and_kind_change_break_groups() {
        let mut buffer = Buffer::new("");
        let mut history = History::default();
        let a = op(&mut buffer, 0..0, "a");
        history.push(vec![a], sel(0), sel(1), EditKind::Insert, 0);
        history.seal();
        let b = op(&mut buffer, 1..1, "b");
        history.push(vec![b], sel(1), sel(2), EditKind::Insert, 1);
        let del = op(&mut buffer, 1..2, "");
        history.push(vec![del], sel(2), sel(1), EditKind::Delete, 2);
        history.undo(&mut buffer);
        assert_eq!(buffer.text(), "ab");
        history.undo(&mut buffer);
        assert_eq!(buffer.text(), "a");
        history.undo(&mut buffer);
        assert_eq!(buffer.text(), "");
    }

    #[test]
    fn non_contiguous_selection_breaks_group() {
        let mut buffer = Buffer::new("xx");
        let mut history = History::default();
        let a = op(&mut buffer, 0..0, "a");
        history.push(vec![a], sel(0), sel(1), EditKind::Insert, 0);
        let b = op(&mut buffer, 3..3, "b");
        history.push(vec![b], sel(3), sel(4), EditKind::Insert, 1);
        history.undo(&mut buffer);
        assert_eq!(buffer.text(), "axx");
    }

    #[test]
    fn dirty_tracking_follows_saved_point() {
        let mut buffer = Buffer::new("");
        let mut history = History::default();
        assert!(!history.is_dirty());
        let a = op(&mut buffer, 0..0, "a");
        history.push(vec![a], sel(0), sel(1), EditKind::Insert, 0);
        assert!(history.is_dirty());
        history.mark_saved();
        assert!(!history.is_dirty());
        // typing after save must not merge into the saved transaction
        let b = op(&mut buffer, 1..1, "b");
        history.push(vec![b], sel(1), sel(2), EditKind::Insert, 1);
        assert!(history.is_dirty());
        history.undo(&mut buffer);
        assert!(!history.is_dirty());
        assert_eq!(buffer.text(), "a");
        history.undo(&mut buffer);
        assert!(history.is_dirty());
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut buffer = Buffer::new("");
        let mut history = History::default();
        let a = op(&mut buffer, 0..0, "a");
        history.push(vec![a], sel(0), sel(1), EditKind::Other, 0);
        history.undo(&mut buffer);
        assert!(history.can_redo());
        let b = op(&mut buffer, 0..0, "b");
        history.push(vec![b], sel(0), sel(1), EditKind::Other, 1);
        assert!(!history.can_redo());
    }
}
