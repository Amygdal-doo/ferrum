//! Editor command tests. Text fixtures mark carets with `|` and selections
//! with `‹anchor … head›` (a selection written `›…‹` is reversed).

mod commands;
mod multi_cursor;

use super::{EditSettings, EditorState};
use crate::display::DisplayMap;
use crate::language::Language;
use crate::selection::Selection;

pub(super) fn parse(fixture: &str) -> (String, Vec<Selection>) {
    let mut text = String::new();
    let mut selections = Vec::new();
    let mut open: Option<(usize, bool)> = None;
    for c in fixture.chars() {
        match c {
            '|' => selections.push(Selection::caret(text.len())),
            '‹' | '›' => match open.take() {
                None => open = Some((text.len(), c == '‹')),
                Some((start, forward)) => {
                    let (anchor, head) = if forward {
                        (start, text.len())
                    } else {
                        (text.len(), start)
                    };
                    selections.push(Selection::new(anchor, head));
                }
            },
            _ => text.push(c),
        }
    }
    (text, selections)
}

pub(super) fn render(state: &EditorState) -> String {
    let text = state.text();
    let mut marks: Vec<(usize, &str)> = Vec::new();
    for s in state.selections() {
        if s.is_empty() {
            marks.push((s.head, "|"));
        } else if s.reversed() {
            marks.push((s.head, "›"));
            marks.push((s.anchor, "‹"));
        } else {
            marks.push((s.anchor, "‹"));
            marks.push((s.head, "›"));
        }
    }
    marks.sort_by_key(|(at, _)| *at);
    let mut out = String::new();
    let mut last = 0;
    for (at, mark) in marks {
        out.push_str(&text[last..at]);
        out.push_str(mark);
        last = at;
    }
    out.push_str(&text[last..]);
    out
}

pub(super) fn editor(fixture: &str, language: Language) -> EditorState {
    let (text, selections) = parse(fixture);
    let mut state = EditorState::new(&text, language, EditSettings::default());
    state.set_selections(selections);
    state
}

pub(super) fn plain(fixture: &str) -> EditorState {
    editor(fixture, Language::PlainText)
}

pub(super) fn map(state: &EditorState) -> DisplayMap {
    let mut map = DisplayMap::new(4, None);
    map.sync(state.buffer(), 4, None);
    map
}

#[test]
fn fixtures_round_trip() {
    for fixture in ["a|b", "‹ab›c", "›ab‹c", "|x\n|y"] {
        assert_eq!(render(&plain(fixture)), fixture);
    }
}

#[test]
fn typing_replaces_selection_and_undo_groups_typing() {
    let mut s = plain("he‹ll›o");
    s.insert_text("y");
    s.insert_text("!");
    assert_eq!(render(&s), "hey!|o");
    s.undo();
    assert_eq!(render(&s), "he‹ll›o");
    s.redo();
    assert_eq!(render(&s), "hey!|o");
}

#[test]
fn cursor_move_breaks_undo_group() {
    let mut s = plain("|");
    s.insert_text("a");
    s.insert_text("b");
    s.move_left(false);
    s.move_right(false);
    s.insert_text("c");
    s.undo();
    assert_eq!(render(&s), "ab|");
    s.undo();
    assert_eq!(render(&s), "|");
}

#[test]
fn dirty_flag_tracks_saves_and_undo() {
    let mut s = plain("|");
    assert!(!s.is_dirty());
    s.insert_text("x");
    assert!(s.is_dirty());
    s.mark_saved();
    assert!(!s.is_dirty());
    s.undo();
    assert!(s.is_dirty());
    s.redo();
    assert!(!s.is_dirty());
}

#[test]
fn backspace_and_delete() {
    let mut s = plain("ab|c");
    s.backspace();
    assert_eq!(render(&s), "a|c");
    s.delete();
    assert_eq!(render(&s), "a|");
    let mut s = plain("|ab");
    s.backspace();
    assert_eq!(render(&s), "|ab");
    let mut s = plain("a\n|b");
    s.backspace();
    assert_eq!(render(&s), "a|b");
}

#[test]
fn backspace_removes_soft_tab_and_combining_grapheme() {
    let mut s = plain("        |x");
    s.backspace();
    assert_eq!(render(&s), "    |x");
    let mut s = plain("      |x");
    s.backspace();
    assert_eq!(render(&s), "    |x");
    let mut s = plain("e\u{301}|");
    s.backspace();
    assert_eq!(render(&s), "|");
}

#[test]
fn word_and_line_deletes() {
    let mut s = plain("foo bar|");
    s.delete_word_left();
    assert_eq!(render(&s), "foo |");
    let mut s = plain("|foo bar");
    s.delete_word_right();
    assert_eq!(render(&s), "| bar");
    let mut s = plain("x\n  foo|bar");
    s.delete_to_line_start();
    assert_eq!(render(&s), "x\n|bar");
    s.delete_to_line_start();
    assert_eq!(render(&s), "x|bar");
    s.delete_to_line_end();
    assert_eq!(render(&s), "x|");
}

#[test]
fn motions_word_line_and_doc() {
    let mut s = plain("  foo bar|\nx");
    s.move_word_left(false);
    assert_eq!(render(&s), "  foo |bar\nx");
    s.move_line_start(false);
    assert_eq!(render(&s), "  |foo bar\nx");
    s.move_line_start(false);
    assert_eq!(render(&s), "|  foo bar\nx");
    s.move_line_end(true);
    assert_eq!(render(&s), "‹  foo bar›\nx");
    s.move_right(false);
    assert_eq!(render(&s), "  foo bar|\nx");
    s.move_doc_end(false);
    assert_eq!(render(&s), "  foo bar\nx|");
    s.move_doc_start(true);
    assert_eq!(render(&s), "›  foo bar\nx‹");
}

#[test]
fn vertical_motion_keeps_goal_column() {
    let mut s = plain("abcdef\nab\nabcdef");
    s.go_to_point(0, 5);
    let m = map(&s);
    s.move_vertical(1, false, &m);
    assert_eq!(render(&s), "abcdef\nab|\nabcdef");
    s.move_vertical(1, false, &m);
    assert_eq!(render(&s), "abcdef\nab\nabcde|f");
    s.move_vertical(1, false, &m);
    assert_eq!(render(&s), "abcdef\nab\nabcdef|");
    s.move_vertical(-5, true, &m);
    assert_eq!(render(&s), "›abcdef\nab\nabcdef‹");
}

#[test]
fn vertical_motion_follows_soft_wrap() {
    let mut s = plain("hello world foo|");
    let mut m = DisplayMap::new(4, Some(8));
    m.sync(s.buffer(), 4, Some(8));
    s.move_vertical(-1, false, &m);
    assert_eq!(render(&s), "hello wor|ld foo");
}

#[test]
fn select_line_and_select_all() {
    let mut s = plain("a|b\ncd\nef");
    s.select_line();
    assert_eq!(render(&s), "‹ab\n›cd\nef");
    s.select_line();
    assert_eq!(render(&s), "‹ab\ncd\n›ef");
    s.select_all();
    assert_eq!(render(&s), "‹ab\ncd\nef›");
}

#[test]
fn escape_collapses_then_reports_nothing_to_do() {
    let mut s = plain("|a\n|b");
    assert!(s.escape());
    assert_eq!(s.selections().len(), 1);
    let mut s = plain("‹ab›");
    assert!(s.escape());
    assert_eq!(render(&s), "ab|");
    assert!(!s.escape());
}

#[test]
fn copy_cut_paste_line_mode() {
    let mut s = plain("one\ntw|o\nthree");
    let copied = s.copy();
    assert_eq!(copied.text, "two\n");
    assert!(copied.line_mode);
    let cut = s.cut();
    assert_eq!(cut.text, "two\n");
    assert_eq!(render(&s), "one\n|three");
    s.paste(&cut.text, true);
    assert_eq!(render(&s), "one\ntwo\n|three");
}

#[test]
fn paste_replaces_selection() {
    let mut s = plain("a‹bc›d");
    s.paste("XY\r\nZ", false);
    assert_eq!(render(&s), "aXY\nZ|d");
}

#[test]
fn go_to_point_clamps() {
    let mut s = plain("ab\ncd");
    s.go_to_point(1, 99);
    assert_eq!(render(&s), "ab\ncd|");
    s.go_to_point(9, 0);
    assert_eq!(render(&s), "ab\ncd|");
}

#[test]
fn click_word_and_row_selection() {
    let mut s = plain("foo bar\nbaz");
    s.select_word_at(5);
    assert_eq!(render(&s), "foo ‹bar›\nbaz");
    s.extend_primary_to(10);
    assert_eq!(render(&s), "foo ‹bar\nba›z");
    s.select_row(0);
    assert_eq!(render(&s), "‹foo bar\n›baz");
    s.click(1);
    assert_eq!(render(&s), "f|oo bar\nbaz");
}
