//! App state without a server: which session a view shows, queued API
//! calls, and which events concern it.

use crate::app::App;
use crate::options::TermViewOptions;
use ferrumterm_protocol::{
    BlockId, BlockInfo, Layout, SessionId, SessionInfo, WindowId, WindowInfo,
};
use std::cell::Cell;
use std::rc::Rc;

fn block(id: u32) -> BlockInfo {
    BlockInfo {
        id: BlockId(id),
        label: None,
        title: String::new(),
        command: Vec::new(),
        cwd: None,
        pid: None,
        cols: 80,
        rows: 24,
        status: None,
        exited: None,
        agent: None,
    }
}

fn session(id: u32, name: &str, blocks: &[u32]) -> SessionInfo {
    SessionInfo {
        id: SessionId(id),
        name: name.into(),
        created_ms: u64::from(id),
        active_window: WindowId(id),
        windows: vec![WindowInfo {
            id: WindowId(id),
            name: "1".into(),
            layout: Layout::Block(BlockId(blocks[0])),
            focused: BlockId(blocks[0]),
            zoomed: false,
            cols: 80,
            rows: 24,
            blocks: blocks.iter().map(|b| block(*b)).collect(),
        }],
    }
}

fn app(options: &TermViewOptions) -> App {
    let (proxy, _events) = futures::channel::mpsc::unbounded();
    App::new(options, proxy)
}

#[test]
fn a_named_session_is_shown_instead_of_the_first() {
    let mut a = app(&TermViewOptions::embedded().session_name("editor:app"));
    a.set_sessions(vec![
        session(1, "main", &[1]),
        session(2, "editor:app", &[2]),
    ]);
    assert_eq!(a.session().map(|s| s.name.as_str()), Some("editor:app"));
    assert!(a.shows_block(BlockId(2)));
    assert!(
        !a.shows_block(BlockId(1)),
        "other sessions' events are not ours"
    );

    let mut plain = app(&TermViewOptions::default());
    plain.set_sessions(vec![session(1, "main", &[1]), session(2, "x", &[2])]);
    assert_eq!(plain.session().map(|s| s.name.as_str()), Some("main"));
    assert!(plain.shows_block(BlockId(2)));
}

#[test]
fn a_missing_named_session_is_not_replaced_by_another() {
    let mut a = app(&TermViewOptions::embedded().session_name("editor:app"));
    a.set_sessions(vec![session(1, "main", &[1])]);
    assert!(a.session().is_none(), "waits for its own session");
}

#[test]
fn api_calls_wait_for_a_connection_and_a_session() {
    let mut a = app(&TermViewOptions::embedded());
    let ran = Rc::new(Cell::new(0));
    let r = ran.clone();
    a.when_ready(move |_| r.set(r.get() + 1));
    a.set_sessions(vec![session(1, "main", &[1])]);
    assert_eq!(ran.get(), 0, "not connected yet");
    assert!(a.session().is_some());
}

#[test]
fn compact_views_use_a_smaller_font_and_the_given_directory() {
    let a = app(&TermViewOptions::embedded().cwd("/code/app"));
    assert!(a.font_pt < crate::app::DEFAULT_FONT_PT);
    assert_eq!(a.spawn_here().cwd.as_deref(), Some("/code/app"));
    assert!(
        !a.ui.sidebar_open,
        "embedded views start with the sidebar closed"
    );
    let b = app(&TermViewOptions::default().show_sidebar(true));
    assert_eq!(b.spawn_here().cwd, None);
    assert!(b.ui.sidebar_open);
}

#[test]
fn command_ids_are_unique_and_early_exits_are_bounded() {
    let mut a = app(&TermViewOptions::embedded());
    let first = a.next_command_id();
    assert_ne!(first, a.next_command_id());
    for i in 0..200 {
        a.remember_exit(BlockId(i), 0);
    }
    assert!(a.early_exits.len() <= 64);
    assert_eq!(a.early_exits.last(), Some(&(BlockId(199), 0)));
}
