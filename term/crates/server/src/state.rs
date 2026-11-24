//! Sessions → windows → split layouts of blocks, and target resolution.

use crate::block::{Block, lock};
use ferrumterm_protocol::{
    Axis, BlockId, BlockInfo, Direction, Inspected, Layout, Rect, SessionId, SessionInfo, WindowId,
    WindowInfo,
};
use std::collections::HashMap;
use std::sync::Arc;

/// Window size before any client reports one.
pub const DEFAULT_COLS: u16 = 120;
pub const DEFAULT_ROWS: u16 = 36;

pub struct Window {
    pub id: WindowId,
    pub name: String,
    pub layout: Layout,
    pub focused: BlockId,
    pub cols: u16,
    pub rows: u16,
}

pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub created_ms: u64,
    pub windows: Vec<Window>,
    pub active: WindowId,
}

#[derive(Default)]
pub struct State {
    pub sessions: Vec<Session>,
    pub blocks: HashMap<BlockId, Arc<Block>>,
    next: u32,
    /// Last block focused by any client (default target).
    pub last_focus: Option<BlockId>,
    /// Notes shared by each session's agents (`Context`), oldest first;
    /// they go with the session.
    pub context: HashMap<SessionId, Vec<String>>,
}

/// A resolved target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Session(SessionId),
    Window(SessionId, WindowId),
    Block(SessionId, WindowId, BlockId),
}

impl State {
    pub fn next_id(&mut self) -> u32 {
        self.next += 1;
        self.next
    }

    pub fn session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn session_mut(&mut self, id: SessionId) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    pub fn window_mut(&mut self, s: SessionId, w: WindowId) -> Option<&mut Window> {
        self.session_mut(s)?.windows.iter_mut().find(|x| x.id == w)
    }

    /// Session and window containing `block`.
    pub fn locate(&self, block: BlockId) -> Option<(SessionId, WindowId)> {
        self.sessions.iter().find_map(|s| {
            s.windows
                .iter()
                .find(|w| w.layout.blocks().contains(&block))
                .map(|w| (s.id, w.id))
        })
    }

    /// Resolves `sN`/`wN`/`bN`, a session name, window name or block label.
    pub fn resolve(&self, target: &str) -> Option<Target> {
        let t = target.trim();
        if let Ok(b) = t.parse::<BlockId>()
            && let Some((s, w)) = self.locate(b)
        {
            return Some(Target::Block(s, w, b));
        }
        if let Ok(w) = t.parse::<WindowId>()
            && let Some(s) = self
                .sessions
                .iter()
                .find(|s| s.windows.iter().any(|x| x.id == w))
        {
            return Some(Target::Window(s.id, w));
        }
        if let Ok(id) = t.parse::<SessionId>()
            && self.session(id).is_some()
        {
            return Some(Target::Session(id));
        }
        if let Some(s) = self.sessions.iter().find(|s| s.name == t) {
            return Some(Target::Session(s.id));
        }
        for s in &self.sessions {
            if let Some(w) = s.windows.iter().find(|w| w.name == t) {
                return Some(Target::Window(s.id, w.id));
            }
        }
        self.blocks
            .values()
            .find(|b| lock(&b.label).as_deref() == Some(t))
            .and_then(|b| self.locate(b.id).map(|(s, w)| Target::Block(s, w, b.id)))
    }

    /// The block a target means for input/capture: a window's focused block,
    /// a session's active window's focused block.
    pub fn resolve_block(&self, target: Option<&str>) -> Result<BlockId, String> {
        let Some(t) = target else {
            return self
                .last_focus
                .filter(|b| self.blocks.contains_key(b))
                .or_else(|| {
                    self.sessions
                        .first()
                        .and_then(|s| self.focused_in(s.id, s.active))
                })
                .ok_or_else(|| "no terminals are running; create one with `fterm new`".to_owned());
        };
        match self.resolve(t) {
            Some(Target::Block(_, _, b)) => Ok(b),
            Some(Target::Window(s, w)) => self
                .focused_in(s, w)
                .ok_or_else(|| format!("{t} has no blocks")),
            Some(Target::Session(s)) => {
                let sess = self.session(s).ok_or("session vanished")?;
                self.focused_in(s, sess.active)
                    .ok_or_else(|| format!("{t} has no blocks"))
            }
            None => Err(format!("no session, window or block matches “{t}”")),
        }
    }

    fn focused_in(&self, s: SessionId, w: WindowId) -> Option<BlockId> {
        self.session(s)?
            .windows
            .iter()
            .find(|x| x.id == w)
            .map(|x| x.focused)
    }

    /// Replaces `block` in `layout` with a split of it and `new`.
    pub fn split_layout(
        layout: &mut Layout,
        block: BlockId,
        new: BlockId,
        dir: Direction,
        ratio: f32,
    ) -> bool {
        match layout {
            Layout::Block(b) if *b == block => {
                let (axis, first, second) = match dir {
                    Direction::Right => (Axis::Row, block, new),
                    Direction::Left => (Axis::Row, new, block),
                    Direction::Down => (Axis::Column, block, new),
                    Direction::Up => (Axis::Column, new, block),
                };
                // `ratio` is the new block's share.
                let r = if first == new { ratio } else { 1.0 - ratio };
                *layout = Layout::Split {
                    axis,
                    ratio: r,
                    a: Box::new(Layout::Block(first)),
                    b: Box::new(Layout::Block(second)),
                };
                true
            }
            Layout::Block(_) => false,
            Layout::Split { a, b, .. } => {
                Self::split_layout(a, block, new, dir, ratio)
                    || Self::split_layout(b, block, new, dir, ratio)
            }
        }
    }

    /// Removes `block`; the sibling takes the parent's place. `None` when the layout becomes empty.
    pub fn remove_from_layout(layout: Layout, block: BlockId) -> Option<Layout> {
        match layout {
            Layout::Block(b) if b == block => None,
            Layout::Block(_) => Some(layout),
            Layout::Split { axis, ratio, a, b } => match (
                Self::remove_from_layout(*a, block),
                Self::remove_from_layout(*b, block),
            ) {
                (Some(a), Some(b)) => Some(Layout::Split {
                    axis,
                    ratio,
                    a: Box::new(a),
                    b: Box::new(b),
                }),
                (Some(x), None) | (None, Some(x)) => Some(x),
                (None, None) => None,
            },
        }
    }

    /// Resizes every block of a window to its layout rectangle.
    pub fn apply_layout(&self, s: SessionId, w: WindowId) {
        let Some(win) = self
            .session(s)
            .and_then(|x| x.windows.iter().find(|y| y.id == w))
        else {
            return;
        };
        for (b, r) in win.layout.rects(Rect {
            x: 0,
            y: 0,
            cols: win.cols,
            rows: win.rows,
        }) {
            if let Some(block) = self.blocks.get(&b) {
                block.resize(r.cols, r.rows);
            }
        }
    }

    pub fn block_info(&self, id: BlockId) -> Option<BlockInfo> {
        let b = self.blocks.get(&id)?;
        let term = lock(&b.term);
        Some(BlockInfo {
            id,
            label: lock(&b.label).clone(),
            title: term.title().to_owned(),
            command: b.command.clone(),
            cwd: b.cwd.clone(),
            pid: b.pid,
            cols: term.cols(),
            rows: term.rows(),
            status: lock(&b.status).clone(),
            exited: *lock(&b.exited),
            agent: lock(&b.agent).clone(),
        })
    }

    fn window_info(&self, w: &Window) -> WindowInfo {
        WindowInfo {
            id: w.id,
            name: w.name.clone(),
            layout: w.layout.clone(),
            focused: w.focused,
            zoomed: false,
            cols: w.cols,
            rows: w.rows,
            blocks: w
                .layout
                .blocks()
                .into_iter()
                .filter_map(|b| self.block_info(b))
                .collect(),
        }
    }

    pub fn session_info(&self, s: &Session) -> SessionInfo {
        SessionInfo {
            id: s.id,
            name: s.name.clone(),
            created_ms: s.created_ms,
            active_window: s.active,
            windows: s.windows.iter().map(|w| self.window_info(w)).collect(),
        }
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        self.sessions.iter().map(|s| self.session_info(s)).collect()
    }

    pub fn inspect(&self, t: Target) -> Option<Inspected> {
        Some(match t {
            Target::Session(s) => Inspected::Session(self.session_info(self.session(s)?)),
            Target::Window(s, w) => Inspected::Window(
                self.window_info(self.session(s)?.windows.iter().find(|x| x.id == w)?),
            ),
            Target::Block(_, _, b) => Inspected::Block(self.block_info(b)?),
        })
    }

    /// A free default name (`1`, `2`, …).
    pub fn default_session_name(&self) -> String {
        (1..)
            .map(|n: u32| n.to_string())
            .find(|n| !self.sessions.iter().any(|s| &s.name == n))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_and_remove_collapse_layout() {
        let mut l = Layout::Block(BlockId(1));
        assert!(State::split_layout(
            &mut l,
            BlockId(1),
            BlockId(2),
            Direction::Right,
            0.5
        ));
        assert!(State::split_layout(
            &mut l,
            BlockId(2),
            BlockId(3),
            Direction::Up,
            0.3
        ));
        assert_eq!(l.blocks(), vec![BlockId(1), BlockId(3), BlockId(2)]);
        let l = State::remove_from_layout(l, BlockId(3)).expect("layout");
        assert_eq!(l.blocks(), vec![BlockId(1), BlockId(2)]);
        let l = State::remove_from_layout(l, BlockId(1)).expect("layout");
        assert_eq!(l, Layout::Block(BlockId(2)));
        assert_eq!(State::remove_from_layout(l, BlockId(2)), None);
    }
}
