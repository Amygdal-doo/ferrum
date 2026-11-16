//! The Agent pane beside the terminal: headless GPUI window, private
//! in-process server, real PTYs, and a chat whose settings and threads
//! live in a temporary folder.

use ferrum_ui::gpui::{
    Context, Entity, IntoElement, ParentElement as _, Render, Styled as _, TestAppContext,
    VisualTestContext, Window, div, px, size,
};
use ferrumterm_ui::{TermView, TermViewOptions};
use serde_json::json;
use std::time::{Duration, Instant};

struct Host {
    term: Entity<TermView>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.term.clone())
    }
}

/// Lets the tokio side and the clock run, redraws, until `done` holds.
fn wait_for(
    cx: &mut VisualTestContext,
    what: &str,
    mut done: impl FnMut(&mut VisualTestContext) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        cx.executor().advance_clock(Duration::from_millis(100));
        std::thread::sleep(Duration::from_millis(60));
    }
}

#[gpui::test]
fn the_agent_pane_knows_the_focused_pane_and_runs_commands_in_it(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        ferrum_ui::init_for_tests(cx);
        ferrumterm_ui::init(cx);
        ferrumagent_ui::init(cx);
    });
    let folder = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("tempdir");
    let options = TermViewOptions {
        show_sidebar: false,
        ..TermViewOptions::default()
    }
    .cwd(folder.path())
    .session_name("agent-test")
    .attach_to_server(false)
    .agent(Some(home.path().to_path_buf()));
    let (host, cx) = cx.add_window_view(|window, cx| Host {
        term: TermView::new(options, window, cx),
    });
    cx.simulate_resize(size(px(1200.), px(700.)));
    let term = host.read_with(cx, |host, _| host.term.clone());
    wait_for(cx, "a session and a grid", |cx| {
        term.read_with(cx, |t, _| t.tab_count() == 1 && t.visible_grid().is_some())
    });

    // The toggle opens a chat without a project: the folder is no repository.
    term.update_in(cx, |t, window, cx| t.toggle_agent(window, cx));
    let chat = term.read_with(cx, |t, _| {
        assert!(t.agent_shown());
        t.agent_chat().expect("a chat")
    });
    wait_for(cx, "the chat to open", |cx| {
        chat.read_with(cx, |c, _| c.session().is_some())
    });
    assert!(!chat.read_with(cx, |c, _| c.has_project()));

    // terminal_run types into the focused pane (once the shell is up); its
    // answer is the screen.
    wait_for(cx, "the shell's prompt", |cx| {
        term.read_with(cx, |t, _| t.agent_context().contains("Its screen"))
    });
    let tools = term.read_with(cx, |t, _| t.agent_tools());
    let run = tools
        .iter()
        .find(|t| t.def().spec.name == "terminal_run")
        .expect("terminal_run")
        .clone();
    let call = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(run.call(json!({"command": "echo agent-$((40+2))"})))
    });
    wait_for(cx, "the command's answer", |_| call.is_finished());
    let out = call.join().expect("call");
    assert!(!out.is_error, "{}", out.text);
    assert!(out.text.contains("agent-42"), "{}", out.text);

    let context = term.read_with(cx, |t, _| t.agent_context());
    assert!(context.contains("Session: agent-test"), "{context}");
    assert!(context.contains("Focused pane:"), "{context}");
    assert!(context.contains("agent-42"), "{context}");

    term.update_in(cx, |t, window, cx| t.toggle_agent(window, cx));
    assert!(!term.read_with(cx, |t, _| t.agent_shown()));
    assert!(!chat.read_with(cx, |c, _| c.is_visible()));
}
