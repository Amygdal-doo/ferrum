//! App state: connection to the server, the current session, frames, and
//! the actions behind keys, shortcuts and clicks. Closing the window only
//! disconnects; every program keeps running in the server.

use crate::input::{Action, Shortcut};
use crate::view::TabKind;
use ferrumterm_client::{Client, connect_or_start, paths};
use ferrumterm_protocol::{
    BlockId, Direction, Event, Frame, Input, Request, Response, SessionId, SessionInfo, Spawn,
    WindowInfo,
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Messages from the tokio side to the GPUI view.
pub enum UiEvent {
    Connected(Client),
    Server(Event),
    Sessions(Vec<SessionInfo>),
    Message(String),
    Disconnected(String),
    Clipboard(String),
}

pub type UiSender = futures::channel::mpsc::UnboundedSender<UiEvent>;

pub struct App {
    pub rt: tokio::runtime::Runtime,
    pub proxy: UiSender,
    pub client: Option<Client>,
    pub sessions: Vec<SessionInfo>,
    pub current: Option<SessionId>,
    pub frames: HashMap<BlockId, Frame>,
    pub dirty: HashSet<BlockId>,
    pub subscribed: Vec<BlockId>,
    pub font_pt: f32,
    /// Pane rectangles in window px from the last paint, for clicks.
    pub panes: Vec<(BlockId, f32, f32, f32, f32)>,
    pub message: Option<(String, Instant)>,
    creating: bool,
    want_newest: bool,
}

pub const DEFAULT_FONT_PT: f32 = 14.0;

fn spawn_env() -> Spawn {
    let env = ["PATH", "LANG", "SSH_AUTH_SOCK"]
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| ((*k).to_owned(), v)))
        .collect();
    Spawn {
        env,
        ..Spawn::default()
    }
}

impl App {
    pub fn new(rt: tokio::runtime::Runtime, proxy: UiSender) -> Self {
        Self {
            rt,
            proxy,
            client: None,
            sessions: Vec::new(),
            current: None,
            frames: HashMap::new(),
            dirty: HashSet::new(),
            subscribed: Vec::new(),
            font_pt: DEFAULT_FONT_PT,
            panes: Vec::new(),
            message: None,
            creating: false,
            want_newest: false,
        }
    }

    /// Connects (starting the server if needed) and forwards its events.
    pub fn connect(&self, delay: Duration) {
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            tokio::time::sleep(delay).await;
            match connect_or_start(&paths::socket(), "ferrumterm").await {
                Ok((client, mut events)) => {
                    let _ = client.request(Request::Watch).await;
                    let _ = proxy.unbounded_send(UiEvent::Connected(client));
                    while let Some(e) = events.recv().await {
                        if proxy.unbounded_send(UiEvent::Server(e)).is_err() {
                            return;
                        }
                    }
                    let _ = proxy.unbounded_send(UiEvent::Disconnected(
                        "connection to the server was lost".into(),
                    ));
                }
                Err(e) => {
                    let _ = proxy.unbounded_send(UiEvent::Disconnected(e.to_string()));
                }
            }
        });
    }

    pub fn send(&self, req: Request) {
        let Some(c) = self.client.clone() else { return };
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            if let Err(e) = c.request(req).await {
                let _ = proxy.unbounded_send(UiEvent::Message(e.to_string()));
            }
        });
    }

    pub fn refresh(&self) {
        let Some(c) = self.client.clone() else { return };
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            if let Ok(Response::Sessions { sessions }) = c.request(Request::List).await {
                let _ = proxy.unbounded_send(UiEvent::Sessions(sessions));
            }
        });
    }

    pub fn notify(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), Instant::now()));
    }

    pub fn session(&self) -> Option<&SessionInfo> {
        self.current
            .and_then(|id| self.sessions.iter().find(|s| s.id == id))
            .or(self.sessions.first())
    }

    pub fn window(&self) -> Option<&WindowInfo> {
        let s = self.session()?;
        s.windows
            .iter()
            .find(|w| w.id == s.active_window)
            .or(s.windows.first())
    }

    pub fn focused(&self) -> Option<BlockId> {
        self.window().map(|w| w.focused)
    }

    pub fn set_sessions(&mut self, sessions: Vec<SessionInfo>) {
        if self.want_newest && sessions.len() > self.sessions.len() {
            self.current = sessions.iter().max_by_key(|s| s.created_ms).map(|s| s.id);
            self.want_newest = false;
        }
        self.sessions = sessions;
        if self
            .current
            .is_none_or(|id| !self.sessions.iter().any(|s| s.id == id))
        {
            self.current = self.sessions.first().map(|s| s.id);
        }
        if self.sessions.is_empty() {
            if !self.creating {
                self.creating = true;
                self.send(Request::NewSession {
                    name: None,
                    spawn: spawn_env(),
                });
            }
        } else {
            self.creating = false;
        }
    }

    /// Sizes the visible window to `cells` and subscribes to its blocks.
    pub fn sync(&mut self, cells: Option<(u16, u16)>) {
        let Some(w) = self.window().cloned() else {
            return;
        };
        if let Some((cols, rows)) = cells
            && (w.cols, w.rows) != (cols, rows)
        {
            self.send(Request::Resize {
                target: w.id.to_string(),
                cols,
                rows,
            });
        }
        let blocks = w.layout.blocks();
        if blocks != self.subscribed {
            self.frames.retain(|b, _| blocks.contains(b));
            self.subscribed.clone_from(&blocks);
            self.send(Request::Subscribe { blocks });
        }
    }

    pub fn on_event(&mut self, e: Event) {
        match e {
            Event::Frame { block, frame } => {
                self.frames.insert(block, frame);
                self.dirty.insert(block);
            }
            Event::Changed | Event::Status { .. } | Event::Exited { .. } => self.refresh(),
            Event::Bell { .. } => {}
            Event::Bye => {
                self.client = None;
                self.notify("FerrumTerm server stopped — reconnecting…");
                self.connect(Duration::from_secs(1));
            }
        }
    }

    pub fn act(&mut self, action: Action) {
        let target = self.focused().map(|b| b.to_string());
        match action {
            Action::Send(input) => {
                if target.is_some() {
                    self.send(Request::Input {
                        target,
                        input: vec![input],
                    });
                }
            }
            Action::Shortcut(s) => self.shortcut(s, target),
        }
    }

    fn cycle_block(&self, step: isize) -> Option<String> {
        let w = self.window()?;
        let blocks = w.layout.blocks();
        let i = blocks.iter().position(|b| *b == w.focused)? as isize;
        let n = blocks.len() as isize;
        Some(blocks[((i + step).rem_euclid(n)) as usize].to_string())
    }

    fn window_at(&self, step: Option<isize>, index: Option<usize>) -> Option<String> {
        let s = self.session()?;
        let i = s.windows.iter().position(|w| w.id == s.active_window)? as isize;
        let n = s.windows.len() as isize;
        let idx = match (step, index) {
            (Some(d), _) => (i + d).rem_euclid(n) as usize,
            (None, Some(x)) => x,
            _ => return None,
        };
        s.windows.get(idx).map(|w| w.id.to_string())
    }

    fn shortcut(&mut self, s: Shortcut, target: Option<String>) {
        let session = self.session().map(|x| x.id.to_string());
        let half = self.window().map_or(10, |w| i32::from(w.rows / 2));
        match s {
            Shortcut::NewTab => self.send(Request::NewWindow {
                session,
                name: None,
                spawn: spawn_env(),
            }),
            Shortcut::NewSession => {
                self.want_newest = true;
                self.send(Request::NewSession {
                    name: None,
                    spawn: spawn_env(),
                });
            }
            Shortcut::SplitRight => self.send(Request::Split {
                target,
                dir: Direction::Right,
                ratio: None,
                spawn: spawn_env(),
                focus: true,
            }),
            Shortcut::SplitDown => self.send(Request::Split {
                target,
                dir: Direction::Down,
                ratio: None,
                spawn: spawn_env(),
                focus: true,
            }),
            Shortcut::CloseBlock => {
                if let Some(t) = target {
                    self.send(Request::Close { target: t });
                }
            }
            Shortcut::NextBlock | Shortcut::PrevBlock => {
                if let Some(t) = self.cycle_block(if s == Shortcut::NextBlock { 1 } else { -1 }) {
                    self.send(Request::Focus { target: t });
                }
            }
            Shortcut::NextTab | Shortcut::PrevTab | Shortcut::Tab(_) => {
                let t = match s {
                    Shortcut::NextTab => self.window_at(Some(1), None),
                    Shortcut::PrevTab => self.window_at(Some(-1), None),
                    Shortcut::Tab(i) => self.window_at(None, Some(i)),
                    _ => None,
                };
                if let Some(t) = t {
                    self.send(Request::Focus { target: t });
                }
            }
            Shortcut::NextSession => {
                if let Some(cur) = self.current
                    && let Some(i) = self.sessions.iter().position(|x| x.id == cur)
                {
                    self.current = self
                        .sessions
                        .get((i + 1) % self.sessions.len())
                        .map(|x| x.id);
                    self.subscribed.clear();
                }
            }
            // The view reads the clipboard (it owns the GPUI context) and calls `paste`.
            Shortcut::Paste => {}
            Shortcut::CopyScreen => {
                let Some(c) = self.client.clone() else { return };
                let proxy = self.proxy.clone();
                self.rt.spawn(async move {
                    if let Ok(Response::Text { text }) = c
                        .request(Request::Capture {
                            target,
                            scrollback: false,
                        })
                        .await
                    {
                        let _ = proxy.unbounded_send(UiEvent::Clipboard(text));
                    }
                });
            }
            Shortcut::FontBigger | Shortcut::FontSmaller | Shortcut::FontReset => {
                self.font_pt = match s {
                    Shortcut::FontBigger => (self.font_pt + 1.0).min(40.0),
                    Shortcut::FontSmaller => (self.font_pt - 1.0).max(8.0),
                    _ => DEFAULT_FONT_PT,
                };
            }
            Shortcut::ScrollPageUp => self.send(Request::Scroll {
                target,
                lines: Some(half),
            }),
            Shortcut::ScrollPageDown => self.send(Request::Scroll {
                target,
                lines: Some(-half),
            }),
        }
    }

    /// Mouse wheel: scrollback normally; arrow keys inside full-screen programs.
    pub fn scroll(&mut self, lines: i32) {
        let Some(b) = self.focused() else { return };
        let alt = self.frames.get(&b).is_some_and(|f| f.alt_screen);
        if lines == 0 {
            return;
        }
        if alt {
            let key = if lines > 0 { "Up" } else { "Down" };
            let input = (0..lines.unsigned_abs().min(20))
                .map(|_| Input::Key(key.into()))
                .collect();
            self.send(Request::Input {
                target: Some(b.to_string()),
                input,
            });
        } else {
            self.send(Request::Scroll {
                target: Some(b.to_string()),
                lines: Some(lines),
            });
        }
    }

    pub fn paste(&mut self, text: String) {
        let target = self.focused().map(|b| b.to_string());
        if target.is_some() {
            self.send(Request::Input {
                target,
                input: vec![Input::Paste(text)],
            });
        }
    }

    pub fn click_tab(&mut self, kind: TabKind) {
        match kind {
            TabKind::Session => self.shortcut(Shortcut::NextSession, None),
            TabKind::Window(id) => self.send(Request::Focus { target: id }),
            TabKind::NewWindow => self.shortcut(Shortcut::NewTab, None),
        }
    }

    /// Focuses the pane under window point `(x, y)`.
    pub fn click(&mut self, x: f32, y: f32) {
        if let Some((b, ..)) = self
            .panes
            .iter()
            .find(|(_, px, py, w, h)| x >= *px && x < px + w && y >= *py && y < py + h)
            && Some(*b) != self.focused()
        {
            self.send(Request::Focus {
                target: b.to_string(),
            });
        }
    }
}
