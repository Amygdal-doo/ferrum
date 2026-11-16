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
use crate::host::{GitRequest, GitStatus};
use crate::view::{ChatEvent, ChatOptions, ChatView};
use ferrumagent_core::permissions::Approval;

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

#[gpui::test]
fn shell_host_drives_threads_tasks_and_footer(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        bind_text_input_keys(cx);
        ferrum_ui::init_for_tests(cx);
        crate::init(cx);
    });
    let project = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("tempdir");
    let root = project.path().canonicalize().expect("canonical");
    let mock = MockProvider::new();
    mock.push_text("A plan.");
    let mut config = SessionConfig::isolated(&root, home.path());
    config.provider_override = Some(Arc::new(mock.clone()));
    let options = ChatOptions {
        config: Some(config),
        shell: true,
        ..Default::default()
    };
    let (view, cx) =
        cx.add_window_view(|window, cx| ChatView::build(root.clone(), options, window, cx));
    cx.simulate_resize(size(px(520.), px(640.)));
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

    // An automation starts a task before the session is open.
    view.update_in(cx, |v, window, cx| {
        v.start_task(Mode::Plan, Some("Plan the work".into()), window, cx)
    });
    wait_until(cx, "the task's answer", |cx| {
        read(&view, cx, |v| {
            !v.is_busy() && v.thread().messages.len() == 2
        })
    });
    assert_eq!(read(&view, cx, |v| v.thread().mode), Mode::Plan);
    assert_eq!(
        read(&view, cx, |v| v.thread().title.clone()),
        "Plan the work"
    );
    assert!(events.borrow().contains(&ChatEvent::ThreadsChanged));
    let first = read(&view, cx, |v| v.thread().id.clone());

    // The footer shows the host's branch; approvals persist.
    view.update(cx, |v, cx| {
        v.set_git_status(
            Some(GitStatus {
                branch: "feat/review-brief".into(),
                ahead: 1,
                behind: 0,
                dirty: 2,
            }),
            cx,
        );
        v.set_approval(Approval::AutoEdits, cx);
        cx.emit(ChatEvent::Git(GitRequest::Push));
    });
    cx.run_until_parked();
    assert_eq!(read(&view, cx, |v| v.approval()), Approval::AutoEdits);
    assert!(read(&view, cx, |v| v.mcp_status().is_empty()));
    assert!(events.borrow().contains(&ChatEvent::Git(GitRequest::Push)));

    // Rename from the ⋯ menu.
    events.borrow_mut().clear();
    view.update_in(cx, |v, window, cx| v.start_rename(window, cx));
    let input =
        read(&view, cx, |v| v.renaming.as_ref().map(|(i, _)| i.clone())).expect("title field");
    input.update(cx, |input, cx| input.set_text("Review brief", cx));
    view.update_in(cx, |v, window, cx| v.commit_rename(window, cx));
    wait_until(cx, "the renamed thread in the list", |cx| {
        read(&view, cx, |v| {
            v.threads.first().is_some_and(|t| t.title == "Review brief")
        })
    });
    assert!(events.borrow().contains(&ChatEvent::ThreadsChanged));

    // A new empty task, then back to the first thread.
    view.update_in(cx, |v, window, cx| {
        v.start_task(Mode::Ask, None, window, cx)
    });
    wait_until(cx, "the new thread", |cx| {
        read(&view, cx, |v| v.thread().id != first)
    });
    assert_eq!(read(&view, cx, |v| v.thread().mode), Mode::Ask);
    view.update_in(cx, |v, window, cx| v.open_thread(&first, window, cx));
    wait_until(cx, "the first thread", |cx| {
        read(&view, cx, |v| v.thread().id == first)
    });
    assert_eq!(
        read(&view, cx, |v| v.thread().title.clone()),
        "Review brief"
    );

    // Copy as Markdown, then delete it.
    view.update(cx, |v, cx| v.copy_markdown(cx));
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
    assert!(copied.is_some_and(|t| t.starts_with("# Review brief")));
    view.update(cx, |v, cx| v.delete_current_thread(cx));
    wait_until(cx, "the thread to be deleted", |cx| {
        read(&view, cx, |v| {
            v.threads.is_empty() && v.thread().id != first
        })
    });
    view.update_in(cx, |v, window, cx| v.open_settings_page(window, cx));
    cx.run_until_parked();
    assert!(read(&view, cx, |v| v.settings.is_some()));
    assert_eq!(mock.remaining(), 0);
}

#[gpui::test]
fn threads_keep_running_in_the_background_with_status_and_alerts(cx: &mut TestAppContext) {
    use ferrumagent_core::status::ThreadStatus;
    cx.executor().allow_parking();
    cx.update(|cx| {
        bind_text_input_keys(cx);
        ferrum_ui::init_for_tests(cx);
        crate::init(cx);
    });
    let project = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("tempdir");
    let root = project.path().canonicalize().expect("canonical");
    std::fs::write(root.join("a.txt"), "one\n").expect("write");
    let mock = MockProvider::new();
    mock.push_tool(
        "t1",
        "edit_file",
        json!({"path": "a.txt", "old_string": "one", "new_string": "ONE"}),
    );
    mock.push_text("B answer");
    mock.push_text("A done, all green.");
    let mut config = SessionConfig::isolated(&root, home.path());
    config.provider_override = Some(Arc::new(mock.clone()));
    let delivered: Rc<RefCell<Vec<(String, String, String)>>> = Rc::default();
    let clicks: Rc<RefCell<Vec<crate::ToastClick>>> = Rc::default();
    let notifier: crate::Notifier = {
        let (delivered, clicks) = (delivered.clone(), clicks.clone());
        Rc::new(move |event, title, body, click, _| {
            delivered
                .borrow_mut()
                .push((event.to_string(), title, body));
            clicks.borrow_mut().push(click);
        })
    };
    let options = ChatOptions {
        config: Some(config),
        notifier: Some(notifier),
        ..Default::default()
    };
    let (view, cx) =
        cx.add_window_view(|window, cx| ChatView::build(root.clone(), options, window, cx));
    cx.simulate_resize(size(px(360.), px(640.)));
    wait_until(cx, "the session", |cx| {
        read(&view, cx, |v| v.session().is_some())
    });
    view.update(cx, |v, cx| {
        v.window_active = true;
        v.set_visible(false, cx);
    });
    let status = |view: &Entity<ChatView>, cx: &mut VisualTestContext, id: &str| {
        read(view, cx, |v| v.statuses().status(id))
    };

    // Thread A asks for permission while the chat is hidden: Needs you,
    // notified once.
    let a = read(&view, cx, |v| v.thread().id.clone());
    view.update(cx, |v, _| {
        v.session().expect("session").send("Edit a", vec![])
    });
    wait_until(cx, "A's prompt", |cx| {
        read(&view, cx, |v| !v.open_permission_requests().is_empty())
    });
    assert_eq!(status(&view, cx, &a), ThreadStatus::NeedsYou);
    assert_eq!(
        delivered.borrow().clone(),
        [(
            crate::NEEDS_YOU_EVENT.to_string(),
            "Edit a needs you".to_string(),
            "Allow Edit a.txt?".to_string()
        )]
    );
    // Shown again: still waiting on the prompt.
    view.update(cx, |v, cx| v.set_visible(true, cx));
    assert_eq!(status(&view, cx, &a), ThreadStatus::NeedsYou);

    // A new thread: A keeps running in the background, not cancelled.
    view.update_in(cx, |v, window, cx| v.new_thread(&NewThread, window, cx));
    cx.run_until_parked();
    let b = read(&view, cx, |v| v.thread().id.clone());
    assert_ne!(a, b);
    assert_eq!(read(&view, cx, |v| v.background_threads()), vec![a.clone()]);
    assert!(read(&view, cx, |v| v.open_permission_requests().is_empty()));
    view.update(cx, |v, _| {
        v.session().expect("session").send("Hello B", vec![])
    });
    wait_until(cx, "B's answer", |cx| {
        read(&view, cx, |v| {
            !v.is_busy() && v.thread().messages.len() == 2
        })
    });
    assert_eq!(
        status(&view, cx, &b),
        ThreadStatus::Idle,
        "watched: no Done"
    );
    assert_eq!(delivered.borrow().len(), 1);

    // The grouped list renders: A under Needs you, B idle.
    view.update_in(cx, |v, window, cx| {
        v.toggle_threads(&ToggleThreads, window, cx)
    });
    cx.run_until_parked();
    let sections = read(&view, cx, |v| {
        v.statuses()
            .group(&v.threads, |t| t.id.as_str(), |t| t.updated)
            .into_iter()
            .map(|(s, rows)| (s, rows.len()))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        sections,
        [(ThreadStatus::NeedsYou, 1), (ThreadStatus::Idle, 1)]
    );
    view.update_in(cx, |v, window, cx| {
        v.toggle_threads(&ToggleThreads, window, cx)
    });

    // Back to A: its prompt is shown again; idle B is let go.
    view.update(cx, |v, cx| {
        v.switch_to(crate::live::Target::Open(a.clone()), cx)
    });
    cx.run_until_parked();
    assert_eq!(read(&view, cx, |v| v.thread().id.clone()), a);
    assert!(read(&view, cx, |v| v.background_threads().is_empty()));
    let request = read(&view, cx, |v| v.open_permission_requests().to_vec());
    assert_eq!(request.len(), 1);

    // Answered, then hidden: the turn ends unseen, Done and notified.
    view.update(cx, |v, cx| {
        v.respond(request[0].id, Decision::AllowOnce, cx)
    });
    view.update(cx, |v, cx| v.set_visible(false, cx));
    wait_until(cx, "A to finish", |cx| {
        read(&view, cx, |v| {
            !v.is_busy() && v.statuses().status(&a) == ThreadStatus::Done
        })
    });
    assert_eq!(
        delivered.borrow().last().cloned(),
        Some((
            crate::DONE_EVENT.to_string(),
            "Edit a finished".to_string(),
            "A done, all green.".to_string()
        ))
    );
    assert_eq!(delivered.borrow().len(), 2);
    assert_eq!(read(&view, cx, |v| v.pending_changes().len()), 1);
    // Looking at it clears Done.
    view.update(cx, |v, cx| v.set_visible(true, cx));
    assert_eq!(status(&view, cx, &a), ThreadStatus::Idle);
    assert_eq!(mock.remaining(), 0);

    // Clicking the notification shows its thread and asks the host to
    // reveal the chat.
    let revealed: Rc<RefCell<Vec<String>>> = Rc::default();
    cx.update({
        let (revealed, view) = (revealed.clone(), view.clone());
        move |_, cx| {
            cx.subscribe(&view, move |_, event: &ChatEvent, _| {
                if let ChatEvent::Reveal { thread_id } = event {
                    revealed.borrow_mut().push(thread_id.clone());
                }
            })
            .detach();
        }
    });
    view.update_in(cx, |v, window, cx| v.new_thread(&NewThread, window, cx));
    cx.run_until_parked();
    assert_ne!(read(&view, cx, |v| v.thread().id.clone()), a);
    let click = clicks.borrow().last().cloned().expect("a click action");
    cx.update(|window, cx| click(window, cx));
    cx.run_until_parked();
    assert_eq!(read(&view, cx, |v| v.thread().id.clone()), a);
    assert_eq!(*revealed.borrow(), vec![a.clone()]);
}

#[test]
fn failed_turns_notify_as_their_own_event() {
    use ferrumagent_core::status::Alert;
    let finished = |error| Alert::Finished {
        summary: "s".into(),
        error,
    };
    assert_eq!(
        crate::live::notification("Fix", finished(true)),
        (crate::FAILED_EVENT, "Fix failed".into(), "s".into())
    );
    assert_eq!(
        crate::live::notification("Fix", finished(false)).0,
        crate::DONE_EVENT
    );
    let needs = Alert::NeedsYou {
        summary: "q".into(),
    };
    assert_eq!(
        crate::live::notification("Fix", needs).0,
        crate::NEEDS_YOU_EVENT
    );
    assert_eq!(ferrum_ui::notify::sound_for(crate::FAILED_EVENT), "Basso");
    assert_eq!(ferrum_ui::notify::sound_for(crate::NEEDS_YOU_EVENT), "Ping");
    assert_eq!(ferrum_ui::notify::sound_for(crate::DONE_EVENT), "Glass");
}

/// The "Opening project" spinner always ends: an open that never answers
/// (a git snapshot stuck on a huge untracked folder) turns into an error
/// that says what to do, and Try again opens the chat.
#[gpui::test]
fn an_open_that_never_answers_ends_in_an_error(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        ferrum_ui::init_for_tests(cx);
        crate::init(cx);
    });
    let project = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("tempdir");
    let root = project.path().canonicalize().expect("canonical");
    let options = ChatOptions {
        config: Some(SessionConfig::isolated(&root, home.path())),
        ..Default::default()
    };
    let opens = Rc::new(std::cell::Cell::new(0));
    let opener: crate::opening::Opener = Rc::new({
        let opens = opens.clone();
        move |config| {
            opens.set(opens.get() + 1);
            if opens.get() == 1 {
                Box::pin(futures::future::pending())
            } else {
                (crate::opening::default_opener())(config)
            }
        }
    });
    let (view, cx) = cx.add_window_view(|window, cx| {
        ChatView::build_with(root.clone(), options, opener, window, cx)
    });
    cx.run_until_parked();
    assert!(read(&view, cx, |v| v.session().is_none()
        && v.load_error.is_none()));

    cx.executor()
        .advance_clock(crate::opening::OPEN_TIMEOUT + Duration::from_secs(1));
    cx.run_until_parked();
    let error = read(&view, cx, |v| v.load_error.clone()).expect("an error, not a spinner");
    assert!(error.contains(".gitignore"), "{error}");
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();

    view.update(cx, |v, cx| v.open_session(cx));
    wait_until(cx, "the session", |cx| {
        read(&view, cx, |v| v.session().is_some())
    });
    assert_eq!(read(&view, cx, |v| v.load_error.clone()), None);
    assert_eq!(opens.get(), 2);
}

/// A project folder that is gone: an error with the path, not a spinner.
#[gpui::test]
fn a_missing_project_folder_is_an_error(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        ferrum_ui::init_for_tests(cx);
        crate::init(cx);
    });
    let home = tempfile::tempdir().expect("tempdir");
    let root = home.path().join("gone");
    let options = ChatOptions {
        config: Some(SessionConfig::isolated(&root, home.path())),
        ..Default::default()
    };
    let (view, cx) =
        cx.add_window_view(|window, cx| ChatView::build(root.clone(), options, window, cx));
    wait_until(cx, "the error", |cx| {
        read(&view, cx, |v| v.load_error.is_some())
    });
    let error = read(&view, cx, |v| v.load_error.clone()).unwrap_or_default();
    assert!(error.contains("gone"), "{error}");
    assert!(read(&view, cx, |v| v.session().is_none()));
}
