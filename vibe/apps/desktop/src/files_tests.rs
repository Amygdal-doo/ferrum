//! Headless window tests of the Files pane: shown on the project, follows
//! project switches, toggles and resizes (saved with the layout), opens
//! files in the Editor, and drags files onto the chat as context, from the
//! tree and from the Finder.

use gpui::{ExternalPaths, FileDropEvent, Focusable as _};

use super::*;
use crate::files::FilesView;
use ferrumagent_ui::core::context::Mention;

/// `a` with a source folder, an ignored build folder and a README.
fn project(fx: &Fixture) -> PathBuf {
    let root = fx.path("a");
    std::fs::create_dir_all(root.join("src")).expect("mkdir");
    std::fs::create_dir_all(root.join("target")).expect("mkdir");
    std::fs::write(root.join(".gitignore"), "target/\n").expect("write");
    std::fs::write(root.join("README.md"), "# a\n").expect("write");
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write");
    root
}

fn files(app: &Entity<FerrumEngineer>, cx: &mut VisualTestContext) -> Entity<FilesView> {
    read(app, cx, |a| a.files.clone()).expect("files pane")
}

fn row_names(files: &Entity<FilesView>, cx: &mut VisualTestContext) -> Vec<String> {
    files.read_with(cx, |f, _| f.rows().iter().map(|r| r.name.clone()).collect())
}

fn mentions(app: &Entity<FerrumEngineer>, cx: &mut VisualTestContext) -> Vec<Mention> {
    let chat = read(app, cx, |a| a.chat()).expect("chat");
    chat.read_with(cx, |c, _| c.mentions().to_vec())
}

/// The chat's context chips once there are `n` (they are read off the UI
/// thread).
fn wait_for_mentions(
    app: &Entity<FerrumEngineer>,
    cx: &mut VisualTestContext,
    n: usize,
) -> Vec<Mention> {
    for _ in 0..300 {
        draw(cx);
        let got = mentions(app, cx);
        if got.len() >= n {
            return got;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("expected {n} mentions, got {:?}", mentions(app, cx));
}

#[gpui::test]
fn the_files_pane_shows_the_project_and_follows_switches(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let root = project(&fx);
    std::fs::write(fx.path("b").join("other.txt"), "").expect("write");
    let (app, cx) = fx.open(cx, Some(root.clone()));
    assert!(cx.debug_bounds("files-pane").is_some(), "shown by default");
    let tree = files(&app, cx);
    assert_eq!(
        row_names(&tree, cx),
        ["src", ".gitignore", "README.md"],
        "folders first, target/ ignored"
    );
    // The pane is left of the chat.
    let pane = cx.debug_bounds("files-pane").expect("pane");
    let chat = cx.debug_bounds("column-chat").expect("chat");
    assert!(pane.right() <= chat.left());

    // A folder expands on click.
    let src = cx.debug_bounds("file-src").expect("src row").center();
    cx.simulate_click(src, Modifiers::none());
    draw(cx);
    assert!(row_names(&tree, cx).contains(&"main.rs".to_string()));

    // Switching project switches the tree; switching back restores it as
    // it was left.
    update(&app, cx, |a, w, cx| a.open_project(fx.path("b"), w, cx));
    assert_eq!(row_names(&tree, cx), ["other.txt"]);
    update(&app, cx, |a, w, cx| a.open_project(root.clone(), w, cx));
    assert!(row_names(&tree, cx).contains(&"main.rs".to_string()));
}

#[gpui::test]
fn the_files_pane_toggles_and_resizes_with_the_layout(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let root = project(&fx);
    let (app, cx) = fx.open(cx, Some(root.clone()));
    let chat = width(cx, "column-chat");

    // ⌥⌘1 hides it (the chat gets the room) and the layout remembers.
    update(&app, cx, |a, w, cx| w.focus(a.focus_handle(), cx));
    cx.simulate_keystrokes("cmd-alt-1");
    draw(cx);
    assert!(cx.debug_bounds("files-pane").is_none());
    assert!(width(cx, "column-chat") > chat + 150.);
    assert!(!layout::load(&fx.layouts_file(), Some(&root)).files.visible);
    cx.simulate_keystrokes("cmd-alt-1");
    draw(cx);
    assert!(cx.debug_bounds("files-pane").is_some());

    // Dragging its splitter widens it; the width is saved per project.
    let before = width(cx, "files-pane");
    let start = cx
        .debug_bounds("splitter-files")
        .expect("splitter")
        .center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    for step in 1..=4 {
        let at = point(start.x + px(20. * step as f32), start.y);
        cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
    }
    cx.simulate_mouse_up(
        point(start.x + px(80.), start.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    draw(cx);
    let after = width(cx, "files-pane");
    assert!((after - before - 80.).abs() < 6., "{before} → {after}");
    wait(&app, cx, |_| {
        (layout::load(&fx.layouts_file(), Some(&root)).files.width - after).abs() < 1.
    });
}

#[gpui::test]
fn enter_opens_the_selected_file_in_the_editor(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let root = project(&fx);
    let (app, cx) = fx.open(cx, Some(root.clone()));
    let tree = files(&app, cx);
    cx.update(|window, cx| window.focus(&tree.focus_handle(cx), cx));
    // ↓ selects `src`, → expands it, ↓ selects main.rs, Enter opens it.
    cx.simulate_keystrokes("down right down enter");
    draw(cx);
    let editor = read(&app, cx, |a| a.editor.clone().expect("editor"));
    let main = root.join("src/main.rs");
    let open = |cx: &mut VisualTestContext| editor.read_with(cx, |e, _| e.paths());
    assert_eq!(open(cx), std::slice::from_ref(&main));
    // Enter on a folder folds it instead.
    cx.update(|window, cx| window.focus(&tree.focus_handle(cx), cx));
    cx.simulate_keystrokes("up enter");
    draw(cx);
    assert!(!row_names(&tree, cx).contains(&"main.rs".to_string()));
    assert_eq!(open(cx), [main]);
    assert!(fx.opened.borrow().is_empty());
}

#[gpui::test]
fn files_dragged_from_the_tree_become_chat_context(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let root = project(&fx);
    let (app, cx) = fx.open(cx, Some(root.clone()));
    let tree = files(&app, cx);

    // ⌘-click selects README.md and src; dragging one carries both.
    for row in ["file-README.md", "file-src"] {
        let at = cx.debug_bounds(row).expect(row).center();
        cx.simulate_click(at, Modifiers::command());
    }
    draw(cx);
    assert_eq!(tree.read_with(cx, |f, _| f.selection().len()), 2);
    let start = cx.debug_bounds("file-README.md").expect("row").center();
    let target = cx.debug_bounds("column-chat").expect("chat").center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(start.x + px(20.), start.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(target, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::none());
    assert_eq!(
        wait_for_mentions(&app, cx, 2),
        [
            Mention::Folder { path: "src".into() },
            Mention::File {
                path: "README.md".into()
            }
        ]
    );
}

#[gpui::test]
fn files_dropped_from_the_finder_become_chat_context(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let root = project(&fx);
    let outside = fx.path("b").join("notes.md");
    std::fs::write(&outside, "one\ntwo\n").expect("write");
    let (app, cx) = fx.open(cx, Some(root.clone()));
    let target = cx.debug_bounds("column-chat").expect("chat").center();
    cx.simulate_event(FileDropEvent::Entered {
        position: target,
        paths: ExternalPaths(vec![root.join("src/main.rs"), outside.clone()].into()),
    });
    cx.simulate_event(FileDropEvent::Pending { position: target });
    cx.simulate_event(FileDropEvent::Submit { position: target });
    let got = wait_for_mentions(&app, cx, 2);
    assert_eq!(
        got[0],
        Mention::File {
            path: "src/main.rs".into()
        }
    );
    // A file outside the project goes in by its content.
    match &got[1] {
        Mention::Selection { path, text, .. } => {
            assert_eq!(
                (path.as_str(), text.as_str()),
                (outside.to_str().expect("utf-8"), "one\ntwo\n")
            );
        }
        other => panic!("{other:?}"),
    }
}

#[gpui::test]
fn opening_a_folder_points_every_pane_at_it(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let (app, cx) = fx.open(cx, None);
    assert!(read(&app, cx, |a| a.files.is_none()));
    // File › Open Folder… (⌘O) and the View toggles are handled.
    update(&app, cx, |a, w, cx| w.focus(a.focus_handle(), cx));
    cx.update(|window, cx| {
        for action in [
            &crate::app::OpenFolder as &dyn gpui::Action,
            &crate::app::ToggleFiles,
            &crate::app::ToggleGit,
            &crate::app::ToggleTerm,
            &crate::app::ToggleSidebar,
        ] {
            assert!(window.is_action_available(action, cx), "{}", action.name());
        }
    });
    update(&app, cx, |a, w, cx| a.open_project(fx.path("b"), w, cx));
    let tree = files(&app, cx);
    assert_eq!(
        tree.read_with(cx, |f, _| f.root().map(Path::to_path_buf)),
        Some(fx.path("b"))
    );
    let chat = read(&app, cx, |a| a.chat()).expect("chat");
    assert_eq!(
        chat.read_with(cx, |c, _| c.thread().project.clone()),
        fx.path("b")
    );
}
