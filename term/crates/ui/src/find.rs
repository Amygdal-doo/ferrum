//! Find in the scrollback (⌘F): the matches the server found in one pane
//! (see `Request::Find`), which one is current, where they are on screen,
//! and how far to scroll to show the current one. The bar itself is in
//! `view::find`.
//!
//! Lines count from the oldest scrollback line, so they stay put while
//! output scrolls the screen (until the scrollback is full and its oldest
//! lines go; the bar searches again as output arrives).

use ferrumterm_protocol::{BlockId, FindMatch, Frame};

/// Which way ↩ / ⇧↩ move through the matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// Up, towards older output (↩, ⌘G).
    Older,
    /// Down, towards newer output (⇧↩, ⌘⇧G).
    Newer,
}

/// One pane's matches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Search {
    pub block: Option<BlockId>,
    /// Oldest first.
    pub matches: Vec<FindMatch>,
    /// The server found more than it returned.
    pub truncated: bool,
    pub current: Option<usize>,
}

/// The screen line of `frame`'s top row (see the module docs).
fn top_line(frame: &Frame) -> i64 {
    i64::from(frame.history) - i64::from(frame.display_offset)
}

impl Search {
    /// Takes new results. The current match stays when it is still there,
    /// else the one nearest it; the first time, the newest is current.
    pub(crate) fn update(&mut self, matches: Vec<FindMatch>, truncated: bool) {
        let before = self.current.and_then(|i| self.matches.get(i)).copied();
        self.current = match before {
            _ if matches.is_empty() => None,
            None => Some(matches.len() - 1),
            Some(old) => Some(
                matches
                    .iter()
                    .position(|m| (m.line, m.col) == (old.line, old.col))
                    .unwrap_or_else(|| {
                        let after = matches.partition_point(|m| m.line < old.line);
                        after.min(matches.len() - 1)
                    }),
            ),
        };
        self.matches = matches;
        self.truncated = truncated;
    }

    pub(crate) fn step(&mut self, step: Step) {
        let n = self.matches.len();
        if n == 0 {
            return;
        }
        self.current = Some(match (self.current, step) {
            (None, _) => n - 1,
            (Some(0), Step::Older) => n - 1,
            (Some(i), Step::Older) => i - 1,
            (Some(i), Step::Newer) => (i + 1) % n,
        });
    }

    /// "3 of 17", "No matches"; "5000+" when the server stopped counting.
    pub(crate) fn label(&self) -> String {
        let n = self.matches.len();
        match self.current {
            _ if n == 0 => "No matches".to_owned(),
            Some(i) if self.truncated => format!("{} of {n}+", i + 1),
            Some(i) => format!("{} of {n}", i + 1),
            None => format!("{n}"),
        }
    }

    /// The highlighted cells of screen `row` of `frame` (a pane `cols`
    /// wide): `(from, to, current)`, `to` exclusive.
    pub(crate) fn row_highlights(
        &self,
        frame: &Frame,
        row: u16,
        cols: u16,
    ) -> Vec<(u16, u16, bool)> {
        let line = top_line(frame) + i64::from(row);
        let Ok(line) = u32::try_from(line) else {
            return Vec::new();
        };
        let first = self.matches.partition_point(|m| m.end_line < line);
        self.matches[first..]
            .iter()
            .enumerate()
            .take_while(|(_, m)| m.line <= line)
            .filter(|(_, m)| m.end_line >= line)
            .map(|(i, m)| {
                let from = if m.line == line { m.col } else { 0 };
                let to = if m.end_line == line { m.end_col } else { cols };
                (
                    from.min(cols),
                    to.min(cols),
                    self.current == Some(first + i),
                )
            })
            .filter(|(from, to, _)| from < to)
            .collect()
    }

    /// Lines to scroll (`Request::Scroll`) so the current match shows, a
    /// third of the way down; `None` when it is on screen already.
    pub(crate) fn scroll_to_current(&self, frame: &Frame) -> Option<i32> {
        let m = self.matches.get(self.current?)?;
        let top = top_line(frame);
        let rows = i64::from(frame.rows);
        let line = i64::from(m.line);
        if (top..top + rows).contains(&line) {
            return None;
        }
        let history = i64::from(frame.history);
        let want_top = (line - rows / 3).max(0);
        let offset = (history - want_top).clamp(0, history);
        i32::try_from(offset - i64::from(frame.display_offset)).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(line: u32, col: u16, end_line: u32, end_col: u16) -> FindMatch {
        FindMatch {
            line,
            col,
            end_line,
            end_col,
        }
    }

    /// A 4-row screen over `history` lines of scrollback, scrolled back `offset`.
    fn frame(history: u32, offset: u32) -> Frame {
        Frame {
            cols: 10,
            rows: 4,
            lines: vec![Vec::new(); 4],
            history,
            display_offset: offset,
            ..Frame::default()
        }
    }

    #[test]
    fn starts_at_the_newest_and_steps_both_ways_around() {
        let mut s = Search::default();
        s.update(vec![at(1, 0, 1, 3), at(5, 2, 5, 4), at(9, 0, 9, 1)], false);
        assert_eq!((s.current, s.label()), (Some(2), "3 of 3".to_owned()));
        s.step(Step::Older);
        s.step(Step::Older);
        assert_eq!(s.current, Some(0));
        s.step(Step::Older);
        assert_eq!(s.current, Some(2), "wraps to the newest");
        s.step(Step::Newer);
        assert_eq!(s.current, Some(0), "wraps to the oldest");
        s.update(Vec::new(), false);
        assert_eq!((s.current, s.label()), (None, "No matches".to_owned()));
        s.update(vec![at(0, 0, 0, 1)], true);
        assert_eq!(s.label(), "1 of 1+");
    }

    #[test]
    fn searching_again_keeps_the_current_match() {
        let mut s = Search::default();
        s.update(vec![at(1, 0, 1, 3), at(5, 2, 5, 4), at(9, 0, 9, 1)], false);
        s.step(Step::Older);
        s.update(
            vec![
                at(1, 0, 1, 3),
                at(5, 2, 5, 4),
                at(9, 0, 9, 1),
                at(12, 0, 12, 1),
            ],
            false,
        );
        assert_eq!(s.current, Some(1), "the same match");
        s.update(vec![at(1, 0, 1, 3), at(7, 0, 7, 1)], false);
        assert_eq!(s.current, Some(1), "gone: the next one after it");
    }

    #[test]
    fn highlights_on_screen_rows_including_wrapped_matches() {
        let mut s = Search::default();
        // Line 10 (row 0 of a screen at the bottom of 10 lines of history)
        // and one match from the end of line 11 into line 12.
        s.update(vec![at(10, 2, 10, 5), at(11, 8, 12, 2)], false);
        let f = frame(10, 0);
        assert_eq!(s.row_highlights(&f, 0, 10), [(2, 5, false)]);
        assert_eq!(s.row_highlights(&f, 1, 10), [(8, 10, true)]);
        assert_eq!(s.row_highlights(&f, 2, 10), [(0, 2, true)]);
        assert!(s.row_highlights(&f, 3, 10).is_empty());
        // Scrolled back two lines: line 10 is row 2.
        assert_eq!(s.row_highlights(&frame(10, 2), 2, 10), [(2, 5, false)]);
    }

    #[test]
    fn scrolls_only_when_the_current_match_is_off_screen() {
        let mut s = Search::default();
        s.update(vec![at(2, 0, 2, 1), at(98, 0, 98, 1)], false);
        // 100 lines of history, at the bottom: lines 100..104 show.
        let bottom = frame(100, 0);
        assert_eq!(s.scroll_to_current(&bottom), Some(100 - 97));
        s.step(Step::Older);
        // Line 2 a third of the way down a 4-row screen: top line 1.
        assert_eq!(s.scroll_to_current(&bottom), Some(99));
        assert_eq!(s.scroll_to_current(&frame(100, 99)), None);
        // From further up the scrollback, back down.
        s.step(Step::Newer);
        assert_eq!(s.scroll_to_current(&frame(100, 99)), Some(3 - 99));
    }
}
