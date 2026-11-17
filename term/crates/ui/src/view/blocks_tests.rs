//! Command blocks on a real zsh with FerrumTerm's shell integration:
//! headless GPUI window, private in-process server, real PTY.

use super::TermView;
use crate::TermViewOptions;
use crate::blocks::Jump;
use ferrum_ui::gpui::{
    Context, Entity, IntoElement, ParentElement as _, Render, Styled as _, TestAppContext,
    VisualTestContext, Window, div, px, size,
};
use ferrumterm_protocol::{BlockId, CommandMark, Input, Request, Spawn};
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

fn commands(t: &TermView) -> Vec<CommandMark> {
    t.app
        .focused()
        .and_then(|b| t.app.frames.get(&b))
        .map(|f| f.commands.clone())
        .unwrap_or_default()
}

fn typed(t: &mut TermView, block: BlockId, line: &str) {
    t.app.send(Request::Input {
        target: Some(block.to_string()),
        input: vec![Input::Text(line.into()), Input::Key("Enter".into())],
    });
}

#[gpui::test]
fn commands_fold_copy_run_again_and_jump(cx: &mut TestAppContext) {
    if !std::path::Path::new("/bin/zsh").exists() {
        return;
    }
    cx.executor().allow_parking();
    cx.update(|cx| {
        ferrum_ui::init_for_tests(cx);
        crate::init(cx);
    });
    let home = tempfile::tempdir().expect("tempdir");
    std::fs::write(home.path().join(".zshrc"), "PS1='%# '\n").expect("zshrc");
    let options = TermViewOptions {
        show_sidebar: false,
        ..TermViewOptions::default()
    }
    .cwd(home.path())
    .session_name("blocks-test")
    .attach_to_server(false);
    let (host, cx) = cx.add_window_view(|window, cx| Host {
        term: TermView::new(options, window, cx),
    });
    cx.simulate_resize(size(px(900.), px(600.)));
    let term = host.read_with(cx, |host, _| host.term.clone());
    wait_for(&term, cx, "a session and a grid", |t| {
        t.tab_count() == 1 && t.visible_grid().is_some()
    });
    // A tab with an interactive zsh in an empty home: only our integration.
    term.update(cx, |t, _| {
        let session = t.app.session().map(|s| s.id.to_string());
        t.app.send(Request::NewWindow {
            session,
            name: Some("zsh".into()),
            spawn: Spawn {
                command: vec!["/bin/zsh".into(), "-i".into()],
                env: vec![("HOME".into(), home.path().display().to_string())],
                ..Spawn::default()
            },
        });
    });
    // The zsh tab shown (not the first tab's login shell, which may have
    // the integration too) and its first prompt marked.
    wait_for(&term, cx, "the zsh prompt", |t| {
        t.tab_count() == 2
            && t.app.window().is_some_and(|w| w.name == "zsh")
            && commands(t).len() == 1
    });
    let block = term.read_with(cx, |t, _| t.app.focused().expect("focused"));
    term.update(cx, |t, _| {
        typed(t, block, "echo one; false");
        typed(t, block, "printf 'a\\nb\\nc\\n'");
    });
    wait_for(&term, cx, "two finished commands", |t| {
        let c = commands(t);
        c.len() == 3 && c[1].exit.is_some()
    });
    let marks = term.read_with(cx, |t, _| commands(t));
    assert_eq!(
        (marks[0].command.as_str(), marks[0].exit, marks[0].output),
        ("echo one; false", Some(1), Some(1))
    );
    assert_eq!(
        (marks[1].command.as_str(), marks[1].exit),
        ("printf 'a\\nb\\nc\\n'", Some(0))
    );
    let printf = marks[1].clone();
    let output_rows = printf.bottom - printf.output.expect("output") + 1;
    assert_eq!(output_rows, 3);

    // The pointer over its output: its toolbar.
    let (x, y) = term.read_with(cx, |t, _| {
        let &(_, x, y, _, _) = t.app.panes.iter().find(|p| p.0 == block).expect("pane");
        let cell = f32::from(t.metrics.expect("metrics").cell_height);
        (x + 20., y + cell * (f32::from(printf.bottom) + 0.5))
    });
    term.update(cx, |t, _| {
        assert!(t.hover_command(x, y));
        assert_eq!(t.hovered_command, Some((block, printf.id)));
    });

    // Collapsed: its output rows are left out of the screen.
    term.update(cx, |t, cx| t.toggle_collapsed(block, printf.id, cx));
    wait_for(&term, cx, "the output folded", |t| {
        t.row_maps
            .get(&block)
            .is_some_and(|rows| !rows.contains(&printf.bottom) && rows.contains(&printf.top))
    });
    term.update(cx, |t, cx| t.toggle_collapsed(block, printf.id, cx));

    // Copy its output (from the server), then run it again.
    term.update(cx, |t, _| t.copy_output(block, printf.id));
    let mut copied = None;
    for _ in 0..100 {
        cx.run_until_parked();
        copied = cx.read_from_clipboard().and_then(|c| c.text());
        if copied.as_deref() == Some("a\nb\nc") {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(copied.as_deref(), Some("a\nb\nc"));
    term.update(cx, |t, cx| t.rerun(block, printf.id, cx));
    wait_for(&term, cx, "the command run again", |t| {
        let c = commands(t);
        c.iter()
            .filter(|m| m.command == printf.command && m.exit == Some(0))
            .count()
            == 2
    });

    // A long output: ⌘↑ scrolls back to where it starts, ⌘↓ to the bottom.
    term.update(cx, |t, _| typed(t, block, "seq 1 80"));
    wait_for(&term, cx, "seq to finish", |t| {
        commands(t)
            .iter()
            .any(|m| m.command == "seq 1 80" && m.exit == Some(0))
    });
    let offset = |t: &TermView| t.app.frames.get(&block).map_or(0, |f| f.display_offset);
    term.update(cx, |t, cx| t.jump_command(Jump::Previous, cx));
    wait_for(&term, cx, "the scroll to seq", |t| {
        let top = commands(t).first().cloned();
        offset(t) > 0
            && top.is_some_and(|m| m.command == "seq 1 80" && m.starts && m.top == 0)
            && t.selected_command.is_some()
    });
    term.update(cx, |t, cx| t.jump_command(Jump::Next, cx));
    wait_for(&term, cx, "the scroll back down", |t| offset(t) == 0);
}
