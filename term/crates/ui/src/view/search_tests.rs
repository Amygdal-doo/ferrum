//! Search Everywhere's terminal items and the scrollback search.

use super::search::{Pane, panes, search_text, terminal_items};
use ferrum_ui::search::{SearchCategory, TextMatcher, TextQuery};
use ferrumterm_protocol::{
    BlockId, BlockInfo, Layout, SessionId, SessionInfo, WindowId, WindowInfo,
};

fn block(id: u32, label: Option<&str>, title: &str, cwd: &str) -> BlockInfo {
    BlockInfo {
        id: BlockId(id),
        label: label.map(str::to_owned),
        title: title.into(),
        command: vec!["zsh".into()],
        cwd: Some(cwd.into()),
        pid: None,
        cols: 80,
        rows: 24,
        status: None,
        exited: None,
        agent: None,
    }
}

fn session(id: u32, name: &str, blocks: Vec<BlockInfo>) -> SessionInfo {
    SessionInfo {
        id: SessionId(id),
        name: name.into(),
        created_ms: 0,
        active_window: WindowId(1),
        windows: vec![WindowInfo {
            id: WindowId(1),
            name: "1".into(),
            layout: Layout::Block(blocks[0].id),
            focused: blocks[0].id,
            zoomed: false,
            cols: 80,
            rows: 24,
            blocks,
        }],
    }
}

#[test]
fn every_pane_is_an_item_named_like_the_sidebar() {
    let sessions = vec![
        session(
            1,
            "work",
            vec![
                block(1, Some("server"), "", "/tmp/a"),
                block(2, None, "vim", "/tmp/b"),
            ],
        ),
        session(2, "play", vec![block(3, None, "", "/tmp/c")]),
    ];
    let items = terminal_items(&sessions);
    let titles: Vec<_> = items.iter().map(|i| i.title.to_string()).collect();
    assert_eq!(titles, ["server", "vim", "zsh"]);
    assert!(items.iter().all(|i| i.category == SearchCategory::Items));
    assert_eq!(items[0].id.as_ref(), "block:s1:b1");
    // Several sessions: the session name leads the detail.
    assert_eq!(items[2].detail.as_ref(), "play · /tmp/c");
    assert_eq!(panes(&sessions).len(), 3);
}

#[test]
fn scrollback_hits_point_at_their_pane() {
    let pane = Pane {
        block: BlockId(7),
        label: "server".into(),
    };
    let matcher = TextMatcher::new(&TextQuery {
        text: "error".into(),
        ..TextQuery::default()
    })
    .expect("matcher");
    let mut hits = Vec::new();
    assert!(search_text(
        &matcher,
        "ok\nERROR: boom\nfine\n",
        &pane,
        10,
        &mut hits
    ));
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source.as_ref(), "b7");
    assert_eq!(hits[0].source_label.as_ref(), "server");
    assert_eq!(hits[0].line, 1);
}
