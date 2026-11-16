//! ⌘K agent edits against a scripted model: the prompt, the answer shown
//! for review, Reject putting the old lines back, Accept keeping the new
//! ones as one undo step, and a failing model reported in the prompt.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ferrumagent_ui::core::models::ModelStore;
use ferrumagent_ui::core::provider::mock::MockProvider;
use ferrumagent_ui::core::secrets::MemoryStore;
use ferrumagent_ui::core::settings::AgentSettings;
use gpui::{Entity, TestAppContext, VisualTestContext};

use crate::editor::{EditorView, InlineModels};
use crate::tests::{init, primary};

const TEXT: &str = "fn main() {\n    let one = 1;\n    println!(\"{one}\");\n}\n";

fn setup(cx: &mut TestAppContext) -> (Entity<EditorView>, MockProvider, &mut VisualTestContext) {
    init(cx);
    // The model answers on FerrumChat's tokio runtime.
    cx.executor().allow_parking();
    let mock = MockProvider::new();
    let home =
        std::env::temp_dir().join(format!("ferrumeditor-inline-test-{}", std::process::id()));
    let store = ModelStore::new(
        home,
        Arc::new(MemoryStore::default()),
        Arc::new(Mutex::new(AgentSettings::default())),
        Some(Arc::new(mock.clone())),
        Vec::new(),
    );
    cx.update(|cx| cx.set_global(InlineModels(store)));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        EditorView::new(TEXT, Some(PathBuf::from("/nowhere/main.rs")), window, cx)
    });
    editor.update_in(cx, |e, window, cx| {
        e.state.go_to_point(1, 6);
        e.focus(window, cx);
    });
    cx.run_until_parked();
    (editor, mock, cx)
}

fn text(editor: &Entity<EditorView>, cx: &mut VisualTestContext) -> String {
    editor.read_with(cx, |e, _| e.state.text())
}

fn wait_for_review(editor: &Entity<EditorView>, cx: &mut VisualTestContext) {
    for _ in 0..200 {
        cx.run_until_parked();
        if editor
            .read_with(cx, |e, _| e.inline_state())
            .is_some_and(|s| s.0 || s.2.is_some())
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for the model");
}

#[gpui::test]
fn inline_edit_review_reject_and_accept(cx: &mut TestAppContext) {
    let (editor, mock, cx) = setup(cx);
    mock.push_text("```rust\n    let two = 2;\n```");
    mock.push_text("    let two = 2;");

    // ⌘K opens the prompt once the next key starts no ⌘K chord (or GPUI's
    // chord timeout passes); that key and the rest go to the prompt.
    cx.simulate_keystrokes(&primary("cmd-k u"));
    assert_eq!(
        editor.read_with(cx, |e, _| e.inline_state()),
        Some((false, false, None))
    );
    cx.simulate_input("se two");
    assert_eq!(text(&editor, cx), TEXT);
    cx.simulate_keystrokes("enter");
    wait_for_review(&editor, cx);
    assert_eq!(
        text(&editor, cx),
        "fn main() {\n    let two = 2;\n    println!(\"{one}\");\n}\n"
    );
    let request = &mock.requests()[0];
    let ferrumagent_ui::core::provider::Part::Text { text: prompt } = &request.messages[0].parts[0]
    else {
        panic!("a text prompt");
    };
    assert!(
        prompt.contains("<selection>\n    let one = 1;\n</selection>"),
        "{prompt}"
    );
    assert!(prompt.contains("Instruction: use two"), "{prompt}");

    // Escape rejects: the old line is back.
    cx.simulate_keystrokes("escape");
    assert_eq!(text(&editor, cx), TEXT);
    assert!(editor.read_with(cx, |e, _| e.inline_state()).is_none());

    // Again, and ⌘Enter accepts; one undo step reverts it.
    editor.update_in(cx, |e, window, cx| {
        e.run_command("editor:inline-edit", window, cx)
    });
    cx.simulate_input("use two");
    cx.simulate_keystrokes("enter");
    wait_for_review(&editor, cx);
    cx.simulate_keystrokes(&primary("cmd-enter"));
    assert!(editor.read_with(cx, |e, _| e.inline_state()).is_none());
    assert!(text(&editor, cx).contains("let two = 2;\n"));
    cx.simulate_keystrokes(&primary("cmd-z"));
    assert_eq!(text(&editor, cx), TEXT);
}

#[gpui::test]
fn inline_edit_reports_model_errors(cx: &mut TestAppContext) {
    let (editor, mock, cx) = setup(cx);
    mock.push(vec![ferrumagent_ui::core::provider::StreamEvent::Stop(
        ferrumagent_ui::core::provider::StopReason::Error("rate limited".into()),
    )]);
    editor.update_in(cx, |e, window, cx| {
        e.run_command("editor:inline-edit", window, cx)
    });
    cx.simulate_input("anything");
    cx.simulate_keystrokes("enter");
    wait_for_review(&editor, cx);
    assert_eq!(
        editor.read_with(cx, |e, _| e.inline_state()),
        Some((false, false, Some("rate limited".to_string())))
    );
    assert_eq!(text(&editor, cx), TEXT);
    cx.simulate_keystrokes("escape");
    assert!(editor.read_with(cx, |e, _| e.inline_state()).is_none());
}
