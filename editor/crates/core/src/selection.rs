//! Selections (anchor + head byte offsets) and the normalised multi-cursor
//! set: sorted by position, overlapping selections merged, never empty.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    /// Where the selection started (stays put while extending).
    pub anchor: usize,
    /// The cursor end.
    pub head: usize,
    /// Visual column to aim for on vertical moves, kept across short lines.
    pub goal: Option<usize>,
}

impl Selection {
    pub fn caret(offset: usize) -> Self {
        Self {
            anchor: offset,
            head: offset,
            goal: None,
        }
    }

    pub fn new(anchor: usize, head: usize) -> Self {
        Self {
            anchor,
            head,
            goal: None,
        }
    }

    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }

    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn range(&self) -> Range<usize> {
        self.start()..self.end()
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn reversed(&self) -> bool {
        self.head < self.anchor
    }

    /// Moves the head (and the anchor unless `extend`).
    pub fn move_head(&mut self, offset: usize, extend: bool) {
        self.head = offset;
        if !extend {
            self.anchor = offset;
        }
    }
}

/// Sorts selections and merges overlapping (or, for carets, coincident)
/// ones. Returns the index of `primary` after normalisation.
pub fn normalize(selections: &mut Vec<Selection>, primary: usize) -> usize {
    if selections.is_empty() {
        selections.push(Selection::caret(0));
        return 0;
    }
    let primary_head = selections.get(primary).map(|s| s.head);
    selections.sort_by_key(|s| (s.start(), s.end()));
    let mut merged: Vec<Selection> = Vec::with_capacity(selections.len());
    for selection in selections.drain(..) {
        if let Some(last) = merged.last_mut() {
            let overlaps = selection.start() < last.end()
                || (selection.start() == last.end() && (selection.is_empty() || last.is_empty()));
            if overlaps {
                let start = last.start().min(selection.start());
                let end = last.end().max(selection.end());
                let reversed = last.reversed();
                *last = if reversed {
                    Selection::new(end, start)
                } else {
                    Selection::new(start, end)
                };
                continue;
            }
        }
        merged.push(selection);
    }
    *selections = merged;
    primary_head
        .and_then(|head| {
            selections
                .iter()
                .position(|s| s.start() <= head && head <= s.end())
        })
        .unwrap_or(selections.len() - 1)
}

/// Shifts `offset` for an edit that replaced `range` with `new_len` bytes.
/// Offsets inside the replaced range move to its end when `bias_right`, else
/// to its start.
pub fn transform_offset(
    offset: usize,
    range: &Range<usize>,
    new_len: usize,
    bias_right: bool,
) -> usize {
    if offset < range.start {
        offset
    } else if offset > range.end {
        offset - (range.end - range.start) + new_len
    } else if range.is_empty() {
        if bias_right { offset + new_len } else { offset }
    } else if offset == range.end {
        range.start + new_len
    } else if offset == range.start || !bias_right {
        range.start
    } else {
        range.start + new_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_basics() {
        let s = Selection::new(5, 2);
        assert_eq!(s.range(), 2..5);
        assert!(s.reversed());
        assert!(!s.is_empty());
        let mut c = Selection::caret(3);
        c.move_head(7, true);
        assert_eq!(c.range(), 3..7);
        c.move_head(1, false);
        assert_eq!(c, Selection::caret(1));
    }

    #[test]
    fn normalize_sorts_and_merges_overlaps() {
        let mut selections = vec![
            Selection::new(10, 15),
            Selection::caret(2),
            Selection::new(12, 20),
            Selection::caret(2),
        ];
        let primary = normalize(&mut selections, 2);
        assert_eq!(
            selections,
            vec![Selection::caret(2), Selection::new(10, 20)]
        );
        assert_eq!(primary, 1);
    }

    #[test]
    fn adjacent_non_empty_selections_stay_separate() {
        let mut selections = vec![Selection::new(0, 3), Selection::new(3, 6)];
        normalize(&mut selections, 0);
        assert_eq!(selections.len(), 2);
    }

    #[test]
    fn normalize_never_leaves_empty_set() {
        let mut selections = Vec::new();
        assert_eq!(normalize(&mut selections, 0), 0);
        assert_eq!(selections, vec![Selection::caret(0)]);
    }

    #[test]
    fn transform_offsets_across_edit() {
        // "hello world" -> replace 6..11 ("world") with "you" (3 bytes)
        let range = 6..11;
        assert_eq!(transform_offset(2, &range, 3, true), 2);
        assert_eq!(transform_offset(11, &range, 3, true), 9);
        assert_eq!(transform_offset(15, &range, 3, true), 13);
        assert_eq!(transform_offset(8, &range, 3, true), 9);
        assert_eq!(transform_offset(8, &range, 3, false), 6);
        // insertion at the offset itself
        assert_eq!(transform_offset(4, &(4..4), 2, true), 6);
        assert_eq!(transform_offset(4, &(4..4), 2, false), 4);
    }
}
