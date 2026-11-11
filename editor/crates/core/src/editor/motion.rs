//! Cursor motions (optionally extending the selection) and mouse-style
//! selection helpers. Vertical motions go through a [`DisplayMap`] so they
//! follow soft-wrapped rows and keep a goal column.

use super::EditorState;
use crate::buffer::Point;
use crate::display::{DisplayMap, DisplayPoint};
use crate::selection::Selection;
use crate::text::{
    next_grapheme, next_word_end, prev_grapheme, prev_word_start, smart_home, word_range_at,
};

impl EditorState {
    fn move_each(
        &mut self,
        extend: bool,
        f: impl Fn(&EditorState, &Selection) -> (usize, Option<usize>),
    ) {
        let moved: Vec<Selection> = self
            .selections()
            .iter()
            .map(|s| {
                let (head, goal) = f(self, s);
                let mut s = *s;
                s.move_head(head, extend);
                s.goal = goal;
                s
            })
            .collect();
        let primary = self.primary_index();
        self.set_selections_keep_primary(moved, primary);
    }

    pub(crate) fn set_selections_keep_primary(
        &mut self,
        selections: Vec<Selection>,
        primary: usize,
    ) {
        let mut selections = selections;
        if primary < selections.len() {
            let p = selections.remove(primary);
            selections.push(p);
        }
        self.set_selections(selections);
    }

    pub fn move_left(&mut self, extend: bool) {
        self.move_each(extend, |state, s| {
            if !extend && !s.is_empty() {
                (s.start(), None)
            } else {
                (prev_grapheme(state.buffer(), s.head), None)
            }
        });
    }

    pub fn move_right(&mut self, extend: bool) {
        self.move_each(extend, |state, s| {
            if !extend && !s.is_empty() {
                (s.end(), None)
            } else {
                (next_grapheme(state.buffer(), s.head), None)
            }
        });
    }

    /// Moves `rows` display rows up (`rows < 0`) or down.
    pub fn move_vertical(&mut self, rows: isize, extend: bool, map: &DisplayMap) {
        self.move_each(extend, |state, s| {
            let from = match (extend, s.is_empty()) {
                (false, false) if rows < 0 => s.start(),
                (false, false) => s.end(),
                _ => s.head,
            };
            vertical_target(state, map, from, s.goal, rows)
        });
    }

    pub fn move_word_left(&mut self, extend: bool) {
        self.move_each(extend, |state, s| {
            (prev_word_start(state.buffer(), s.head), None)
        });
    }

    pub fn move_word_right(&mut self, extend: bool) {
        self.move_each(extend, |state, s| {
            (next_word_end(state.buffer(), s.head), None)
        });
    }

    /// Smart home: first non-blank, then column 0.
    pub fn move_line_start(&mut self, extend: bool) {
        self.move_each(extend, |state, s| {
            (smart_home(state.buffer(), s.head), None)
        });
    }

    pub fn move_line_end(&mut self, extend: bool) {
        self.move_each(extend, |state, s| {
            (state.buffer().line_end(state.buffer().row_of(s.head)), None)
        });
    }

    pub fn move_doc_start(&mut self, extend: bool) {
        self.move_each(extend, |_, _| (0, None));
        if !extend {
            self.single_cursor_at_primary();
        }
    }

    pub fn move_doc_end(&mut self, extend: bool) {
        let end = self.buffer().len();
        self.move_each(extend, |_, _| (end, None));
        if !extend {
            self.single_cursor_at_primary();
        }
    }

    pub fn select_all(&mut self) {
        let len = self.buffer().len();
        self.set_selections(vec![Selection::new(0, len)]);
    }

    /// Cmd+L: select whole lines; repeating extends by one line.
    pub fn select_line(&mut self) {
        let buffer = self.buffer();
        let selections = self
            .selections()
            .iter()
            .map(|s| {
                let start_row = buffer.row_of(s.start());
                let end_row = buffer.row_of(s.end()) + 1;
                let end = if end_row < buffer.line_count() {
                    buffer.line_start(end_row)
                } else {
                    buffer.len()
                };
                Selection::new(buffer.line_start(start_row), end)
            })
            .collect();
        self.set_selections(selections);
    }

    /// Escape: drop extra cursors, else collapse the selection; ends any
    /// snippet session.
    pub fn escape(&mut self) -> bool {
        self.end_snippet();
        if self.selections().len() > 1 {
            self.single_cursor_at_primary();
            return true;
        }
        let s = self.primary();
        if !s.is_empty() {
            self.set_selections(vec![Selection::caret(s.head)]);
            return true;
        }
        false
    }

    fn single_cursor_at_primary(&mut self) {
        let p = self.primary();
        self.set_selections(vec![p]);
    }

    /// Mouse click: a caret at `offset` (replacing all selections).
    pub fn click(&mut self, offset: usize) {
        self.set_cursor(offset);
    }

    /// Shift+click / drag: extend the primary selection to `offset`.
    pub fn extend_primary_to(&mut self, offset: usize) {
        let mut selections = self.selections().to_vec();
        let primary = self.primary_index();
        selections[primary].head = offset;
        selections[primary].goal = None;
        self.set_selections_keep_primary(selections, primary);
    }

    pub fn select_word_at(&mut self, offset: usize) {
        let range = word_range_at(self.buffer(), offset);
        self.set_selections(vec![Selection::new(range.start, range.end)]);
    }

    pub fn select_row(&mut self, row: usize) {
        let buffer = self.buffer();
        let start = buffer.line_start(row);
        let end = if row + 1 < buffer.line_count() {
            buffer.line_start(row + 1)
        } else {
            buffer.len()
        };
        self.set_selections(vec![Selection::new(start, end)]);
    }

    /// Ctrl+G: caret at 0-based `line` / `column` (clamped).
    pub fn go_to_point(&mut self, line: usize, column: usize) {
        let offset = self.buffer().point_to_offset(Point::new(line, column));
        self.set_cursor(offset);
    }
}

fn vertical_target(
    state: &EditorState,
    map: &DisplayMap,
    from: usize,
    goal: Option<usize>,
    rows: isize,
) -> (usize, Option<usize>) {
    let buffer = state.buffer();
    let point = map.to_display(buffer, from);
    let goal = goal.unwrap_or(point.col);
    let target = point.row as isize + rows;
    if target < 0 {
        return (0, Some(goal));
    }
    let target = target as usize;
    if target >= map.row_count() {
        return (buffer.len(), Some(goal));
    }
    let offset = map.to_offset(
        buffer,
        DisplayPoint {
            row: target,
            col: goal,
        },
    );
    (offset, Some(goal))
}

pub(crate) fn vertical_offset(
    state: &EditorState,
    map: &DisplayMap,
    from: usize,
    rows: isize,
) -> Option<usize> {
    let point = map.to_display(state.buffer(), from);
    let target = point.row as isize + rows;
    if target < 0 || target as usize >= map.row_count() {
        return None;
    }
    Some(map.to_offset(
        state.buffer(),
        DisplayPoint {
            row: target as usize,
            col: point.col,
        },
    ))
}
