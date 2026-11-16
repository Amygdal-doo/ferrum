//! App state: connection to the server, the current session, frames, and
//! the actions behind keys, shortcuts and clicks. Closing the window only
//! disconnects; every program keeps running in the server.

use crate::connection::{self, Backend};
use crate::events::CommandId;
use crate::input::{Action, Shortcut};
use crate::options::TermViewOptions;
use crate::sidebar::agents;
use crate::sidebar::model::Seen;
use crate::ui_state::UiState;
use crate::view::TabKind;
use ferrumterm_client::Client;
use ferrumterm_protocol::{
    BlockId, Direction, Event, Frame, Input, Request, Response, SessionId, SessionInfo, Spawn,
    WindowInfo,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Messages from the tokio side to the GPUI view.
pub enum UiEvent {
    /// Connected; the server's start time (`ServerInfo::started_ms`) when known.
    Connected(Client, Backend, Option<u64>),
    /// A `run_command` tab was created.
    CommandStarted(CommandId, BlockId),
    Server(Event),
    Sessions(Vec<SessionInfo>),
    Message(String),
    Disconnected(String),
    /// The shared server is from an older build; waits for "Restart server".
    Incompatible(String),
    Clipboard(String),
    Forwards(Vec<ferrumterm_protocol::ForwardInfo>),
    /// A terminal created by "New terminal in this folder": (its id, folder).
    IntoFolder(String, u64),
    Ports(crate::ssh::PortsResult),
    Killed(crate::ssh::KillOutcome),
}

pub type UiSender = futures::channel::mpsc::UnboundedSender<UiEvent>;

/// Work queued until the view's session exists (API calls made before the
/// connection is up).
type Deferred = Box<dyn FnOnce(&mut App)>;

pub struct App {
    pub rt: tokio::runtime::Handle,
    pub proxy: UiSender,
    pub client: Option<Client>,
    pub backend: Option<Backend>,
    /// Use the shared server (else the private in-process one).
    attach: bool,
    /// Fall back to the private server when the shared one can't be
    /// reached or started (embedded views; the desktop window retries).
    fallback: bool,
    /// Show this session (created when missing) instead of the first one.
    pub session_name: Option<String>,
    /// Directory for new sessions, tabs and splits (`None`: server default).
    pub default_cwd: Option<PathBuf>,
    /// Sidebar open/width are saved (the desktop window), else only the
    /// recent agent commands are.
    persist_layout: bool,
    pub default_font_pt: f32,
    /// Blocks started by `run_command`.
    pub commands: HashMap<BlockId, CommandId>,
    next_command: u64,
    /// Recent exits not yet matched to a `run_command` (fast commands can
    /// exit before their creation reply arrives).
    pub early_exits: Vec<(BlockId, i32)>,
    deferred: Vec<Deferred>,
    pub sessions: Vec<SessionInfo>,
    pub current: Option<SessionId>,
    pub frames: HashMap<BlockId, Frame>,
    pub dirty: HashSet<BlockId>,
    pub subscribed: Vec<BlockId>,
    pub font_pt: f32,
    /// Pane rectangles in window px from the last paint, for clicks.
    pub panes: Vec<(BlockId, f32, f32, f32, f32)>,
    pub message: Option<(String, Instant)>,
    /// Agent statuses already shown on screen (for the sidebar's badge).
    pub seen: Seen,
    /// The window showing this view is the active one (as of the last render).
    pub window_active: bool,
    /// Which agent transitions already notified (see `alerts`).
    pub alerts: crate::alerts::Alerts,
    /// This view posts agent notifications (the FerrumTerm window).
    pub notifies: bool,
    pub ui: UiState,
    /// SSH hosts and port forwards (sidebar).
    pub ssh: crate::ssh::SshState,
    /// Sidebar folders grouping terminals.
    pub folders: crate::folders::Folders,
    creating: bool,
    want_newest: bool,
}

pub const DEFAULT_FONT_PT: f32 = 14.0;
pub const COMPACT_FONT_PT: f32 = 12.0;
const EARLY_EXITS: usize = 64;

/// `~/src` → `$HOME/src`.
fn expand_tilde(path: &str) -> String {
    match (path.strip_prefix('~'), std::env::var("HOME")) {
        (Some(rest), Ok(home)) if rest.is_empty() || rest.starts_with('/') => {
            format!("{home}{rest}")
        }
        _ => path.to_owned(),
    }
}

pub(crate) fn spawn_env() -> Spawn {
    let env = ["PATH", "LANG", "SSH_AUTH_SOCK"]
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| ((*k).to_owned(), v)))
        .collect();
    Spawn {
        command: crate::settings::get().startup_argv(),
        env,
        ..Spawn::default()
    }
}

impl App {
    pub fn new(options: &TermViewOptions, proxy: UiSender) -> Self {
        let mut ui = UiState::load();
        if !options.standalone {
            ui.sidebar_open = options.show_sidebar;
        }
        let default_font_pt = if options.compact {
            COMPACT_FONT_PT
        } else {
            crate::settings::get().font_size
        };
        let mut app = Self {
            rt: connection::runtime(),
            proxy,
            client: None,
            backend: None,
            attach: options.attach_to_server,
            fallback: !options.standalone,
            session_name: options.session_name.clone(),
            default_cwd: options.cwd.clone(),
            persist_layout: options.standalone,
            default_font_pt,
            commands: HashMap::new(),
            next_command: 0,
            early_exits: Vec::new(),
            deferred: Vec::new(),
            sessions: Vec::new(),
            current: None,
            frames: HashMap::new(),
            dirty: HashSet::new(),
            subscribed: Vec::new(),
            font_pt: default_font_pt,
            panes: Vec::new(),
            message: None,
            seen: Seen::default(),
            window_active: true,
            alerts: crate::alerts::Alerts::default(),
            notifies: options.standalone,
            ui,
            ssh: crate::ssh::SshState::default(),
            // Only the FerrumTerm window keeps folders (one file per machine).
            folders: if options.standalone {
                crate::folders::Folders::load()
            } else {
                crate::folders::Folders::default()
            },
            creating: false,
            want_newest: false,
        };
        app.reload_ssh();
        app
    }

    /// Connects (starting the server if needed, else falling back to the
    /// private one) and forwards its events.
    pub fn connect(&self, delay: Duration) {
        let proxy = self.proxy.clone();
        let (attach, fallback) = (self.attach, self.fallback);
        // The FerrumTerm window tells the server it delivers notifications.
        let name = if self.notifies {
            ferrumterm_protocol::WINDOW_CLIENT
        } else {
            "ferrumterm"
        };
        self.rt.spawn(async move {
            tokio::time::sleep(delay).await;
            match connection::connect(attach, fallback, name).await {
                Ok((client, mut events, backend)) => {
                    let _ = client.request(Request::Watch).await;
                    let started = client.info().await.ok().map(|i| i.started_ms);
                    let _ = proxy.unbounded_send(UiEvent::Connected(client, backend, started));
                    while let Some(e) = events.recv().await {
                        if proxy.unbounded_send(UiEvent::Server(e)).is_err() {
                            return;
                        }
                    }
                    let _ = proxy.unbounded_send(UiEvent::Disconnected(
                        "connection to the server was lost".into(),
                    ));
                }
                Err(e) => match e.strip_prefix(connection::INCOMPATIBLE) {
                    Some(why) => {
                        let _ = proxy.unbounded_send(UiEvent::Incompatible(why.to_owned()));
                    }
                    None => {
                        let _ = proxy.unbounded_send(UiEvent::Disconnected(e));
                    }
                },
            }
        });
    }

    /// Stops the shared server (even one too old to talk to), then connects
    /// again, which starts the current build (`ferrumterm_client::fterm_binary`).
    pub fn restart_server(&self) {
        let proxy = self.proxy.clone();
        // While connected, losing the connection already reconnects.
        let connected = self.client.is_some();
        self.rt.spawn(async move {
            let event = match connection::restart_shared().await {
                Ok(()) if connected => UiEvent::Message("Restarting the FerrumTerm server…".into()),
                Ok(()) => UiEvent::Disconnected("Restarting the FerrumTerm server".into()),
                Err(e) if connected => {
                    UiEvent::Message(format!("could not restart the server: {e}"))
                }
                Err(e) => UiEvent::Incompatible(format!("could not restart the server: {e}")),
            };
            let _ = proxy.unbounded_send(event);
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
            .or_else(|| self.home_session())
    }

    /// The session to show by default: the named one, else the first.
    fn home_session(&self) -> Option<&SessionInfo> {
        match &self.session_name {
            Some(name) => self.sessions.iter().find(|s| &s.name == name),
            None => self.sessions.first(),
        }
    }

    /// `spawn_env()` in the default directory.
    pub fn spawn_here(&self) -> Spawn {
        Spawn {
            cwd: self.default_cwd.as_ref().map(|d| d.display().to_string()),
            ..spawn_env()
        }
    }

    /// Whether events about `block` concern this view: any block, unless it
    /// shows one named session.
    pub fn shows_block(&self, block: BlockId) -> bool {
        self.session_name.is_none()
            || self.commands.contains_key(&block)
            || self
                .home_session()
                .is_some_and(|s| s.windows.iter().any(|w| w.layout.blocks().contains(&block)))
    }

    /// Whether `block` is in the (last listed) sessions.
    pub fn knows_block(&self, block: BlockId) -> bool {
        self.sessions
            .iter()
            .flat_map(|s| &s.windows)
            .any(|w| w.blocks.iter().any(|b| b.id == block))
    }

    /// Sidebar folders are offered (the FerrumTerm window).
    pub fn folders_enabled(&self) -> bool {
        self.persist_layout
    }

    /// The host asked for the shared server (so a private one is a fallback).
    pub fn attach_requested(&self) -> bool {
        self.attach
    }

    /// Runs `f` now when connected with a session, else once there is one.
    pub fn when_ready(&mut self, f: impl FnOnce(&mut Self) + 'static) {
        if self.client.is_some() && self.session().is_some() {
            f(self);
        } else {
            self.deferred.push(Box::new(f));
        }
    }

    fn run_deferred(&mut self) {
        if self.client.is_none() || self.session().is_none() {
            return;
        }
        for f in std::mem::take(&mut self.deferred) {
            f(self);
        }
    }

    pub fn next_command_id(&mut self) -> CommandId {
        self.next_command += 1;
        CommandId(self.next_command)
    }

    /// Opens a tab labelled after the program running `command_line`
    /// through the login shell; the tab stays open after it exits.
    pub fn run_command(&mut self, id: CommandId, command_line: &str, cwd: Option<PathBuf>) {
        let label = agents::unique_label(&agents::label_for(command_line), &self.sessions);
        let cwd = cwd.or_else(|| self.default_cwd.clone());
        let request = Request::NewWindow {
            session: self.session().map(|s| s.id.to_string()),
            name: Some(label.clone()),
            spawn: Spawn {
                command: agents::shell_command(command_line),
                cwd: cwd.map(|d| d.display().to_string()),
                label: Some(label),
                keep_open: true,
                ..spawn_env()
            },
        };
        let Some(c) = self.client.clone() else { return };
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let event = match c.request(request).await {
                Ok(Response::Created { block, .. }) => UiEvent::CommandStarted(id, block),
                Ok(_) => return,
                Err(e) => UiEvent::Message(e.to_string()),
            };
            let _ = proxy.unbounded_send(event);
        });
    }

    /// Remembers an exit that may belong to a command whose creation reply
    /// has not arrived yet.
    pub fn remember_exit(&mut self, block: BlockId, code: i32) {
        self.early_exits.push((block, code));
        if self.early_exits.len() > EARLY_EXITS {
            self.early_exits.remove(0);
        }
    }

    /// New tab in the current session, in `cwd` (else the default directory).
    pub fn new_tab(&mut self, cwd: Option<PathBuf>) {
        let mut spawn = self.spawn_here();
        if let Some(cwd) = cwd {
            spawn.cwd = Some(cwd.display().to_string());
        }
        self.send(Request::NewWindow {
            session: self.session().map(|s| s.id.to_string()),
            name: None,
            spawn,
        });
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
            self.current = self.home_session().map(|s| s.id);
        }
        if self.home_session().is_none() {
            if !self.creating {
                self.creating = true;
                self.send(Request::NewSession {
                    name: self.session_name.clone(),
                    spawn: self.spawn_here(),
                });
            }
        } else {
            self.creating = false;
        }
        self.seen.retain(&self.sessions);
        let alive: HashSet<BlockId> = self
            .sessions
            .iter()
            .flat_map(|s| &s.windows)
            .flat_map(|w| &w.blocks)
            .map(|b| b.id)
            .collect();
        self.alerts.retain(|b| alive.contains(&b));
        let ids: HashSet<String> = self
            .sessions
            .iter()
            .flat_map(|s| {
                std::iter::once(s.id.to_string()).chain(s.windows.iter().map(|w| w.id.to_string()))
            })
            .collect();
        if self.persist_layout
            && !self.sessions.is_empty()
            && self.folders.prune(|i| ids.contains(i))
        {
            self.folders.save();
        }
        if self.window_active
            && let Some(w) = self.window().cloned()
        {
            self.seen.look_at(&w);
        }
        self.run_deferred();
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
            Event::Forwards => self.refresh_forwards(),
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

    pub(crate) fn shortcut(&mut self, s: Shortcut, target: Option<String>) {
        let session = self.session().map(|x| x.id.to_string());
        let half = self.window().map_or(10, |w| i32::from(w.rows / 2));
        match s {
            Shortcut::NewTab => self.send(Request::NewWindow {
                session,
                name: None,
                spawn: self.spawn_here(),
            }),
            Shortcut::NewSession => {
                self.want_newest = true;
                self.send(Request::NewSession {
                    name: None,
                    spawn: self.spawn_here(),
                });
            }
            Shortcut::SplitRight => self.split(target, Direction::Right),
            Shortcut::SplitDown => self.split(target, Direction::Down),
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
            // The view owns the GPUI clipboard: it pastes (calling `paste`) and copies selections.
            Shortcut::Paste | Shortcut::Copy => {}
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
                    _ => self.default_font_pt,
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
            Shortcut::ToggleSidebar => self.toggle_sidebar(),
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
            TabKind::Window(id) => self.send(Request::Focus { target: id }),
            TabKind::NewWindow => self.shortcut(Shortcut::NewTab, None),
        }
    }

    /// Splits `target` (else the focused pane), focusing the new pane.
    pub fn split(&mut self, target: Option<String>, dir: Direction) {
        let target = target.or_else(|| self.focused().map(|b| b.to_string()));
        self.send(Request::Split {
            target,
            dir,
            ratio: None,
            spawn: self.spawn_here(),
            focus: true,
        });
    }

    pub fn toggle_sidebar(&mut self) {
        self.ui.sidebar_open = !self.ui.sidebar_open;
        self.save_ui();
    }

    /// Saves window preferences: all of them for the desktop window; only
    /// the recent agent commands for an embedded view (its sidebar state is
    /// the host's business).
    pub fn save_ui(&self) {
        if self.persist_layout {
            self.ui.save();
        } else {
            self.ui.save_recent_agents();
        }
    }

    /// Sidebar click: shows `block`'s session and focuses its tab and pane.
    pub fn focus_block(&mut self, session: SessionId, block: BlockId) {
        if self.current != Some(session) {
            self.current = Some(session);
            self.subscribed.clear();
        }
        self.send(Request::Focus {
            target: block.to_string(),
        });
    }

    /// Starts `command_line` in a new tab of the current session, labelled
    /// after its program (like `fterm run --window -l claude -- claude`), in
    /// the focused pane's directory.
    pub fn start_agent(&mut self, command_line: &str) {
        let command_line = command_line.trim();
        if command_line.is_empty() || self.client.is_none() {
            return;
        }
        let label = agents::unique_label(&agents::label_for(command_line), &self.sessions);
        let cwd = self
            .window()
            .and_then(|w| w.blocks.iter().find(|b| b.id == w.focused))
            .and_then(|b| b.cwd.clone())
            .or_else(|| self.default_cwd.as_ref().map(|d| d.display().to_string()));
        self.send(Request::NewWindow {
            session: self.session().map(|s| s.id.to_string()),
            name: Some(label.clone()),
            spawn: Spawn {
                command: agents::shell_command(command_line),
                cwd,
                label: Some(label),
                ..spawn_env()
            },
        });
    }

    /// The directory new tabs start in: the focused pane's, else the view's.
    fn current_cwd(&self) -> Option<String> {
        self.window()
            .and_then(|w| w.blocks.iter().find(|b| b.id == w.focused))
            .and_then(|b| b.cwd.clone())
            .or_else(|| self.default_cwd.as_ref().map(|d| d.display().to_string()))
    }

    /// "New terminal in this folder": a tab (a session when tabs run across
    /// the top) in the folder's directory, else the current one, put into
    /// the folder once the server created it.
    pub fn new_terminal_in_folder(&mut self, folder: u64) {
        let Some(f) = self.folders.get(folder) else {
            return;
        };
        let cwd = f
            .path
            .as_deref()
            .map(expand_tilde)
            .or_else(|| self.current_cwd());
        let spawn = Spawn {
            cwd,
            ..self.spawn_here()
        };
        let sessions = crate::settings::get().tab_position == crate::settings::TabPosition::Top;
        let request = if sessions {
            Request::NewSession { name: None, spawn }
        } else {
            Request::NewWindow {
                session: self.session().map(|s| s.id.to_string()),
                name: None,
                spawn,
            }
        };
        let Some(c) = self.client.clone() else { return };
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let event = match c.request(request).await {
                Ok(Response::Created {
                    session, window, ..
                }) => UiEvent::IntoFolder(
                    if sessions {
                        session.to_string()
                    } else {
                        window.to_string()
                    },
                    folder,
                ),
                Ok(_) => return,
                Err(e) => UiEvent::Message(e.to_string()),
            };
            let _ = proxy.unbounded_send(event);
        });
    }

    /// A new tab with a shell (⌘T).
    pub fn new_terminal(&mut self) {
        self.shortcut(Shortcut::NewTab, None);
    }

    /// A saved tab configuration: `command` (or the shell) in a tab named `name`.
    pub fn start_tab_config(&mut self, name: &str, command: &str) {
        if self.client.is_none() {
            return;
        }
        let command = command.trim();
        let label = agents::unique_label(name, &self.sessions);
        self.send(Request::NewWindow {
            session: self.session().map(|s| s.id.to_string()),
            name: Some(label.clone()),
            spawn: Spawn {
                command: if command.is_empty() {
                    Vec::new()
                } else {
                    agents::shell_command(command)
                },
                cwd: self.current_cwd(),
                label: Some(label),
                ..spawn_env()
            },
        });
    }

    /// `command_line` on SSH host `host`, in a new tab ("cloud agent").
    pub fn start_remote_agent(&mut self, host: &str, command_line: &str) {
        let Some(found) = ferrumterm_net::hosts::find_host(&self.ssh.store, &self.ssh.config, host)
        else {
            self.notify(format!("{host} is no longer configured"));
            return;
        };
        match ferrumterm_net::args::remote_command(
            &ferrumterm_net::args::ssh_binary(),
            &found,
            command_line,
        ) {
            Ok(command) => {
                let label = agents::unique_label(
                    &format!("{}@{host}", agents::label_for(command_line)),
                    &self.sessions,
                );
                self.send(Request::NewWindow {
                    session: self.session().map(|s| s.id.to_string()),
                    name: Some(label.clone()),
                    spawn: Spawn {
                        command,
                        label: Some(label),
                        ..spawn_env()
                    },
                });
            }
            Err(e) => self.notify(e),
        }
    }

    /// Creates a git worktree for `branch` next to the current repository
    /// (`<repo>-<branch>`, a new branch if it doesn't exist) and opens a
    /// terminal in it.
    pub fn new_worktree(&mut self, branch: &str) {
        let branch = branch.trim();
        let Some(cwd) = self.current_cwd() else {
            self.notify("Open a terminal in a git repository first");
            return;
        };
        match crate::worktree::create(std::path::Path::new(&cwd), branch) {
            Ok(dir) => {
                let label = agents::unique_label(branch, &self.sessions);
                self.send(Request::NewWindow {
                    session: self.session().map(|s| s.id.to_string()),
                    name: Some(label.clone()),
                    spawn: Spawn {
                        cwd: Some(dir.display().to_string()),
                        label: Some(label),
                        ..spawn_env()
                    },
                });
                self.notify(format!("Worktree {} created", dir.display()));
            }
            Err(e) => self.notify(e),
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
