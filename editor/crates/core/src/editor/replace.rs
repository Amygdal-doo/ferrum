//! Replacing arbitrary ranges: find/replace, IME composition and reloading
//! a file from disk while keeping the cursors.

use std::ops::Range;

use super::{Change, EditorState};
use crate::history::EditKind;
use crate::selection::Selection;

impl EditorState {
    /// Replaces each range (non-overlapping) as one undo step; cursors
    /// follow the text around them.
    pub fn replace_ranges(&mut self, edits: Vec<(Range<usize>, String)>) {
        let changes = edits
            .into_iter()
            .map(|(range, text)| Change::new(range, text))
            .collect();
        self.apply_transforming(changes, EditKind::Other);
    }

    /// Replaces `range` and puts a single selection at `select` (relative to
    /// the inserted text; the caret after it when `None`). Used by IME input.
    pub fn replace_and_select(
        &mut self,
        range: Range<usize>,
        text: &str,
        select: Option<Range<usize>>,
        kind: EditKind,
    ) {
        let start = self.buffer().clip(range.start.min(self.buffer().len()));
        self.apply_transforming(vec![Change::new(start..range.end.max(start), text)], kind);
        let text_len = crate::buffer::normalize_newlines(text).len();
        let selection = match select {
            Some(r) => Selection::new(start + r.start.min(text_len), start + r.end.min(text_len)),
            None => Selection::caret(start + text_len),
        };
        self.replace_selections_after_edit(vec![selection]);
    }

    /// Replaces the whole text with `text` as one undoable edit touching only
    /// the differing middle, so cursors before/after it keep their place.
    pub fn set_text_preserving(&mut self, text: &str) {
        let text = crate::buffer::normalize_newlines(text);
        let old = self.text();
        if old == *text {
            return;
        }
        let prefix = old
            .char_indices()
            .zip(text.chars())
            .find(|((_, a), b)| a != b)
            .map_or(old.len().min(text.len()), |((i, _), _)| i);
        let max_suffix = (old.len() - prefix).min(text.len() - prefix);
        let suffix = old[old.len() - max_suffix..]
            .chars()
            .rev()
            .zip(text[text.len() - max_suffix..].chars().rev())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a.len_utf8())
            .sum::<usize>();
        let range = prefix..old.len() - suffix;
        let new = text[prefix..text.len() - suffix].to_string();
        self.apply_transforming(vec![Change::new(range, new)], EditKind::Other);
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::tests::{plain, render};
    use crate::history::EditKind;

    #[test]
    fn replace_ranges_keeps_cursor() {
        let mut s = plain("a b a|");
        s.replace_ranges(vec![(0..1, "xyz".into()), (4..5, "q".into())]);
        assert_eq!(render(&s), "xyz b q|");
        s.undo();
        assert_eq!(s.text(), "a b a");
    }

    #[test]
    fn ime_style_replace_and_select() {
        let mut s = plain("ab|");
        s.replace_and_select(2..2, "かな", Some(0..6), EditKind::Insert);
        assert_eq!(render(&s), "ab‹かな›");
        s.replace_and_select(2..8, "仮名", None, EditKind::Insert);
        assert_eq!(render(&s), "ab仮名|");
    }

    #[test]
    fn reload_preserves_cursors_outside_change() {
        let mut s = plain("one\ntwo|\nthree");
        s.set_text_preserving("zero\none\ntwo\nthree");
        assert_eq!(render(&s), "zero\none\ntwo|\nthree");
        s.set_text_preserving("zero\none\ntwo\nthree");
        assert!(s.can_undo());
        s.undo();
        assert_eq!(s.text(), "one\ntwo\nthree");
    }

    #[test]
    fn reload_handles_multibyte_suffix() {
        let mut s = plain("é|é");
        s.set_text_preserving("éxé");
        assert_eq!(s.text(), "éxé");
    }
}
