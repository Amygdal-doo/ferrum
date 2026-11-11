//! Multi-cursor commands: select next / all occurrences, split selection
//! into lines, add cursors above / below, Cmd+click and column selection.

use super::EditorState;
use super::motion::vertical_offset;
use crate::display::{DisplayMap, DisplayPoint};
use crate::find::SearchQuery;
use crate::selection::Selection;
use crate::text::{CharClass, char_class, word_range_at};

impl EditorState {
    /// Cmd+D: select the word under each caret, then add the next
    /// occurrence of the newest selection's text.
    pub fn select_next_occurrence(&mut self) {
        let buffer = self.buffer();
        if self.selections().iter().any(Selection::is_empty) {
            let single = self.selections().len() == 1;
            let expanded: Vec<Selection> = self
                .selections()
                .iter()
                .map(|s| {
                    if !s.is_empty() {
                        return *s;
                    }
                    let range = word_range_at(buffer, s.head);
                    let is_word = buffer
                        .char_at(range.start)
                        .is_some_and(|c| char_class(c) == CharClass::Word);
                    if is_word {
                        Selection::new(range.start, range.end)
                    } else {
                        *s
                    }
                })
                .collect();
            let primary = self.primary_index();
            self.set_selections_keep_primary(expanded, primary);
            self.select_next_whole_word = single;
            return;
        }
        let whole_word = self.select_next_whole_word;
        let newest = self.primary();
        let needle = buffer.slice(newest.range());
        let Ok(query) = SearchQuery::plain(needle)
            .case_sensitive(true)
            .whole_word(whole_word)
            .compile()
        else {
            return;
        };
        let text = buffer.text();
        let matches = query.find_all(&text);
        let taken = |m: &std::ops::Range<usize>| {
            self.selections()
                .iter()
                .any(|s| s.start() < m.end && m.start < s.end())
        };
        let next = matches
            .iter()
            .filter(|m| m.start >= newest.end())
            .chain(matches.iter())
            .find(|m| !taken(m))
            .cloned();
        if let Some(m) = next {
            let mut selections = self.selections().to_vec();
            selections.push(Selection::new(m.start, m.end));
            self.set_selections(selections);
            self.select_next_whole_word = whole_word;
        }
    }

    /// Selects every occurrence of the primary selection's text (or the
    /// word under the caret).
    pub fn select_all_occurrences(&mut self) {
        let buffer = self.buffer();
        let primary = self.primary();
        let (range, whole_word) = if primary.is_empty() {
            (word_range_at(buffer, primary.head), true)
        } else {
            (primary.range(), false)
        };
        if range.is_empty() {
            return;
        }
        let Ok(query) = SearchQuery::plain(buffer.slice(range))
            .case_sensitive(true)
            .whole_word(whole_word)
            .compile()
        else {
            return;
        };
        let selections: Vec<Selection> = query
            .find_all(&buffer.text())
            .into_iter()
            .map(|m| Selection::new(m.start, m.end))
            .collect();
        if !selections.is_empty() {
            self.set_selections(selections);
        }
    }

    /// Cmd+Shift+L: one selection per line of each multi-line selection.
    pub fn split_selection_into_lines(&mut self) {
        let buffer = self.buffer();
        let mut out = Vec::new();
        for s in self.selections() {
            let first = buffer.row_of(s.start());
            let last = buffer.row_of(s.end());
            if first == last {
                out.push(*s);
                continue;
            }
            for row in first..=last {
                let start = if row == first {
                    s.start()
                } else {
                    buffer.line_start(row)
                };
                let end = if row == last {
                    s.end()
                } else {
                    buffer.line_end(row)
                };
                if row == last && end == buffer.line_start(row) {
                    continue;
                }
                out.push(Selection::new(start, end));
            }
        }
        self.set_selections(out);
    }

    pub fn add_cursor_above(&mut self, map: &DisplayMap) {
        self.add_cursor_vertical(map, -1);
    }

    pub fn add_cursor_below(&mut self, map: &DisplayMap) {
        self.add_cursor_vertical(map, 1);
    }

    fn add_cursor_vertical(&mut self, map: &DisplayMap, rows: isize) {
        let mut selections = self.selections().to_vec();
        let edge = if rows < 0 {
            selections.iter().map(|s| s.head).min()
        } else {
            selections.iter().map(|s| s.head).max()
        };
        let Some(edge) = edge else { return };
        if let Some(offset) = vertical_offset(self, map, edge, rows) {
            selections.push(Selection::caret(offset));
            self.set_selections(selections);
        }
    }

    /// Cmd+click: add a caret, or remove the one already at `offset`.
    pub fn toggle_cursor_at(&mut self, offset: usize) {
        let mut selections = self.selections().to_vec();
        if selections.len() > 1
            && let Some(index) = selections
                .iter()
                .position(|s| s.is_empty() && s.head == offset)
        {
            selections.remove(index);
        } else {
            selections.push(Selection::caret(offset));
        }
        self.set_selections(selections);
    }

    /// Alt+drag: a rectangle of selections between two display points.
    pub fn column_select(&mut self, from: DisplayPoint, to: DisplayPoint, map: &DisplayMap) {
        let buffer = self.buffer();
        let (top, bottom) = (from.row.min(to.row), from.row.max(to.row));
        let selections: Vec<Selection> = (top..=bottom.min(map.row_count() - 1))
            .map(|row| {
                let anchor = map.to_offset(buffer, DisplayPoint { row, col: from.col });
                let head = map.to_offset(buffer, DisplayPoint { row, col: to.col });
                Selection::new(anchor, head)
            })
            .collect();
        self.set_selections(selections);
    }
}
