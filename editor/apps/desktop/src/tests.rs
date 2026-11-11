//! Headless GPUI smoke tests: real keystrokes go through the keymap,
//! `RunCommand` dispatch, the editor's input handler and the workspace.

use std::path::Path;

use ferrumeditor_core::buffer::Point;
use gpui::{App, Entity, Font, Pixels, TestAppContext, VisualTestContext, font, px};
use theme::{ThemeSettingsProvider, UiDensity};

use crate::actions;
use crate::editor::EditorView;
use crate::settings::{MAC, Settings};
use crate::workspace::Workspace;

struct TestFonts {
    ui: Font,
    buffer: Font,
}

impl ThemeSettingsProvider for TestFonts {
    fn ui_font<'a>(&'a self, _: &'a App) -> &'a Font {
        &self.ui
    }
    fn buffer_font<'a>(&'a self, _: &'a App) -> &'a Font {
        &self.buffer
    }
    fn ui_font_size(&self, _: &App) -> Pixels {
        px(14.)
    }
    fn buffer_font_size(&self, _: &App) -> Pixels {
        px(13.)
    }
    fn ui_density(&self, _: &App) -> UiDensity {
        UiDensity::Default
    }
}

/// The real app gets these from `ferrum_ui::run`; tests bind the few the
/// pickers and find bar rely on, by action name (the types are private).
fn bind_text_input_keys(cx: &mut App) {
    let predicate = gpui::KeyBindingContextPredicate::parse("FerrumTextInput").expect("predicate");
    let predicate = std::rc::Rc::new(predicate);
    let mut bindings = Vec::new();
    for (keys, action) in [
        ("enter", "text_input::Enter"),
        ("up", "text_input::Up"),
        ("down", "text_input::Down"),
        ("backspace", "text_input::Backspace"),
    ] {
        let action = cx.build_action(action, None).expect("text input action");
        bindings.push(
            gpui::KeyBinding::load(
                keys,
                action,
                Some(predicate.clone()),
                false,
                None,
                &gpui::DummyKeyboardMapper,
            )
            .expect("binding"),
        );
    }
    let escape = gpui::KeyBinding::new("escape", menu::Cancel, None);
    cx.bind_keys(bindings);
    cx.bind_keys([escape]);
}

pub(crate) fn init(cx: &mut TestAppContext) {
    cx.update(|cx| {
        bind_text_input_keys(cx);
        theme::set_theme_settings_provider(
            Box::new(TestFonts {
                ui: font("Helvetica"),
                buffer: font("Menlo"),
            }),
            cx,
        );
        theme::init(theme::LoadThemes::JustBase, cx);
        actions::capture_base_bindings(cx);
        let settings = Settings::load(None, Settings::default_commands());
        assert!(actions::apply_keymap(&settings.keymap, cx).is_empty());
        let theme = settings.theme.clone();
        cx.set_global(settings);
        crate::theme_apply::apply(&theme, cx);
    });
}

/// `cmd` on macOS, `ctrl` elsewhere.
pub(crate) fn primary(keys: &str) -> String {
    if MAC {
        keys.to_string()
    } else {
        keys.replace("cmd-", "ctrl-")
    }
}

fn editor_window<'a>(
    text: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<EditorView>, &'a mut VisualTestContext) {
    init(cx);
    let text = text.to_string();
    let (editor, cx) =
        cx.add_window_view(move |window, cx| EditorView::new(&text, None, window, cx));
    editor.update_in(cx, |editor, window, cx| editor.focus(window, cx));
    cx.run_until_parked();
    (editor, cx)
}

fn text(editor: &Entity<EditorView>, cx: &mut VisualTestContext) -> String {
    editor.read_with(cx, |e, _| e.state.text())
}

#[gpui::test]
fn typing_goes_through_the_input_handler(cx: &mut TestAppContext) {
    let (editor, cx) = editor_window("", cx);
    cx.simulate_input("fn main() {");
    assert_eq!(text(&editor, cx), "fn main() {}");
    cx.simulate_keystrokes("enter");
    assert_eq!(text(&editor, cx), "fn main() {\n    \n}");
    cx.simulate_input("x");
    assert_eq!(text(&editor, cx), "fn main() {\n    x\n}");
    cx.simulate_keystrokes(&primary("cmd-z"));
    assert_eq!(text(&editor, cx), "fn main() {\n    \n}");
    cx.simulate_keystrokes(&primary("cmd-a backspace"));
    assert_eq!(text(&editor, cx), "");
}

#[gpui::test]
fn editor_key_bindings_dispatch_commands(cx: &mut TestAppContext) {
    let (editor, cx) = editor_window("foo bar foo", cx);
    cx.simulate_keystrokes(&primary("cmd-d cmd-d"));
    cx.simulate_input("x");
    assert_eq!(text(&editor, cx), "x bar x");
    cx.simulate_keystrokes("escape");
    editor.read_with(cx, |e, _| assert_eq!(e.state.selections().len(), 1));
    cx.simulate_keystrokes(&primary("cmd-shift-d"));
    assert_eq!(text(&editor, cx), "x bar x\nx bar x");
    cx.simulate_keystrokes(&primary("cmd-/"));
    // Plain text has no comment syntax: nothing changes.
    assert_eq!(text(&editor, cx), "x bar x\nx bar x");
    cx.simulate_keystrokes("up home shift-end");
    editor.read_with(cx, |e, _| {
        assert_eq!(e.state.selected_texts(), vec!["x bar x".to_string()])
    });
}

#[gpui::test]
fn copy_and_paste_whole_lines(cx: &mut TestAppContext) {
    let (editor, cx) = editor_window("one\ntwo", cx);
    cx.simulate_keystrokes(&primary("cmd-c down"));
    cx.simulate_keystrokes(&primary("cmd-v"));
    assert_eq!(text(&editor, cx), "one\none\ntwo");
}

#[gpui::test]
fn editor_renders_highlighted_rust(cx: &mut TestAppContext) {
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("main.rs");
    std::fs::write(&path, "fn main() {\n\tlet s = \"héllo\"; // 日本\n}\n").expect("write");
    let (editor, cx) = cx.add_window_view(move |window, cx| {
        let file = ferrumeditor_core::files::load(&path).expect("load");
        EditorView::from_file(file, path.clone(), window, cx)
    });
    cx.run_until_parked();
    editor.update_in(cx, |e, window, cx| {
        e.focus(window, cx);
        e.soft_wrap = true;
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("down end");
    editor.read_with(cx, |e, _| {
        assert_eq!(e.language(), ferrumeditor_core::language::Language::Rust);
        assert_eq!(
            e.cursor_point(),
            Point::new(1, e.state.buffer().line_len(1))
        );
    });
}

pub(crate) fn workspace_window(
    cx: &mut TestAppContext,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    init(cx);
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    cx.run_until_parked();
    (workspace, cx)
}

fn active_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    workspace
        .read_with(cx, |w, _| w.active_editor())
        .map(|e| e.read_with(cx, |e, _| e.state.text()))
}

#[gpui::test]
fn workspace_opens_edits_and_saves_files(cx: &mut TestAppContext) {
    let (workspace, cx) = workspace_window(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("notes.md"), "# Notes\n").expect("write");
    let root = dir.path().to_path_buf();
    workspace.update_in(cx, |w, window, cx| {
        w.set_root(root.clone(), window, cx);
        w.open_file(&root.join("notes.md"), window, cx);
    });
    cx.run_until_parked();
    assert_eq!(active_text(&workspace, cx).as_deref(), Some("# Notes\n"));
    cx.simulate_keystrokes(&primary("cmd-down"));
    cx.simulate_input("hello");
    cx.simulate_keystrokes(&primary("cmd-s"));
    cx.run_until_parked();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("notes.md")).expect("read"),
        "# Notes\nhello"
    );
    // Markdown preview opens in a split and closes again.
    cx.simulate_keystrokes(&primary("cmd-shift-m"));
    cx.run_until_parked();
    cx.simulate_keystrokes(&primary("cmd-shift-m"));
    cx.run_until_parked();
    assert_eq!(
        active_text(&workspace, cx).as_deref(),
        Some("# Notes\nhello")
    );
}

#[gpui::test]
fn palette_finder_and_find_bar(cx: &mut TestAppContext) {
    let (workspace, cx) = workspace_window(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("src")).expect("mkdir");
    std::fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn alpha() {}\npub fn beta() {}\n",
    )
    .expect("write");
    let root = dir.path().to_path_buf();
    workspace.update_in(cx, |w, window, cx| w.set_root(root, window, cx));
    // File finder: type a fuzzy query and confirm.
    cx.simulate_keystrokes(&primary("cmd-p"));
    cx.simulate_input("slib");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        active_text(&workspace, cx).as_deref(),
        Some("pub fn alpha() {}\npub fn beta() {}\n")
    );
    // Command palette runs a command by name.
    cx.simulate_keystrokes(&primary("cmd-shift-p"));
    cx.simulate_input("duplicate lines");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        active_text(&workspace, cx).as_deref(),
        Some("pub fn alpha() {}\npub fn alpha() {}\npub fn beta() {}\n")
    );
    // Find bar selects the match; escape returns to the editor.
    cx.simulate_keystrokes(&primary("cmd-f"));
    cx.simulate_input("beta");
    cx.simulate_keystrokes("enter escape");
    let editor = workspace
        .read_with(cx, |w, _| w.active_editor())
        .expect("editor");
    editor.read_with(cx, |e, _| {
        assert_eq!(e.state.selected_texts(), vec!["beta".to_string()])
    });
    // Go to line.
    cx.simulate_keystrokes("ctrl-g");
    cx.simulate_input("2:5");
    cx.simulate_keystrokes("enter");
    editor.read_with(cx, |e, _| assert_eq!(e.cursor_point(), Point::new(1, 4)));
}

#[gpui::test]
fn splitting_panes_and_closing_tabs(cx: &mut TestAppContext) {
    let (workspace, cx) = workspace_window(cx);
    workspace.update_in(cx, |w, window, cx| w.new_file(window, cx));
    cx.simulate_keystrokes(&primary("cmd-k right"));
    cx.run_until_parked();
    cx.simulate_input("shared");
    assert_eq!(active_text(&workspace, cx).as_deref(), Some("shared"));
    // The same buffer is shown in both panes; closing one view needs no prompt.
    cx.simulate_keystrokes(&primary("cmd-w"));
    cx.run_until_parked();
    assert_eq!(active_text(&workspace, cx).as_deref(), Some("shared"));
}

#[gpui::test]
fn cli_targets_open_at_line(cx: &mut TestAppContext) {
    let (workspace, cx) = workspace_window(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "1\n2\n3\n").expect("write");
    let arg = format!("{}:3:1", file.display());
    let ferrumeditor_core::cli::CliAction::Run(targets) =
        ferrumeditor_core::cli::parse_args([arg], Path::new("/"))
    else {
        unreachable!("a path is a run target")
    };
    workspace.update_in(cx, |w, window, cx| w.open_targets(targets, window, cx));
    cx.run_until_parked();
    let editor = workspace
        .read_with(cx, |w, _| w.active_editor())
        .expect("editor");
    editor.read_with(cx, |e, _| assert_eq!(e.cursor_point(), Point::new(2, 0)));
}

pub(crate) fn init_with_user_dir(cx: &mut TestAppContext, dir: &Path) {
    init(cx);
    let files = ferrumeditor_core::paths::UserFiles::new(dir.join("editor"));
    cx.update(|cx| {
        let settings = Settings::load(Some(files), Settings::default_commands());
        actions::apply_keymap(&settings.keymap, cx);
        cx.set_global(settings);
    });
}

#[gpui::test]
fn keymap_and_config_reload_live(cx: &mut TestAppContext) {
    let home = tempfile::tempdir().expect("tempdir");
    init_with_user_dir(cx, home.path());
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    workspace.update_in(cx, |w, window, cx| w.new_file(window, cx));
    cx.simulate_input("abc");
    let keymap = home.path().join("editor/keymap.toml");
    let mut contents = std::fs::read_to_string(&keymap).expect("default keymap written");
    contents.push_str(&format!(
        "\"{}\" = \"editor:duplicate-lines\"\n",
        primary("cmd-d")
    ));
    std::fs::write(&keymap, contents).expect("write");
    std::fs::write(home.path().join("editor/config.toml"), "tab_size = 2\n").expect("write");
    workspace.update(cx, |w, cx| w.reload_settings(cx));
    cx.simulate_keystrokes(&primary("cmd-d"));
    assert_eq!(active_text(&workspace, cx).as_deref(), Some("abc\nabc"));
    cx.simulate_keystrokes("tab");
    // Tab size 2: column 3 → next stop at 4.
    assert_eq!(active_text(&workspace, cx).as_deref(), Some("abc\nabc "));
}

#[gpui::test]
fn session_round_trip(cx: &mut TestAppContext) {
    let home = tempfile::tempdir().expect("tempdir");
    let project = tempfile::tempdir().expect("tempdir");
    std::fs::write(project.path().join("a.txt"), "one\ntwo\nthree\n").expect("write");
    init_with_user_dir(cx, home.path());
    let root = project.path().to_path_buf();
    let (first, cx) = cx.add_window_view(Workspace::new);
    first.update_in(cx, |w, window, cx| {
        w.set_root(root.clone(), window, cx);
        w.open_file_at(&root.join("a.txt"), Some(2), Some(1), window, cx);
        w.save_session(cx);
    });
    let (second, cx) = cx.add_window_view(Workspace::new);
    let restored = second.update_in(cx, |w, window, cx| w.restore_session(window, cx));
    assert!(restored);
    cx.run_until_parked();
    let editor = second
        .read_with(cx, |w, _| w.active_editor())
        .expect("restored tab");
    editor.read_with(cx, |e, _| assert_eq!(e.cursor_point(), Point::new(2, 1)));
    second.read_with(cx, |w, _| {
        assert!(
            w.root
                .as_deref()
                .is_some_and(|r| r.ends_with(root.file_name().expect("name")))
        )
    });
}

#[gpui::test]
fn find_in_files_lists_and_opens_matches(cx: &mut TestAppContext) {
    let (workspace, cx) = workspace_window(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.rs"), "fn needle() {}\n").expect("write");
    std::fs::write(dir.path().join("b.rs"), "// no\n// needle here\n").expect("write");
    let root = dir.path().to_path_buf();
    workspace.update_in(cx, |w, window, cx| w.set_root(root, window, cx));
    cx.simulate_keystrokes(&primary("cmd-shift-f"));
    cx.simulate_input("needle");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let panel = workspace.read_with(cx, |w, _| w.project_find().clone());
    panel.read_with(cx, |p, _| {
        assert_eq!(p.result_rows(), 4, "{}", p.status());
        assert_eq!(p.status().as_ref(), "2 results in 2 files");
    });
    panel.update(cx, |p, cx| p.open_first_match(cx));
    cx.run_until_parked();
    let editor = workspace
        .read_with(cx, |w, _| w.active_editor())
        .expect("opened");
    editor.read_with(cx, |e, _| assert_eq!(e.cursor_point(), Point::new(0, 3)));
}

#[gpui::test]
fn tree_view_new_file_dialog_creates_and_opens(cx: &mut TestAppContext) {
    let (workspace, cx) = workspace_window(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    workspace.update_in(cx, |w, window, cx| {
        w.set_root(root.clone(), window, cx);
        w.run_command("tree-view:new-file", window, cx);
    });
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |w, _| w.has_dialog()));
    cx.simulate_input("new.txt");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let created = root.canonicalize().expect("canonical").join("new.txt");
    assert!(created.exists());
    assert!(!workspace.read_with(cx, |w, _| w.has_dialog()));
    let editor = workspace
        .read_with(cx, |w, _| w.active_editor())
        .expect("opened");
    editor.read_with(cx, |e, _| {
        assert_eq!(e.path.as_deref(), Some(created.as_path()))
    });
}

#[gpui::test]
fn project_bar_opens_the_picked_folder(cx: &mut TestAppContext) {
    let (workspace, cx) = workspace_window(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    // ⌥⌘O reaches the bar from anywhere in the window.
    cx.dispatch_action(ferrum_ui::ToggleProjectSwitcher);
    cx.run_until_parked();
    let bar = workspace.read_with(cx, |w, _| w.project_bar().clone());
    // Picking a project in the switcher opens it like Open Folder.
    bar.update(cx, |_, cx| {
        cx.emit(ferrum_ui::ProjectBarEvent::Open(root.clone()))
    });
    cx.run_until_parked();
    workspace.read_with(cx, |w, _| {
        assert_eq!(w.root.as_deref(), Some(root.as_path()));
        assert!(w.tree().is_some());
    });
}
