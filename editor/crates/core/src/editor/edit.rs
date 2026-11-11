//! Typing (auto-close pairs, auto-dedent), newlines with auto-indent,
//! deletion, clipboard and Tab / indentation commands.

use super::{Change, EditorState, Mark, Plan};
use crate::history::EditKind;
use crate::selection::Selection;
use crate::text::{next_grapheme, next_word_end, prev_grapheme, prev_word_start};

const OPENERS: &str = "([{";
const CLOSERS: &str = ")]}";

fn closer_for(open: char) -> Option<char> {
    OPENERS.find(open).and_then(|i| CLOSERS.chars().nth(i))
}

impl EditorState {
    /// Inserts typed text at every selection.
    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let mut chars = text.chars();
        let single = match (chars.next(), chars.next()) {
            (Some(c), None) => Some(c),
            _ => None,
        };
        self.apply_plans(EditKind::Insert, |state, sel| match single {
            Some(c) => state.plan_char(sel, c),
            None => Plan::replace(sel.range(), text, text.len()),
        });
    }

    fn plan_char(&self, sel: &Selection, c: char) -> Plan {
        let buffer = self.buffer();
        let typed = c.to_string();
        let pair = self
            .language
            .auto_pairs()
            .iter()
            .find(|(open, _)| *open == c);
        let closes = self
            .language
            .auto_pairs()
            .iter()
            .any(|(_, close)| *close == c);
        if self.settings.auto_close {
            if let Some(&(open, close)) = pair
                && !sel.is_empty()
            {
                let open_len = open.len_utf8();
                let (anchor, head) = if sel.reversed() {
                    (
                        Mark {
                            change: 1,
                            delta: 0,
                        },
                        Mark {
                            change: 0,
                            delta: open_len,
                        },
                    )
                } else {
                    (
                        Mark {
                            change: 0,
                            delta: open_len,
                        },
                        Mark {
                            change: 1,
                            delta: 0,
                        },
                    )
                };
                return Plan {
                    changes: vec![
                        Change::insert(sel.start(), open.to_string()),
                        Change::insert(sel.end(), close.to_string()),
                    ],
                    anchor,
                    head,
                };
            }
            if sel.is_empty() {
                let next = buffer.char_at(sel.head);
                let prev = buffer.char_before(sel.head);
                if closes && next == Some(c) {
                    return Plan::caret(sel.head + c.len_utf8());
                }
                if let Some(&(open, close)) = pair {
                    let next_ok = next.is_none_or(|n| {
                        n.is_whitespace() || CLOSERS.contains(n) || ",;:".contains(n)
                    });
                    let quote_ok = open != close
                        || prev.is_none_or(|p| !(p.is_alphanumeric() || p == '_' || p == open));
                    if next_ok && quote_ok {
                        return Plan::replace(
                            sel.head..sel.head,
                            format!("{open}{close}"),
                            open.len_utf8(),
                        );
                    }
                }
            }
        }
        if self.settings.auto_indent && sel.is_empty() && CLOSERS.contains(c) {
            let row = buffer.row_of(sel.head);
            let line_start = buffer.line_start(row);
            let prefix = buffer.slice(line_start..sel.head);
            if !prefix.is_empty()
                && prefix.chars().all(|ch| ch == ' ' || ch == '\t')
                && let Some(open_row) = self.opener_row(line_start, c)
            {
                let indent = buffer.indent_of(open_row);
                let len = indent.len() + c.len_utf8();
                return Plan::replace(line_start..sel.head, format!("{indent}{c}"), len);
            }
        }
        Plan::replace(sel.range(), typed.clone(), typed.len())
    }

    /// Row of the unmatched opener for `close` before `offset`.
    fn opener_row(&self, offset: usize, close: char) -> Option<usize> {
        let open = OPENERS.chars().nth(CLOSERS.find(close)?)?;
        let rope = self.buffer().rope();
        let mut chars = rope.chars_at(rope.byte_to_char(offset));
        let mut depth = 0usize;
        let mut index = rope.byte_to_char(offset);
        let mut budget = 200_000;
        while let Some(ch) = chars.prev() {
            index -= 1;
            budget -= 1;
            if budget == 0 {
                return None;
            }
            if ch == close {
                depth += 1;
            } else if ch == open {
                if depth == 0 {
                    return Some(rope.char_to_line(index));
                }
                depth -= 1;
            }
        }
        None
    }

    /// Enter: newline plus auto-indent; between brackets the closer moves
    /// to its own line.
    pub fn newline(&mut self) {
        let unit = self.settings.indent_unit();
        self.apply_plans(EditKind::Other, |state, sel| {
            let buffer = state.buffer();
            let start = sel.start();
            let line_start = buffer.line_start(buffer.row_of(start));
            let prefix = buffer.slice(line_start..start);
            if !state.settings.auto_indent {
                return Plan::replace(sel.range(), "\n", 1);
            }
            let indent: String = prefix
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect();
            let last = prefix.trim_end().chars().last();
            let opens = last.is_some_and(|c| OPENERS.contains(c))
                || (state.language.indents_after_colon() && last == Some(':'));
            let next = buffer.char_at(sel.end());
            if opens && last.and_then(closer_for).is_some() && next == last.and_then(closer_for) {
                let text = format!("\n{indent}{unit}\n{indent}");
                let caret = 1 + indent.len() + unit.len();
                Plan::replace(sel.range(), text, caret)
            } else if opens {
                let text = format!("\n{indent}{unit}");
                let len = text.len();
                Plan::replace(sel.range(), text, len)
            } else {
                let text = format!("\n{indent}");
                let len = text.len();
                Plan::replace(sel.range(), text, len)
            }
        });
    }

    /// Cmd+Enter: open a line below the caret's line.
    pub fn newline_below(&mut self) {
        let ends: Vec<Selection> = self
            .selections()
            .iter()
            .map(|s| Selection::caret(self.buffer().line_end(self.buffer().row_of(s.head))))
            .collect();
        self.set_selections(ends);
        self.newline();
    }

    /// Cmd+Shift+Enter: open a line above, with the current line's indent.
    pub fn newline_above(&mut self) {
        self.apply_plans(EditKind::Other, |state, sel| {
            let row = state.buffer().row_of(sel.head);
            let indent = state.buffer().indent_of(row);
            let start = state.buffer().line_start(row);
            let len = indent.len();
            Plan::replace(start..start, format!("{indent}\n"), len)
        });
    }

    pub fn backspace(&mut self) {
        let tab = self.settings.tab_size.max(1);
        self.apply_plans(EditKind::Delete, |state, sel| {
            let buffer = state.buffer();
            if !sel.is_empty() {
                return Plan::replace(sel.range(), "", 0);
            }
            let head = sel.head;
            if head == 0 {
                return Plan::caret(0);
            }
            let prev = buffer.char_before(head);
            let next = buffer.char_at(head);
            if state.settings.auto_close
                && let (Some(p), Some(n)) = (prev, next)
                && state.language.auto_pairs().contains(&(p, n))
            {
                return Plan::replace(head - p.len_utf8()..head + n.len_utf8(), "", 0);
            }
            let line_start = buffer.line_start(buffer.row_of(head));
            let prefix = buffer.slice(line_start..head);
            if state.settings.soft_tabs && !prefix.is_empty() && prefix.bytes().all(|b| b == b' ') {
                let remove = (prefix.len() - 1) % tab + 1;
                return Plan::replace(head - remove..head, "", 0);
            }
            Plan::replace(prev_grapheme(buffer, head)..head, "", 0)
        });
    }

    pub fn delete(&mut self) {
        self.delete_with(|state, head| next_grapheme(state.buffer(), head), false);
    }

    pub fn delete_word_left(&mut self) {
        self.delete_with(|state, head| prev_word_start(state.buffer(), head), true);
    }

    pub fn delete_word_right(&mut self) {
        self.delete_with(|state, head| next_word_end(state.buffer(), head), false);
    }

    /// Cmd+Backspace: delete to the line start (or join with the line above).
    pub fn delete_to_line_start(&mut self) {
        self.delete_with(
            |state, head| {
                let start = state.buffer().line_start(state.buffer().row_of(head));
                if start == head {
                    head.saturating_sub(1)
                } else {
                    start
                }
            },
            true,
        );
    }

    /// Cmd+Delete: delete to the line end (or join with the line below).
    pub fn delete_to_line_end(&mut self) {
        self.delete_with(
            |state, head| {
                let end = state.buffer().line_end(state.buffer().row_of(head));
                if end == head {
                    (head + 1).min(state.buffer().len())
                } else {
                    end
                }
            },
            false,
        );
    }

    fn delete_with(&mut self, target: impl Fn(&EditorState, usize) -> usize, backwards: bool) {
        self.apply_plans(EditKind::Delete, |state, sel| {
            if !sel.is_empty() {
                return Plan::replace(sel.range(), "", 0);
            }
            let other = target(state, sel.head);
            let range = if backwards {
                other..sel.head
            } else {
                sel.head..other
            };
            Plan::replace(range.start..range.end.max(range.start), "", 0)
        });
    }
}
