//! The find bar on a real pane: headless GPUI window, private in-process
//! server, a real PTY with a long scrollback.

use super::TermView;
use crate::find::Step;
use crate::{Find, TermViewOptions};
use ferrum_ui::gpui::{
    Context, Entity, IntoElement, ParentElement as _, Render, Styled as _, TestAppContext,
    VisualTestContext, Window, div, px, size,
};
use std::time::{Duration, Instant};

struct Host {
    term: Entity<TermView>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.term.clone())
    }
}

fn wait_for(
    term: &Entity<TermView>,
    cx: &mut VisualTestContext,
    what: &str,
    done: impl Fn(&TermView) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        if term.read_with(cx, |t, _| done(t)) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        cx.executor().advance_clock(Duration::from_millis(100));
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// The current match is highlighted on the focused pane's screen.
fn current_on_screen(t: &TermView) -> bool {
    let (Some(bar), Some(block)) = (&t.find, t.app.focused()) else {
        return false;
    };
    let Some(frame) = t.app.frames.get(&block) else {
        return false;
    };
    (0..frame.rows).any(|row| {
        bar.search
            .row_highlights(frame, row, frame.cols)
            .iter()
            .any(|(_, _, current)| *current)
    })
}

#[gpui::test]
fn find_highlights_counts_and_scrolls_to_matches(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        ferrum_ui::init_for_tests(cx);
        crate::init(cx);
    });
    let options = TermViewOptions {
        show_sidebar: false,
        ..TermViewOptions::default()
    }
    .cwd(std::env::temp_dir())
    .session_name("find-test")
    .attach_to_server(false);
    let (host, cx) = cx.add_window_view(|window, cx| Host {
        term: TermView::new(options, window, cx),
    });
    cx.simulate_resize(size(px(900.), px(500.)));
    let term = host.read_with(cx, |host, _| host.term.clone());
    wait_for(&term, cx, "a session and a grid", |t| {
        t.tab_count() == 1 && t.visible_grid().is_some()
    });
    // Its own tab (now the focused pane): a needle, 300 lines, a needle.
    term.update(cx, |t, cx| {
        t.run_command(
            "sh -c 'echo Needle-1; i=0; while [ $i -lt 300 ]; do echo line-$i; i=$((i+1)); done; echo needle-2'",
            None,
            cx,
        );
    });
    wait_for(&term, cx, "the output", |t| {
        t.tab_count() == 2
            && t.app
                .focused()
                .and_then(|b| t.app.frames.get(&b))
                .is_some_and(|f| (0..f.rows as usize).any(|r| f.line_text(r) == "needle-2"))
    });

    cx.update(|window, cx| {
        term.update(cx, |t, cx| t.focus_active(window, cx));
        window.dispatch_action(Box::new(Find), cx);
    });
    let input = term.read_with(cx, |t, _| t.find.as_ref().expect("the bar").input.clone());
    cx.update(|window, cx| {
        assert!(
            term.read(cx)
                .find
                .as_ref()
                .expect("bar")
                .focused(window, cx)
        );
        input.update(cx, |input, cx| input.set_text("needle", cx));
        term.update(cx, |t, cx| t.search_now(true, cx));
    });
    wait_for(&term, cx, "two matches", |t| {
        t.find
            .as_ref()
            .is_some_and(|b| b.search.label() == "2 of 2")
    });
    assert!(
        term.read_with(cx, |t, _| current_on_screen(t)),
        "the newest is on screen"
    );

    // ↩ goes up to the first needle, 300 lines back: the pane scrolls there.
    term.update(cx, |t, cx| t.find_step(Step::Older, cx));
    wait_for(&term, cx, "the scroll to the first needle", |t| {
        t.find
            .as_ref()
            .is_some_and(|b| b.search.label() == "1 of 2")
            && current_on_screen(t)
    });
    let scrolled = term.read_with(cx, |t, _| {
        t.app
            .focused()
            .and_then(|b| t.app.frames.get(&b))
            .map(|f| f.display_offset)
    });
    assert!(scrolled.is_some_and(|o| o > 0), "{scrolled:?}");

    // Case sensitive: not "Needle-1"; a bad regex says why.
    term.update(cx, |t, cx| t.toggle_find_option(false, cx));
    wait_for(&term, cx, "one case-sensitive match", |t| {
        t.find
            .as_ref()
            .is_some_and(|b| b.search.label() == "1 of 1")
    });
    cx.update(|_, cx| input.update(cx, |input, cx| input.set_text("needle-(", cx)));
    term.update(cx, |t, cx| t.toggle_find_option(true, cx));
    wait_for(&term, cx, "the regex error", |t| {
        t.find.as_ref().is_some_and(|b| b.error.is_some())
    });

    cx.update(|window, cx| term.update(cx, |t, cx| t.close_find(window, cx)));
    assert!(term.read_with(cx, |t, _| t.find.is_none()));
}
