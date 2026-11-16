//! A chat built into an app without a project (FerrumData,
//! FerrumContainers, FerrumSystem): it opens at once — no "Opening
//! project" spinner — reads what the user is looking at before each
//! message, and shows in an [`AgentPane`] behind the Agent toggle.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ferrumagent_core::host::AppHost;
use ferrumagent_core::provider::mock::MockProvider;
use ferrumagent_core::session::SessionConfig;
use gpui::{Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext, Window};

use crate::pane::AgentPane;
use crate::view::{ChatOptions, ChatView};

fn init(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        ferrum_ui::init_for_tests(cx);
        crate::init(cx);
    });
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

#[gpui::test]
fn a_chat_without_a_project_opens_at_once_and_sends_the_apps_context(cx: &mut TestAppContext) {
    init(cx);
    let home = tempfile::tempdir().expect("tempdir");
    let mock = MockProvider::new();
    mock.push_text("Restart it.");
    let mut config = SessionConfig::isolated(home.path(), home.path());
    config.provider_override = Some(Arc::new(mock.clone()));
    let asked = Rc::new(Cell::new(0));
    let options = ChatOptions {
        config: Some(config),
        host: Some(AppHost::new("FerrumContainers", "A container manager.")),
        context: Some(Rc::new({
            let asked = asked.clone();
            move |_| {
                asked.set(asked.get() + 1);
                "Section: Containers\nweb (exited 137)".to_string()
            }
        })),
        ..Default::default()
    };
    let (view, cx) =
        cx.add_window_view(|window, cx| ChatView::build(PathBuf::new(), options, window, cx));
    // Opening takes no snapshot: the session is there after one pass.
    wait_until(cx, "the session", |cx| {
        view.read_with(cx, |v, _| v.session().is_some())
    });
    view.read_with(cx, |v, _| {
        assert!(!v.has_project());
        assert!(v.load_error.is_none());
        assert!(v.project.ends_with("apps/ferrumcontainers"));
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert_eq!(asked.get(), 0, "the context is read when a message is sent");

    let composer = view.read_with(cx, |v, _| v.composer.clone());
    composer.update(cx, |input, cx| input.set_text("Why did web stop?", cx));
    view.update_in(cx, |v, window, cx| v.send(&crate::Send, window, cx));
    wait_until(cx, "the answer", |cx| {
        view.read_with(cx, |v, _| !v.is_busy() && v.thread().messages.len() == 2)
    });
    assert_eq!(asked.get(), 1);
    let system = mock.requests()[0].system.clone();
    assert!(system.contains("built into FerrumContainers"), "{system}");
    assert!(system.contains("web (exited 137)"), "{system}");
}

/// A host window with the Agent pane.
struct Host {
    pane: AgentPane,
    home: PathBuf,
    made: usize,
}

impl Host {
    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let home = self.home.clone();
        let made = &mut self.made;
        self.pane.toggle(window, cx, |window, cx| {
            *made += 1;
            let options = ChatOptions {
                config: Some(SessionConfig::isolated(&home, &home)),
                ..AgentPane::options(None)
            };
            ChatView::without_project(AppHost::new("FerrumData", ""), options, window, cx)
        });
        cx.notify();
    }
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use ferrum_ui::prelude::*;
        h_flex()
            .size_full()
            .child(
                crate::agent_toggle("toggle-agent", self.pane.is_shown())
                    .on_click(cx.listener(|this, _, window, cx| this.toggle(window, cx))),
            )
            .children(self.pane.card(cx))
    }
}

#[gpui::test]
fn the_agent_pane_opens_hides_and_keeps_its_chat(cx: &mut TestAppContext) {
    init(cx);
    let home = tempfile::tempdir().expect("tempdir");
    let (host, cx): (Entity<Host>, _) = cx.add_window_view(|_, _| Host {
        pane: AgentPane::new(),
        home: home.path().to_path_buf(),
        made: 0,
    });
    host.update_in(cx, |h, window, cx| h.toggle(window, cx));
    cx.run_until_parked();
    let chat = host.read_with(cx, |h, _| {
        assert!(h.pane.is_shown());
        h.pane.chat().cloned().expect("a chat")
    });
    wait_until(cx, "the session", |cx| {
        chat.read_with(cx, |c, _| c.session().is_some())
    });
    assert!(chat.read_with(cx, |c, _| c.is_visible()));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();

    host.update_in(cx, |h, window, cx| h.toggle(window, cx));
    host.read_with(cx, |h, cx| {
        assert!(!h.pane.is_shown());
        assert!(h.pane.card(cx).is_none());
    });
    assert!(!chat.read_with(cx, |c, _| c.is_visible()));

    host.update_in(cx, |h, window, cx| h.toggle(window, cx));
    host.read_with(cx, |h, _| {
        assert!(h.pane.is_shown());
        assert_eq!(h.made, 1, "the chat is kept while hidden");
    });
}
