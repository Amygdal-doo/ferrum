//! Headless GPUI tests of the shell: every tool renders, history and saved
//! inputs. No window is shown.

use std::time::Duration;

use ferrumkit_core::Tool;
use ferrumkit_core::store::KitStore;
use gpui::{Entity, TestAppContext, VisualTestContext};

use crate::app::FerrumKit;

fn window(
    cx: &mut TestAppContext,
    store: Option<std::path::PathBuf>,
) -> (Entity<FerrumKit>, &mut VisualTestContext) {
    cx.update(ferrum_ui::init_for_tests);
    let (app, cx) = cx.add_window_view(|window, cx| FerrumKit::with_store(store, window, cx));
    cx.run_until_parked();
    (app, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn every_tool_renders(cx: &mut TestAppContext) {
    let (app, cx) = window(cx, None);
    for tool in Tool::ALL {
        app.update(cx, |app, cx| {
            app.select(tool, cx);
            app.history_open = true;
        });
        cx.run_until_parked();
        draw(cx);
        assert_eq!(app.read_with(cx, |app, _| app.selected()), tool);
    }
}

/// Types `text` into the first input field of `tool` as the user would.
fn type_into(app: &Entity<FerrumKit>, tool: Tool, text: &str, cx: &mut VisualTestContext) {
    let input = app.read_with(cx, |app, _| {
        app.view(tool).expect("view").inputs()[0].clone()
    });
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            let len = input.text().len();
            input.select_range(0..len, cx);
            input.insert(text, window, cx);
        });
    });
}

#[gpui::test]
fn records_restores_and_saves_inputs(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("kit").join("inputs.json");
    let (app, cx) = window(cx, Some(path.clone()));
    app.update(cx, |app, cx| app.select(Tool::Base64, cx));
    draw(cx);

    type_into(&app, Tool::Base64, "hello", cx);
    // Recorded after a pause in typing; further typing in the same session
    // updates that entry.
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    type_into(&app, Tool::Base64, "hello world", cx);
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    let stored = KitStore::load(&path).expect("load");
    assert_eq!(stored.history(Tool::Base64).len(), 1);
    assert_eq!(stored.history(Tool::Base64)[0].inputs, ["hello world"]);

    // Saved under a name, then restored after the input changed.
    app.update(cx, |app, cx| {
        app.save_name
            .update(cx, |input, cx| input.set_text("greeting", cx));
        app.save_input(cx);
    });
    type_into(&app, Tool::Base64, "other", cx);
    app.update(cx, |app, cx| {
        let saved = app.store.saved(Tool::Base64)[0].inputs.clone();
        app.restore(saved, cx);
        let values = app.view(Tool::Base64).expect("view").values(cx);
        assert_eq!(values, ["hello world"]);
    });
    assert_eq!(
        KitStore::load(&path).expect("load").saved(Tool::Base64)[0].name,
        "greeting"
    );

    // JWT keeps no history by default.
    app.update(cx, |app, cx| app.select(Tool::Jwt, cx));
    draw(cx);
    type_into(&app, Tool::Jwt, "eyJhbGciOiJIUzI1NiJ9.e30.x", cx);
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    app.update(cx, |app, _| {
        assert!(app.store.history(Tool::Jwt).is_empty());
    });
}

#[gpui::test]
fn wrap_applies_to_every_tool_and_is_remembered(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputs.json");
    let (app, cx) = window(cx, Some(path.clone()));
    let wraps = |app: &Entity<FerrumKit>, tool: Tool, cx: &mut VisualTestContext| {
        app.read_with(cx, |app, cx| {
            app.view(tool)
                .expect("view")
                .fields()
                .all(|field| field.read(cx).wraps())
        })
    };
    assert!(!wraps(&app, Tool::Json, cx));
    cx.dispatch_action(crate::app::ToggleWrap);
    assert!(wraps(&app, Tool::Json, cx));
    // A tool opened later wraps too.
    app.update(cx, |app, cx| app.select(Tool::Jwt, cx));
    draw(cx);
    assert!(wraps(&app, Tool::Jwt, cx));
    assert!(KitStore::load(&path).expect("load").wrap);
}
