//! Request handlers that change sessions, windows and blocks.

use crate::block::{Block, SpawnContext, lock};
use crate::state::{DEFAULT_COLS, DEFAULT_ROWS, Session, State, Target, Window};
use crate::{Server, now_ms};
use ferrumterm_protocol::{
    BlockId, Direction, Event, Layout, Response, SessionId, Spawn, WindowId,
};
use std::sync::Arc;

fn spawn_block(
    server: &Server,
    id: BlockId,
    spawn: &Spawn,
    cols: u16,
    rows: u16,
    session: &str,
    window: &str,
) -> Result<Arc<Block>, String> {
    let socket = server.config.socket.display().to_string();
    let ctx = SpawnContext {
        session,
        window,
        socket: &socket,
        wake: &server.wake,
    };
    Block::spawn(id, spawn, cols, rows, &ctx, server.internal.clone())
}

pub fn new_session(server: &Server, name: Option<String>, spawn: &Spawn) -> Response {
    let (sid, wid, bid, name) = {
        let mut st = lock(&server.state);
        let name = name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| st.default_session_name());
        if st.sessions.iter().any(|s| s.name == name) {
            return Response::Error {
                message: format!("a session named “{name}” already exists"),
            };
        }
        (
            SessionId(st.next_id()),
            WindowId(st.next_id()),
            BlockId(st.next_id()),
            name,
        )
    };
    let block = match spawn_block(
        server,
        bid,
        spawn,
        DEFAULT_COLS,
        DEFAULT_ROWS,
        &sid.to_string(),
        &wid.to_string(),
    ) {
        Ok(b) => b,
        Err(message) => return Response::Error { message },
    };
    {
        let mut st = lock(&server.state);
        st.blocks.insert(bid, block);
        st.sessions.push(Session {
            id: sid,
            name,
            created_ms: now_ms(),
            windows: vec![Window {
                id: wid,
                name: "1".into(),
                layout: Layout::Block(bid),
                focused: bid,
                cols: DEFAULT_COLS,
                rows: DEFAULT_ROWS,
            }],
            active: wid,
        });
        st.last_focus = Some(bid);
    }
    server.emit(Event::Changed);
    Response::Created {
        session: sid,
        window: wid,
        block: bid,
    }
}

pub fn new_window(
    server: &Server,
    session: Option<&str>,
    name: Option<String>,
    spawn: &Spawn,
) -> Response {
    let (sid, wid, bid, name, cols, rows) = {
        let mut st = lock(&server.state);
        let sid = match session {
            Some(t) => match st.resolve(t) {
                Some(Target::Session(s) | Target::Window(s, _) | Target::Block(s, _, _)) => s,
                None => {
                    return Response::Error {
                        message: format!("no session matches “{t}”"),
                    };
                }
            },
            None => match st
                .last_focus
                .and_then(|b| st.locate(b))
                .map(|(s, _)| s)
                .or_else(|| st.sessions.first().map(|s| s.id))
            {
                Some(s) => s,
                None => {
                    drop(st);
                    return new_session(server, None, spawn);
                }
            },
        };
        let Some(sess) = st.session(sid) else {
            return Response::Error {
                message: "session vanished".into(),
            };
        };
        let (cols, rows) = sess
            .windows
            .first()
            .map_or((DEFAULT_COLS, DEFAULT_ROWS), |w| (w.cols, w.rows));
        let name = name.unwrap_or_else(|| (sess.windows.len() + 1).to_string());
        (
            sid,
            WindowId(st.next_id()),
            BlockId(st.next_id()),
            name,
            cols,
            rows,
        )
    };
    let block = match spawn_block(
        server,
        bid,
        spawn,
        cols,
        rows,
        &sid.to_string(),
        &wid.to_string(),
    ) {
        Ok(b) => b,
        Err(message) => return Response::Error { message },
    };
    {
        let mut st = lock(&server.state);
        st.blocks.insert(bid, block);
        let Some(sess) = st.session_mut(sid) else {
            return Response::Error {
                message: "session vanished".into(),
            };
        };
        sess.windows.push(Window {
            id: wid,
            name,
            layout: Layout::Block(bid),
            focused: bid,
            cols,
            rows,
        });
        sess.active = wid;
        st.last_focus = Some(bid);
    }
    server.emit(Event::Changed);
    Response::Created {
        session: sid,
        window: wid,
        block: bid,
    }
}

pub fn split(
    server: &Server,
    target: Option<&str>,
    dir: Direction,
    ratio: Option<f32>,
    spawn: &Spawn,
    focus: bool,
) -> Response {
    let (sid, wid, at, bid) = {
        let mut st = lock(&server.state);
        let at = match st.resolve_block(target) {
            Ok(b) => b,
            Err(message) => return Response::Error { message },
        };
        let Some((sid, wid)) = st.locate(at) else {
            return Response::Error {
                message: format!("{at} is not in a window"),
            };
        };
        (sid, wid, at, BlockId(st.next_id()))
    };
    let spawn = Spawn {
        cwd: spawn.cwd.clone().or_else(|| {
            lock(&server.state)
                .blocks
                .get(&at)
                .and_then(|b| b.cwd.clone())
        }),
        ..spawn.clone()
    };
    let block = match spawn_block(
        server,
        bid,
        &spawn,
        DEFAULT_COLS / 2,
        DEFAULT_ROWS,
        &sid.to_string(),
        &wid.to_string(),
    ) {
        Ok(b) => b,
        Err(message) => return Response::Error { message },
    };
    {
        let mut st = lock(&server.state);
        st.blocks.insert(bid, block);
        let Some(win) = st.window_mut(sid, wid) else {
            return Response::Error {
                message: "window vanished".into(),
            };
        };
        if !State::split_layout(
            &mut win.layout,
            at,
            bid,
            dir,
            ratio.unwrap_or(0.5).clamp(0.1, 0.9),
        ) {
            return Response::Error {
                message: format!("{at} is no longer in its window"),
            };
        }
        if focus {
            win.focused = bid;
            st.last_focus = Some(bid);
        }
        st.apply_layout(sid, wid);
    }
    server.emit(Event::Changed);
    Response::Created {
        session: sid,
        window: wid,
        block: bid,
    }
}

/// Removes a block from its window; empty windows and sessions are removed too.
pub fn remove_block(server: &Server, id: BlockId) {
    let removed = {
        let mut st = lock(&server.state);
        let block = st.blocks.remove(&id);
        if let Some((sid, wid)) = st.locate(id) {
            let mut emptied_session = false;
            if let Some(sess) = st.session_mut(sid) {
                if let Some(pos) = sess.windows.iter().position(|w| w.id == wid) {
                    let win = &mut sess.windows[pos];
                    let layout = std::mem::replace(&mut win.layout, Layout::Block(id));
                    match State::remove_from_layout(layout, id) {
                        Some(l) => {
                            if win.focused == id {
                                win.focused = l.blocks()[0];
                            }
                            win.layout = l;
                        }
                        None => {
                            sess.windows.remove(pos);
                            if sess.active == wid
                                && let Some(w) = sess
                                    .windows
                                    .get(pos.saturating_sub(1))
                                    .or(sess.windows.first())
                            {
                                sess.active = w.id;
                            }
                        }
                    }
                }
                emptied_session = sess.windows.is_empty();
            }
            if emptied_session {
                st.sessions.retain(|s| s.id != sid);
            } else {
                st.apply_layout(sid, wid);
            }
        }
        if st.last_focus == Some(id) {
            st.last_focus = None;
        }
        block
    };
    if let Some(b) = removed {
        b.kill();
        server.emit(Event::Changed);
    }
}

pub fn close(server: &Server, target: &str) -> Response {
    let blocks: Vec<BlockId> = {
        let st = lock(&server.state);
        match st.resolve(target) {
            Some(Target::Block(_, _, b)) => vec![b],
            Some(Target::Window(s, w)) => st
                .session(s)
                .and_then(|x| x.windows.iter().find(|y| y.id == w))
                .map(|w| w.layout.blocks())
                .unwrap_or_default(),
            Some(Target::Session(s)) => st
                .session(s)
                .map(|x| x.windows.iter().flat_map(|w| w.layout.blocks()).collect())
                .unwrap_or_default(),
            None => {
                return Response::Error {
                    message: format!("no session, window or block matches “{target}”"),
                };
            }
        }
    };
    for b in blocks {
        remove_block(server, b);
    }
    Response::Ok
}

pub fn rename(server: &Server, target: &str, name: &str) -> Response {
    let name = name.trim();
    if name.is_empty() {
        return Response::Error {
            message: "name cannot be empty".into(),
        };
    }
    {
        let mut st = lock(&server.state);
        match st.resolve(target) {
            Some(Target::Session(s)) => {
                if st.sessions.iter().any(|x| x.name == name && x.id != s) {
                    return Response::Error {
                        message: format!("a session named “{name}” already exists"),
                    };
                }
                if let Some(x) = st.session_mut(s) {
                    x.name = name.to_owned();
                }
            }
            Some(Target::Window(s, w)) => {
                if let Some(x) = st.window_mut(s, w) {
                    x.name = name.to_owned();
                }
            }
            Some(Target::Block(_, _, b)) => {
                if let Some(x) = st.blocks.get(&b) {
                    *lock(&x.label) = Some(name.to_owned());
                }
            }
            None => {
                return Response::Error {
                    message: format!("nothing matches “{target}”"),
                };
            }
        }
    }
    server.emit(Event::Changed);
    Response::Ok
}

pub fn focus(server: &Server, target: &str) -> Response {
    {
        let mut st = lock(&server.state);
        let Some(t) = st.resolve(target) else {
            return Response::Error {
                message: format!("nothing matches “{target}”"),
            };
        };
        let (sid, wid, block) = match t {
            Target::Block(s, w, b) => (s, Some(w), Some(b)),
            Target::Window(s, w) => (s, Some(w), None),
            Target::Session(s) => (s, None, None),
        };
        let Some(sess) = st.session_mut(sid) else {
            return Response::Error {
                message: "session vanished".into(),
            };
        };
        if let Some(w) = wid {
            sess.active = w;
        }
        let active = sess.active;
        if let (Some(b), Some(win)) = (block, sess.windows.iter_mut().find(|x| x.id == active)) {
            win.focused = b;
        }
        let focused = sess
            .windows
            .iter()
            .find(|x| x.id == active)
            .map(|w| w.focused);
        st.last_focus = focused;
    }
    server.emit(Event::Changed);
    Response::Ok
}

pub fn resize(server: &Server, target: &str, cols: u16, rows: u16) -> Response {
    let mut st = lock(&server.state);
    let (sid, wid) = match st.resolve(target) {
        Some(Target::Window(s, w) | Target::Block(s, w, _)) => (s, w),
        Some(Target::Session(s)) => match st.session(s) {
            Some(x) => (s, x.active),
            None => {
                return Response::Error {
                    message: "session vanished".into(),
                };
            }
        },
        None => {
            return Response::Error {
                message: format!("nothing matches “{target}”"),
            };
        }
    };
    if let Some(w) = st.window_mut(sid, wid) {
        w.cols = cols.clamp(4, 1000);
        w.rows = rows.clamp(2, 500);
    }
    st.apply_layout(sid, wid);
    Response::Ok
}
