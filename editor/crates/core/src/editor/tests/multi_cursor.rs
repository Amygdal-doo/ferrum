//! Multi-cursor editing and selection commands.

use super::{map, plain, render};
use crate::display::DisplayPoint;

#[test]
fn typing_at_many_cursors_is_one_undo_step() {
    let mut s = plain("a|\nb|\nc|");
    s.insert_text("1");
    s.insert_text("2");
    assert_eq!(render(&s), "a12|\nb12|\nc12|");
    s.undo();
    assert_eq!(render(&s), "a|\nb|\nc|");
}

#[test]
fn backspace_and_newline_at_many_cursors() {
    let mut s = plain("ab|\ncd|");
    s.backspace();
    assert_eq!(render(&s), "a|\nc|");
    s.newline();
    assert_eq!(render(&s), "a\n|\nc\n|");
}

#[test]
fn cursors_merge_when_they_meet() {
    let mut s = plain("a|b|");
    s.backspace();
    s.backspace();
    assert_eq!(render(&s), "|");
    assert_eq!(s.selections().len(), 1);
}

#[test]
fn select_next_occurrence_expands_then_adds() {
    let mut s = plain("foo fo|o foobar foo");
    s.select_next_occurrence();
    assert_eq!(render(&s), "foo ‹foo› foobar foo");
    s.select_next_occurrence();
    assert_eq!(render(&s), "foo ‹foo› foobar ‹foo›");
    s.select_next_occurrence();
    assert_eq!(render(&s), "‹foo› ‹foo› foobar ‹foo›");
    s.select_next_occurrence();
    assert_eq!(s.selections().len(), 3);
    s.insert_text("x");
    assert_eq!(render(&s), "x| x| foobar x|");
}

#[test]
fn select_next_from_selection_matches_substrings() {
    let mut s = plain("‹ab›c abd");
    s.select_next_occurrence();
    assert_eq!(render(&s), "‹ab›c ‹ab›d");
}

#[test]
fn select_all_occurrences_of_word() {
    let mut s = plain("x |y x y");
    s.select_all_occurrences();
    assert_eq!(render(&s), "x ‹y› x ‹y›");
}

#[test]
fn split_selection_into_lines() {
    let mut s = plain("a‹bc\ndef\ngh›i");
    s.split_selection_into_lines();
    assert_eq!(render(&s), "a‹bc›\n‹def›\n‹gh›i");
    let mut s = plain("‹ab\ncd\n›ef");
    s.split_selection_into_lines();
    assert_eq!(render(&s), "‹ab›\n‹cd›\nef");
}

#[test]
fn add_cursor_above_and_below() {
    let mut s = plain("abc\na|bc\nabc");
    let m = map(&s);
    s.add_cursor_below(&m);
    s.add_cursor_above(&m);
    assert_eq!(render(&s), "a|bc\na|bc\na|bc");
}

#[test]
fn toggle_cursor_adds_and_removes() {
    let mut s = plain("|ab");
    s.toggle_cursor_at(2);
    assert_eq!(render(&s), "|ab|");
    s.toggle_cursor_at(2);
    assert_eq!(render(&s), "|ab");
}

#[test]
fn column_select_makes_rectangle() {
    let mut s = plain("abcd\nab\nabcd");
    let m = map(&s);
    s.column_select(
        DisplayPoint { row: 0, col: 1 },
        DisplayPoint { row: 2, col: 3 },
        &m,
    );
    assert_eq!(render(&s), "a‹bc›d\na‹b›\na‹bc›d");
}

#[test]
fn multi_cursor_paste_distributes_lines() {
    let mut s = plain("|\n|");
    s.paste("one\ntwo", false);
    assert_eq!(render(&s), "one|\ntwo|");
    let mut s = plain("|\n|");
    s.paste("same", false);
    assert_eq!(render(&s), "same|\nsame|");
}

#[test]
fn multi_cursor_copy_joins_selections() {
    let s = plain("‹a› ‹b›");
    assert_eq!(s.copy().text, "a\nb");
}

#[test]
fn multi_cursor_comment_and_duplicate() {
    let mut s = plain("a|\nb\nc|");
    s.language = crate::language::Language::Bash;
    s.toggle_comment();
    assert_eq!(s.text(), "# a\nb\n# c");
    s.duplicate_lines();
    assert_eq!(s.text(), "# a\n# a\nb\n# c\n# c");
    assert_eq!(s.selections().len(), 2);
}
