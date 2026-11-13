//! Headless GPUI tests of the whole view with the scripted mock provider:
//! renders at 360px in both themes, sends with Enter (over the text
//! input's own Enter), streams, asks permission on a tool card, lists the
//! change for review, accepts it, picks an @-mention, cycles modes, and
//! opens the thread list and settings sheet.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ferrum_ui::theme::{self, GlobalTheme};
use ferrumagent_core::context::Mention;
use ferrumagent_core::modes::Mode;
use ferrumagent_core::permissions::Decision;
use ferrumagent_core::provider::mock::MockProvider;
use ferrumagent_core::session::SessionConfig;
use ferrumagent_core::thread::ToolStatus;
use gpui::{App, Entity, TestAppContext, VisualTestContext, px, size};
use serde_json::json;

use crate::actions::*;
use crate::view::{ChatEvent, ChatOptions, ChatView};

/// The real app gets these from `ferrum_ui::run`; bound here by action
/// name (the types are private), before FerrumChat's own.
fn bind_text_input_keys(cx: &mut App) {
    let predicate =
        Rc::new(gpui::KeyBindingContextPredicate::parse("FerrumTextInput").expect("predicate"));
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
    cx.bind_keys(bindings);
}

fn wait_until(
    cx: &mut VisualTestContext,
    what: &str,
    mut done: impl FnMut(&mut VisualTestContext) -> bool,
) {
    for _ in 0..400 {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        cx.executor().advance_clock(Duration::from_millis(50));
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {what}");
}

fn read<R>(
    view: &Entity<ChatView>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&ChatView) -> R,
) -> R {
    view.read_with(cx, |v, _| f(v))
}

#[gpui::test]
fn chat_view_runs_a_turn_at_narrow_width(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        bind_text_input_keys(cx);
        ferrum_ui::init_for_tests(cx);
        crate::init(cx);
    });
    let project = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("tempdir");
    let root = project.path().canonicalize().expect("canonical");
    std::fs::write(root.join("a.txt"), "alpha\nbeta\n").expect("write");
    let mock = MockProvider::new();
    mock.push(vec![
        ferrumagent_core::provider::StreamEvent::TextDelta("Editing `a.txt`.".into()),
        ferrumagent_core::provider::StreamEvent::ToolCall {
            id: "t1".into(),
            name: "edit_file".into(),
            input: json!({"path": "a.txt", "old_string": "beta", "new_string": "BETA"}),
        },
        ferrumagent_core::provider::StreamEvent::Stop(
            ferrumagent_core::provider::StopReason::ToolUse,
        ),
    ]);
    mock.push_text("Done **ok**.\n\n```rust\nfn x() {}\n```\n- one\n- two");
    let mut config = SessionConfig::isolated(&root, home.path());
    config.provider_override = Some(Arc::new(mock.clone()));
    let options = ChatOptions {
        config: Some(config),
        ..Default::default()
    };

    let (view, cx) =
        cx.add_window_view(|window, cx| ChatView::build(root.clone(), options, window, cx));
    cx.simulate_resize(size(px(360.), px(640.)));
    let events: Rc<RefCell<Vec<ChatEvent>>> = Rc::default();
    cx.update({
        let events = events.clone();
        let view = view.clone();
        move |_, cx| {
            cx.subscribe(&view, move |_, event: &ChatEvent, _| {
                events.borrow_mut().push(event.clone())
            })
            .detach();
        }
    });
    wait_until(cx, "the session", |cx| {
        read(&view, cx, |v| v.session().is_some())
    });

    // Type and send with Enter (our binding wins over the input's Enter).
    let composer = read(&view, cx, |v| v.composer.clone());
    cx.update(|window, cx| composer.update(cx, |input, cx| input.focus(window, cx)));
    cx.simulate_input("Make beta loud");
    cx.simulate_keystrokes("enter");
    wait_until(cx, "the permission prompt", |cx| {
        read(&view, cx, |v| !v.open_permission_requests().is_empty())
    });
    assert_eq!(composer.read_with(cx, |i, _| i.text().to_string()), "");
    let request = read(&view, cx, |v| v.open_permission_requests()[0].clone());
    assert_eq!(request.tool, "edit_file");
    assert_eq!(
        read(&view, cx, |v| v.thread().tool("t1").map(|c| c.status)),
        Some(ToolStatus::AwaitingPermission)
    );
    view.update(cx, |v, cx| v.respond(request.id, Decision::AllowOnce, cx));
    wait_until(cx, "the turn to end with a change", |cx| {
        read(&view, cx, |v| {
            !v.is_busy() && v.pending_changes().len() == 1
        })
    });
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        "alpha\nBETA\n"
    );
    let texts: Vec<String> = read(&view, cx, |v| {
        v.thread().messages.iter().map(|m| m.text()).collect()
    });
    assert_eq!(texts[0], "Make beta loud");
    assert!(texts.last().is_some_and(|t| t.starts_with("Done **ok**")));
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, ChatEvent::ChangesUpdated { files: 1 }))
    );

    // Expand the tool card and the changed file, render, accept all.
    view.update(cx, |v, cx| {
        v.expanded_tools.insert("t1".into());
        v.expanded_files.insert("a.txt".into());
        cx.notify();
    });
    cx.run_until_parked();
    cx.dispatch_action(AcceptAll);
    wait_until(cx, "the change to be accepted", |cx| {
        read(&view, cx, |v| v.pending_changes().is_empty())
    });

    // @-mention picker: type "@a.t", accept with Tab.
    cx.update(|window, cx| composer.update(cx, |input, cx| input.focus(window, cx)));
    cx.simulate_input("see @a.t");
    wait_until(cx, "mention suggestions", |cx| {
        read(&view, cx, |v| {
            v.picker.as_ref().is_some_and(|p| !p.items.is_empty())
        })
    });
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert_eq!(
        read(&view, cx, |v| v.mentions.clone()),
        [Mention::File {
            path: "a.txt".into()
        }]
    );
    assert_eq!(composer.read_with(cx, |i, _| i.text().to_string()), "see ");

    // Modes, thread list, settings, and the light theme all render.
    cx.dispatch_action(CycleMode);
    assert_eq!(read(&view, cx, |v| v.thread().mode), Mode::Ask);
    cx.dispatch_action(ToggleThreads);
    cx.run_until_parked();
    assert_eq!(read(&view, cx, |v| v.threads.len()), 1);
    cx.dispatch_action(ToggleThreads);
    cx.dispatch_action(OpenSettings);
    cx.run_until_parked();
    assert!(read(&view, cx, |v| v.settings.is_some()), "settings opened");
    wait_until(cx, "key status", |cx| {
        read(&view, cx, |v| {
            v.settings
                .as_ref()
                .is_some_and(|s| s.forms.iter().all(|f| f.status != "Checking…"))
        })
    });
    cx.update(|window, cx| {
        GlobalTheme::update_theme(cx, Arc::new(theme::light()));
        window.refresh();
    });
    cx.run_until_parked();
    cx.dispatch_action(Dismiss);
    cx.run_until_parked();
    assert!(read(&view, cx, |v| v.settings.is_none()));
    assert_eq!(mock.remaining(), 0);
}
