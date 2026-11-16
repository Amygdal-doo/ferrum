//! Headless tests of the editor's view features: code folding (keys,
//! gutter state, carets moving in and out of folds), the minimap and
//! Settings › Editor.

use std::path::PathBuf;

use ferrumeditor_core::buffer::Point;
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, point, px};

use crate::editor::EditorView;
use crate::tests::{init, primary};

const RUST: &str = "fn main() {\n    let a = 1;\n    let b = [\n        2,\n    ];\n}\n\nfn other() {\n    a();\n}\n";

fn rust_editor(cx: &mut TestAppContext) -> (Entity<EditorView>, &mut VisualTestContext) {
    init(cx);
    let (editor, cx) = cx.add_window_view(|window, cx| {
        EditorView::new(RUST, Some(PathBuf::from("/nowhere/main.rs")), window, cx)
    });
    editor.update_in(cx, |e, window, cx| e.focus(window, cx));
    cx.run_until_parked();
    assert!(editor.read_with(cx, |e, _| e.fold_regions_ready()));
    (editor, cx)
}

fn hidden(editor: &Entity<EditorView>, cx: &mut VisualTestContext) -> Vec<std::ops::Range<usize>> {
    editor.read_with(cx, |e, _| e.hidden_rows())
}

fn cursor(editor: &Entity<EditorView>, cx: &mut VisualTestContext) -> Point {
    editor.read_with(cx, |e, _| e.cursor_point())
}

#[gpui::test]
fn fold_and_unfold_with_keys(cx: &mut TestAppContext) {
    let (editor, cx) = rust_editor(cx);
    // Fold the innermost region around the caret (the array), then its
    // function; the closing brace rows stay visible.
    editor.update_in(cx, |e, _, _| e.state.go_to_point(3, 8));
    cx.simulate_keystrokes(&primary("cmd-alt-["));
    assert_eq!(hidden(&editor, cx), vec![3..4]);
    // The caret left the fold for the end of its first row.
    assert_eq!(cursor(&editor, cx), Point::new(2, 13));
    cx.simulate_keystrokes(&primary("cmd-alt-["));
    assert_eq!(hidden(&editor, cx), vec![1..5]);
    // Down from the header skips the folded rows.
    cx.simulate_keystrokes("down");
    assert_eq!(cursor(&editor, cx).row, 5);
    cx.simulate_keystrokes(&primary("up up cmd-alt-]"));
    assert_eq!(cursor(&editor, cx).row, 0);
    assert_eq!(hidden(&editor, cx), vec![3..4]);

    // Fold all / unfold all.
    cx.simulate_keystrokes(&primary("cmd-k cmd-0"));
    assert_eq!(hidden(&editor, cx), vec![1..5, 8..9]);
    cx.simulate_keystrokes(&primary("cmd-k cmd-j"));
    assert!(hidden(&editor, cx).is_empty());
}

#[gpui::test]
fn folds_follow_edits_and_open_for_the_caret(cx: &mut TestAppContext) {
    let (editor, cx) = rust_editor(cx);
    editor.update_in(cx, |e, window, cx| {
        e.state.go_to_point(7, 0);
        e.run_command("editor:toggle-fold", window, cx);
    });
    assert_eq!(hidden(&editor, cx), vec![8..9]);
    // A line typed above moves the fold down.
    editor.update_in(cx, |e, _, _| e.state.go_to_point(0, 0));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(hidden(&editor, cx), vec![9..10]);
    // A caret put inside the fold (go to line, search) unfolds it.
    editor.update_in(cx, |e, _, cx| {
        e.state.go_to_point(9, 2);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(hidden(&editor, cx).is_empty());
}

#[gpui::test]
fn minimap_click_scrolls_and_toggles(cx: &mut TestAppContext) {
    init(cx);
    let text: String = (0..400).map(|i| format!("let line_{i} = {i};\n")).collect();
    let (editor, cx) = cx.add_window_view(move |window, cx| {
        EditorView::new(&text, Some(PathBuf::from("/nowhere/long.rs")), window, cx)
    });
    cx.run_until_parked();
    let (minimap, scroll) = editor
        .read_with(cx, |e, _| e.minimap_layout())
        .expect("the minimap is on by default");
    assert_eq!(scroll, px(0.));
    // A click near the bottom of the strip scrolls down there.
    let target = point(minimap.bounds.center().x, minimap.bounds.bottom() - px(4.));
    cx.simulate_click(target, Modifiers::none());
    cx.run_until_parked();
    let (_, scroll) = editor
        .read_with(cx, |e, _| e.minimap_layout())
        .expect("minimap");
    assert!(scroll > px(0.), "{scroll:?}");
    // The caret did not move: the click only scrolled.
    assert_eq!(
        editor.read_with(cx, |e, _| e.cursor_point()),
        Point::new(0, 0)
    );

    editor.update_in(cx, |e, window, cx| {
        e.run_command("editor:toggle-minimap", window, cx);
    });
    cx.run_until_parked();
    assert!(editor.read_with(cx, |e, _| e.minimap_layout()).is_none());
}

#[gpui::test]
fn editor_settings_page_writes_config(cx: &mut TestAppContext) {
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let files = ferrumeditor_core::paths::UserFiles::new(dir.path());
    std::fs::write(
        &files.config,
        ferrumeditor_core::config::DEFAULT_CONFIG_TOML,
    )
    .expect("write");
    cx.update(|cx| {
        cx.global_mut::<crate::settings::Settings>().files = Some(files.clone());
        crate::settings_page::set_bool("soft_wrap", true, cx).expect("soft wrap");
        crate::settings_page::set_bool("minimap", false, cx).expect("minimap");
        assert!(crate::settings_page::set_bool("nope", true, cx).is_err());
        let config = &cx.global::<crate::settings::Settings>().config;
        assert!(config.soft_wrap && !config.minimap);
    });
    let written = std::fs::read_to_string(&files.config).expect("read");
    let (config, warnings) =
        ferrumeditor_core::config::Config::parse(&written).expect("valid config");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(config.soft_wrap && !config.minimap);
    // Comments survive.
    assert!(written.contains("# insert spaces for Tab"));
}
