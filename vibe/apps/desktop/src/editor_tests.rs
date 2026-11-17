//! Headless tests of the Editor column: text, images and other files,
//! Open in FerrumIDE, moving and closing the column, its layout saved per
//! project, and unsaved changes asked about on close.

use super::*;
use crate::editor::{EditorEvent, EditorView};

fn editor(app: &Entity<FerrumEngineer>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    read(app, cx, |a| a.editor.clone().expect("editor"))
}

#[gpui::test]
fn the_editor_column_opens_files_moves_and_is_saved(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let root = fx.path("a");
    let text = root.join("notes.md");
    let image = root.join("logo.svg");
    let binary = root.join("data.bin");
    std::fs::write(&text, "# Notes\nline two\n").expect("write");
    std::fs::write(
        &image,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"/>"#,
    )
    .expect("write");
    std::fs::write(&binary, [0u8, 1, 2, 255, 0]).expect("write");
    let (app, cx) = fx.open(cx, Some(root.clone()));

    for (path, line) in [(&text, Some(2)), (&image, None), (&binary, None)] {
        update(&app, cx, |a, w, cx| a.open_file(path, line, w, cx));
    }
    assert_eq!(drawn(cx), [Column::Chat, Column::Editor]);
    let view = editor(&app, cx);
    view.read_with(cx, |e, _| {
        assert_eq!(e.paths(), [text.clone(), image.clone(), binary.clone()]);
        // Only text gets an editor; the binary is described.
        assert!(e.active_editor().is_none());
    });
    update(&app, cx, |a, w, cx| a.open_file(&text, Some(2), w, cx));
    let code = view.read_with(cx, |e, _| e.active_editor().expect("text"));
    assert_eq!(code.read_with(cx, |c, _| c.path().to_path_buf()), text);

    // Open in FerrumIDE goes to the stub, at the cursor's line.
    view.update(cx, |_, cx| {
        cx.emit(EditorEvent::OpenInIde {
            path: text.clone(),
            line: Some(2),
        });
    });
    assert_eq!(
        *fx.opened.borrow(),
        [(Some(root.clone()), text.clone(), Some(2))]
    );

    // Moved left of Git, never left of the chat; saved for the project.
    cx.simulate_keystrokes("cmd-alt-2");
    draw(cx);
    assert_eq!(drawn(cx), [Column::Chat, Column::Git, Column::Editor]);
    view.update(cx, |_, cx| cx.emit(EditorEvent::Move(-1)));
    draw(cx);
    assert_eq!(drawn(cx), [Column::Chat, Column::Editor, Column::Git]);
    view.update(cx, |_, cx| cx.emit(EditorEvent::Move(-1)));
    draw(cx);
    assert_eq!(drawn(cx), [Column::Chat, Column::Editor, Column::Git]);
    let saved = layout::load(&fx.layouts_file(), Some(&root));
    assert_eq!(saved.visible(), [Column::Chat, Column::Editor, Column::Git]);

    // Another project keeps its own layout; coming back restores this one.
    update(&app, cx, |a, w, cx| a.set_project(fx.path("b"), w, cx));
    update(&app, cx, |a, w, cx| {
        a.set_visible(Column::Editor, false, w, cx)
    });
    assert_eq!(drawn(cx), [Column::Chat, Column::Git]);
    update(&app, cx, |a, w, cx| a.set_project(root.clone(), w, cx));
    assert_eq!(drawn(cx), [Column::Chat, Column::Editor, Column::Git]);

    // Closing hides the column (focus goes back to the chat); its files
    // stay open.
    update(&app, cx, |a, w, cx| a.open_file(&text, None, w, cx));
    view.update(cx, |_, cx| cx.emit(EditorEvent::Close));
    draw(cx);
    assert_eq!(drawn(cx), [Column::Chat, Column::Git]);
    cx.simulate_keystrokes("cmd-alt-4");
    draw(cx);
    assert_eq!(drawn(cx), [Column::Chat, Column::Editor, Column::Git]);
    assert_eq!(view.read_with(cx, |e, _| e.paths().len()), 3);
}

#[gpui::test]
fn closing_an_edited_file_asks_first(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let root = fx.path("a");
    let file = root.join("main.rs");
    std::fs::write(&file, "fn main() {}\n").expect("write");
    let (app, cx) = fx.open(cx, Some(root.clone()));
    update(&app, cx, |a, w, cx| a.open_file(&file, None, w, cx));
    let view = editor(&app, cx);
    let code = view.read_with(cx, |e, _| e.active_editor().expect("text"));
    code.update(cx, |c, cx| {
        c.state.insert_text("// edited\n");
        cx.notify();
    });

    // Cancel keeps it; Save writes it and closes the tab.
    view.update(cx, |e, cx| e.close(&file, cx));
    view.update(cx, |e, cx| e.answer_close(None, cx));
    assert_eq!(
        view.read_with(cx, |e, _| e.paths()),
        std::slice::from_ref(&file)
    );
    view.update(cx, |e, cx| e.close(&file, cx));
    view.update(cx, |e, cx| e.answer_close(Some(true), cx));
    draw(cx);
    assert!(view.read_with(cx, |e, _| e.paths()).is_empty());
    assert_eq!(
        std::fs::read_to_string(&file).expect("read"),
        "// edited\nfn main() {}\n"
    );

    // The agent's writes reload a file without edits of its own.
    update(&app, cx, |a, w, cx| a.open_file(&file, None, w, cx));
    std::fs::write(&file, "fn changed() {}\n").expect("write");
    // Disk stamps have a coarse resolution: make the change visible.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&file)
        .and_then(|f| f.set_modified(later))
        .expect("touch");
    let chat = read(&app, cx, |a| a.chat().expect("chat"));
    chat.update(cx, |_, cx| cx.emit(ChatEvent::ChangesUpdated { files: 1 }));
    draw(cx);
    let code = view.read_with(cx, |e, _| e.active_editor().expect("text"));
    assert_eq!(
        code.read_with(cx, |c, _| c.state.text()),
        "fn changed() {}\n"
    );
}
