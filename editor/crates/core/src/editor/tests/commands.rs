//! Auto-pairs, auto-indent, comments, line commands, tabs and snippets.

use super::{editor, plain, render};
use crate::editor::matching_bracket;
use crate::language::Language;
use crate::snippets::SnippetSet;

#[test]
fn auto_close_pairs_and_skip_over() {
    let mut s = plain("|");
    s.insert_text("(");
    assert_eq!(render(&s), "(|)");
    s.insert_text("x");
    s.insert_text(")");
    assert_eq!(render(&s), "(x)|");
    let mut s = plain("|word");
    s.insert_text("(");
    assert_eq!(render(&s), "(|word");
}

#[test]
fn auto_close_wraps_selection() {
    let mut s = plain("‹ab›");
    s.insert_text("[");
    assert_eq!(render(&s), "[‹ab›]");
}

#[test]
fn quotes_do_not_pair_inside_words() {
    let mut s = editor("don|", Language::Python);
    s.insert_text("'");
    assert_eq!(render(&s), "don'|");
    let mut s = editor("x = |", Language::Python);
    s.insert_text("'");
    assert_eq!(render(&s), "x = '|'");
    s.insert_text("'");
    assert_eq!(render(&s), "x = ''|");
}

#[test]
fn backspace_deletes_empty_pair() {
    let mut s = plain("{|}");
    s.backspace();
    assert_eq!(render(&s), "|");
}

#[test]
fn newline_keeps_and_increases_indent() {
    let mut s = editor("    let x = 1;|", Language::Rust);
    s.newline();
    assert_eq!(render(&s), "    let x = 1;\n    |");
    let mut s = editor("fn f() {|}", Language::Rust);
    s.newline();
    assert_eq!(render(&s), "fn f() {\n    |\n}");
    let mut s = editor("if x:|", Language::Python);
    s.newline();
    assert_eq!(render(&s), "if x:\n    |");
}

#[test]
fn typing_closer_dedents_to_opener() {
    let mut s = editor("fn f() {\n    x\n    |", Language::Rust);
    s.insert_text("}");
    assert_eq!(render(&s), "fn f() {\n    x\n}|");
}

#[test]
fn newline_above_and_below() {
    let mut s = editor("  a|b\nc", Language::Rust);
    s.newline_below();
    assert_eq!(render(&s), "  ab\n  |\nc");
    let mut s = editor("  a|b", Language::Rust);
    s.newline_above();
    assert_eq!(render(&s), "  |\n  ab");
}

#[test]
fn toggle_line_comment_rust_and_python() {
    let mut s = editor("‹fn a() {\n    b();\n}›", Language::Rust);
    s.toggle_comment();
    assert_eq!(s.text(), "// fn a() {\n//     b();\n// }");
    s.toggle_comment();
    assert_eq!(s.text(), "fn a() {\n    b();\n}");
    let mut s = editor("    x = 1|\n    y = 2", Language::Python);
    s.toggle_comment();
    assert_eq!(render(&s), "    # x = 1|\n    y = 2");
    s.toggle_comment();
    assert_eq!(render(&s), "    x = 1|\n    y = 2");
}

#[test]
fn toggle_comment_uses_min_indent_and_skips_blank_lines() {
    let mut s = editor("‹  a\n\n    b›", Language::Yaml);
    s.toggle_comment();
    assert_eq!(s.text(), "  # a\n\n  #   b");
}

#[test]
fn toggle_block_comment_html_and_css() {
    let mut s = editor("  <p>hi</p>|", Language::Html);
    s.toggle_comment();
    assert_eq!(s.text(), "  <!-- <p>hi</p> -->");
    s.toggle_comment();
    assert_eq!(s.text(), "  <p>hi</p>");
    let mut s = editor("‹a { }\nb { }›", Language::Css);
    s.toggle_comment();
    assert_eq!(s.text(), "/* a { }\nb { } */");
}

#[test]
fn comment_toggle_is_one_undo_step() {
    let mut s = editor("a|\nb", Language::Go);
    s.toggle_comment();
    assert_eq!(s.text(), "// a\nb");
    s.undo();
    assert_eq!(render(&s), "a|\nb");
}

#[test]
fn duplicate_lines_moves_cursor_to_copy() {
    let mut s = plain("a|b\nc");
    s.duplicate_lines();
    assert_eq!(render(&s), "ab\na|b\nc");
    let mut s = plain("‹x\ny›");
    s.duplicate_lines();
    assert_eq!(s.text(), "x\ny\nx\ny");
}

#[test]
fn move_lines_up_and_down() {
    let mut s = plain("a\nb|\nc");
    s.move_lines_up();
    assert_eq!(render(&s), "b|\na\nc");
    s.move_lines_up();
    assert_eq!(render(&s), "b|\na\nc");
    s.move_lines_down();
    s.move_lines_down();
    assert_eq!(render(&s), "a\nc\nb|");
    s.undo();
    assert_eq!(render(&s), "a\nb|\nc");
    let mut s = plain("‹1\n2›\n3");
    s.move_lines_down();
    assert_eq!(render(&s), "3\n‹1\n2›");
}

#[test]
fn delete_and_join_lines() {
    let mut s = plain("a\nb|b\nc");
    s.delete_lines();
    assert_eq!(render(&s), "a\n|c");
    let mut s = plain("a\nb|");
    s.delete_lines();
    assert_eq!(s.text(), "a");
    let mut s = plain("foo  |\n    bar");
    s.join_lines();
    assert_eq!(s.text(), "foo bar");
}

#[test]
fn tab_inserts_to_next_stop_and_indents_selection() {
    let mut s = plain("ab|");
    s.tab(&[]);
    assert_eq!(render(&s), "ab  |");
    let mut s = plain("‹a\nb›");
    s.tab(&[]);
    assert_eq!(render(&s), "‹    a\n    b›");
    s.shift_tab();
    assert_eq!(render(&s), "‹a\nb›");
    let mut s = plain("\t|x");
    s.shift_tab();
    assert_eq!(render(&s), "|x");
}

#[test]
fn hard_tabs_setting() {
    let mut s = plain("|x");
    s.settings.soft_tabs = false;
    s.tab(&[]);
    assert_eq!(render(&s), "\t|x");
}

#[test]
fn upper_and_lower_case() {
    let mut s = plain("‹abc› de|f");
    s.upper_case();
    assert_eq!(s.text(), "ABC DEF");
    s.lower_case();
    assert_eq!(s.text(), "abc def");
}

#[test]
fn bracket_matching() {
    let s = plain("f(a[1], {b})");
    assert_eq!(matching_bracket(s.buffer(), 1), Some((1..2, 11..12)));
    assert_eq!(matching_bracket(s.buffer(), 12), Some((11..12, 1..2)));
    assert_eq!(matching_bracket(s.buffer(), 8), Some((8..9, 10..11)));
    assert_eq!(matching_bracket(s.buffer(), 0), None);
    assert_eq!(matching_bracket(plain("((").buffer(), 0), None);
}

fn snippet_set() -> SnippetSet {
    SnippetSet::parse(
        r#"
        [rust.fn]
        prefix = "fn"
        body = "fn ${1:name}($2) {\n    $0\n}"
        "#,
    )
    .expect("valid snippets")
}

#[test]
fn snippet_expands_and_walks_tab_stops() {
    let snippets = snippet_set().for_language(Language::Rust);
    let mut s = editor("  fn|", Language::Rust);
    s.tab(&snippets);
    assert_eq!(render(&s), "  fn ‹name›() {\n      \n  }");
    s.insert_text("go");
    s.tab(&snippets);
    assert_eq!(render(&s), "  fn go(|) {\n      \n  }");
    s.insert_text("x");
    s.tab(&snippets);
    assert_eq!(render(&s), "  fn go(x) {\n      |\n  }");
    assert!(s.snippet_session().is_none());
    s.tab(&snippets);
    assert_eq!(render(&s), "  fn go(x) {\n        |\n  }");
}

#[test]
fn snippet_prefix_needs_word_boundary() {
    let snippets = snippet_set().for_language(Language::Rust);
    let mut s = editor("xfn|", Language::Rust);
    s.tab(&snippets);
    assert_eq!(render(&s), "xfn |");
}

#[test]
fn snippet_shift_tab_goes_back_and_undo_restores() {
    let snippets = snippet_set().for_language(Language::Rust);
    let mut s = editor("fn|", Language::Rust);
    s.tab(&snippets);
    s.tab(&snippets);
    assert_eq!(render(&s), "fn name(|) {\n    \n}");
    s.shift_tab();
    assert_eq!(render(&s), "fn ‹name›() {\n    \n}");
    s.undo();
    assert_eq!(render(&s), "fn|");
}

#[test]
fn completion_snippet_replaces_range_and_walks_stops() {
    let mut s = editor("  v.pu|", Language::Rust);
    let end = s.primary().head;
    s.insert_snippet(end - 2..end, "push(${1:value})$0");
    assert_eq!(render(&s), "  v.push(‹value›)");
    s.insert_text("1");
    s.tab(&[]);
    assert_eq!(render(&s), "  v.push(1)|");
    assert!(s.snippet_session().is_none());
    s.undo();
    s.undo();
    assert_eq!(s.text(), "  v.pu");
}
