//! The edit pipeline shared by all commands: apply a batch of changes as
//! one undo transaction and compute the selections afterwards.

use std::ops::Range;

use super::{Change, EditorState, Mark, Plan, now_ms};
use crate::buffer::normalize_newlines;
use crate::history::{EditKind, EditOp};
use crate::selection::{Selection, normalize, transform_offset};

impl EditorState {
    /// Runs `plan` at every selection and applies the result as one
    /// transaction. Overlapping changes from different selections are
    /// clipped so the batch stays valid.
    pub(crate) fn apply_plans(
        &mut self,
        kind: EditKind,
        mut plan: impl FnMut(&EditorState, &Selection) -> Plan,
    ) {
        let plans: Vec<Plan> = self.selections.iter().map(|s| plan(self, s)).collect();
        let mut changes: Vec<Change> = Vec::new();
        let mut marks = Vec::with_capacity(plans.len());
        for plan in plans {
            let base = changes.len();
            let last = plan.changes.len().saturating_sub(1);
            let shift = |mark: Mark| Mark {
                change: base + mark.change.min(last),
                delta: mark.delta,
            };
            marks.push((shift(plan.anchor), shift(plan.head)));
            if plan.changes.is_empty() {
                changes.push(Change::insert(0, ""));
            } else {
                changes.extend(plan.changes);
            }
        }
        let (new_ranges, recorded) = self.apply_changes(&mut changes, kind);
        let mut selections: Vec<Selection> = marks
            .into_iter()
            .map(|(anchor, head)| {
                let at = |mark: Mark| {
                    new_ranges
                        .get(mark.change)
                        .map_or(0, |range: &Range<usize>| range.start + mark.delta)
                };
                Selection::new(at(anchor), at(head))
            })
            .collect();
        self.finish_edit(&mut selections, recorded);
    }

    /// Applies `changes` (overlaps clipped) and records history. Selections
    /// are mapped through the edit: carets and selection ends move right
    /// past text inserted at them, selection starts stay put.
    pub(crate) fn apply_transforming(&mut self, mut changes: Vec<Change>, kind: EditKind) {
        changes.sort_by_key(|c| (c.range.start, c.range.end));
        let old = self.selections.clone();
        let (_, recorded) = self.apply_changes(&mut changes, kind);
        let mut selections: Vec<Selection> = old
            .into_iter()
            .map(|s| {
                let anchor_right = s.is_empty() || s.anchor > s.head;
                let head_right = s.is_empty() || s.head > s.anchor;
                let mut anchor = s.anchor;
                let mut head = s.head;
                // Changes were applied last-to-first; replay in that order.
                for change in changes.iter().rev() {
                    let len = change.text.len();
                    anchor = transform_offset(anchor, &change.range, len, anchor_right);
                    head = transform_offset(head, &change.range, len, head_right);
                }
                Selection::new(anchor, head)
            })
            .collect();
        self.finish_edit(&mut selections, recorded);
    }

    fn finish_edit(&mut self, selections: &mut Vec<Selection>, recorded: bool) {
        let primary = self.primary_index();
        let mut new = std::mem::take(selections);
        let len = self.buffer.len();
        for s in &mut new {
            s.anchor = self.buffer.clip(s.anchor.min(len));
            s.head = self.buffer.clip(s.head.min(len));
        }
        let primary = primary.min(new.len().saturating_sub(1));
        self.primary = normalize(&mut new, primary);
        self.selections = new;
        if recorded {
            self.history
                .set_last_selections_after(self.selections.clone());
        } else {
            self.history.seal();
        }
        self.last_edit_recorded = recorded;
        self.select_next_whole_word = false;
    }

    /// Overrides the selections an edit command just produced (keeping the
    /// undo entry's "after" selections in sync).
    pub(crate) fn replace_selections_after_edit(&mut self, selections: Vec<Selection>) {
        let len = self.buffer.len();
        let mut selections: Vec<Selection> = selections
            .into_iter()
            .map(|s| {
                Selection::new(
                    self.buffer.clip(s.anchor.min(len)),
                    self.buffer.clip(s.head.min(len)),
                )
            })
            .collect();
        self.primary = normalize(&mut selections, self.primary_index());
        self.selections = selections;
        if self.last_edit_recorded {
            self.history
                .set_last_selections_after(self.selections.clone());
        }
    }

    /// Sorts, clips and applies changes from the end backwards. Returns
    /// where each change's new text ended up (indexed like `changes`) and
    /// whether a history entry was recorded.
    fn apply_changes(
        &mut self,
        changes: &mut [Change],
        kind: EditKind,
    ) -> (Vec<Range<usize>>, bool) {
        let before = self.selections.clone();
        let mut order: Vec<usize> = (0..changes.len()).collect();
        order.sort_by_key(|&i| (changes[i].range.start, changes[i].range.end));
        let mut floor = 0;
        for &i in &order {
            let change = &mut changes[i];
            change.text = normalize_newlines(&change.text).into_owned();
            let start = self.buffer.clip(change.range.start.max(floor));
            let end = self.buffer.clip(change.range.end.max(start));
            change.range = start..end;
            floor = end;
        }
        let mut ops = Vec::new();
        for &i in order.iter().rev() {
            let change = &changes[i];
            let old_text = self.buffer.slice(change.range.clone());
            if old_text == change.text {
                continue;
            }
            self.buffer.edit(change.range.clone(), &change.text);
            ops.push(EditOp {
                range: change.range.clone(),
                old_text,
                new_text: change.text.clone(),
            });
        }
        let mut new_ranges = vec![0..0; changes.len()];
        let mut delta: isize = 0;
        for &i in &order {
            let change = &changes[i];
            let start = (change.range.start as isize + delta) as usize;
            new_ranges[i] = start..start + change.text.len();
            delta += change.text.len() as isize - change.range.len() as isize;
        }
        if let Some(session) = &mut self.snippet {
            for op in &ops {
                session.transform(&op.range, op.new_text.len());
            }
        }
        // `selections_after` is filled in by `finish_edit`.
        let recorded = self
            .history
            .push(ops, before.clone(), before, kind, now_ms());
        (new_ranges, recorded)
    }
}
