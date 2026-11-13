//! A host app embedding the terminal: headless GPUI window, private
//! in-process server (no shared `fterm` server is touched), real PTYs.

use ferrum_ui::gpui::{
    self, Context, Entity, Focusable as _, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, TestAppContext, VisualTestContext, Window, div, px, size,
};
use ferrumterm_ui::{Backend, CommandId, TermView, TermViewEvent, TermViewOptions};
use std::time::{Duration, Instant};

/// A host with the terminal as its only pane, recording the events.
struct Host {
    term: Entity<TermView>,
    events: Vec<TermViewEvent>,
    _events: Subscription,
}

impl Host {
    fn new(options: TermViewOptions, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let term = TermView::new(options, window, cx);
        let events = cx.subscribe(&term, |host: &mut Self, _, event: &TermViewEvent, _| {
            host.events.push(event.clone());
        });
        Self {
            term,
            events: Vec::new(),
            _events: events,
        }
    }
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.term.clone())
    }
}

/// Lets the tokio side run, redraws, and re-checks `done` until it holds.
fn wait_for(
    host: &Entity<Host>,
    cx: &mut VisualTestContext,
    what: &str,
    done: impl Fn(&Host, &gpui::App) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        if cx.read(|cx| done(host.read(cx), cx)) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[gpui::test]
fn an_embedded_view_connects_renders_and_runs_commands(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        ferrum_ui::init_for_tests(cx);
        ferrumterm_ui::init(cx);
    });
    let cwd = std::env::temp_dir();
    let options = TermViewOptions::embedded()
        .cwd(&cwd)
        .session_name("embed-test")
        .attach_to_server(false);
    let (host, cx) = cx.add_window_view(|window, cx| Host::new(options, window, cx));
    // The narrowest supported pane.
    cx.simulate_resize(size(px(320.), px(240.)));

    wait_for(&host, cx, "a session and a laid-out grid", |host, cx| {
        let term = host.term.read(cx);
        term.backend() == Some(Backend::Local)
            && term.tab_count() == 1
            && term.visible_grid().is_some()
    });
    let term = host.read_with(cx, |host, _| host.term.clone());
    let (cols, rows) = term
        .read_with(cx, |term, _| term.visible_grid())
        .expect("grid");
    assert!(cols >= 20 && rows >= 4, "{cols}x{rows} at 320x240");
    assert!(term.read_with(cx, |term, _| term.title().contains("embed-test")));
    assert!(host.read_with(cx, |host, _| {
        host.events
            .contains(&TermViewEvent::Connected(Backend::Local))
    }));

    // An agent-style command: its own tab, exit code reported.
    let id = term.update(cx, |term, cx| term.run_command("sh -c 'exit 3'", None, cx));
    assert_eq!(id, CommandId(1));
    wait_for(&host, cx, "the command to finish", |host, _| {
        host.events.iter().any(|e| {
            matches!(
                e,
                TermViewEvent::CommandFinished {
                    command: Some(c),
                    exit_code: 3,
                    ..
                } if *c == id
            )
        })
    });
    assert!(
        host.read_with(cx, |host, _| host.events.iter().any(|e| matches!(
            e,
            TermViewEvent::CommandStarted { command, .. } if *command == id
        )))
    );
    wait_for(&host, cx, "the command's tab", |host, cx| {
        host.term.read(cx).tab_count() == 2
    });

    // Splits and a sidebar toggle still lay out at 320 px.
    term.update(cx, |term, cx| {
        term.split(gpui::Axis::Horizontal, cx);
        term.set_sidebar_visible(true, cx);
    });
    wait_for(&host, cx, "the sidebar", |host, cx| {
        host.term.read(cx).sidebar_visible()
    });
    cx.update(|window, cx| {
        term.update(cx, |term, cx| term.focus_active(window, cx));
        let handle = term.read(cx).focus_handle(cx);
        assert!(handle.is_focused(window));
    });
}
