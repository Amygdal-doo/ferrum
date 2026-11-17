//! The FerrumTerm server: a persistent multiplexer that owns every terminal.
//!
//! Clients come and go — the GPU app, `fterm` commands, `fterm attach` over
//! SSH, agents — while sessions and their programs keep running. Like tmux
//! and Rex, closing a window only disconnects a client.

mod block;
mod conn;
mod detect;
mod forward;
mod listen;
mod ops;
pub mod shell;
mod state;
mod system;

use forward::ForwardTable;
pub use listen::{check_listen_addr, load_or_create_token};
pub use state::{DEFAULT_COLS, DEFAULT_ROWS};

use block::Internal;
pub(crate) use block::lock;
use ferrumterm_protocol::status::StatusUpdate;
use ferrumterm_protocol::{AwakeMode, BlockId, Event, ProgramState, ServerInfo};
use state::State;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{Notify, broadcast, mpsc};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Holds the token, pid file and logs.
    pub state_dir: PathBuf,
    pub socket: PathBuf,
    /// TCP listeners (loopback or tailnet addresses only; token required).
    pub tcp: Vec<SocketAddr>,
    pub awake: AwakeMode,
    /// macOS notifications when a block reports done / blocked / error.
    pub notify: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Config(String),
}

/// Recently exited blocks remembered for `wait` (bounded).
const EXIT_MEMORY: usize = 512;

pub struct Server {
    pub(crate) state: Mutex<State>,
    pub(crate) events: broadcast::Sender<Event>,
    pub(crate) config: ServerConfig,
    pub(crate) token: String,
    pub(crate) started_ms: u64,
    pub(crate) clients: AtomicUsize,
    /// Connected FerrumTerm windows ([`ferrumterm_protocol::WINDOW_CLIENT`]):
    /// while there is one, it posts agent notifications instead of the server.
    pub(crate) windows: AtomicUsize,
    /// Set when shutting down (stops the detection thread).
    pub(crate) stopping: AtomicBool,
    /// Wakes the detection thread (agent output, Enter typed).
    pub(crate) wake: Arc<detect::Wake>,
    pub(crate) exits: Mutex<VecDeque<(BlockId, i32)>>,
    pub(crate) internal: mpsc::UnboundedSender<Internal>,
    pub(crate) awake: Mutex<system::Awake>,
    pub(crate) listeners: Mutex<Vec<String>>,
    pub(crate) forwards: Mutex<ForwardTable>,
    pub(crate) shutdown: Notify,
    /// Where the shell integration scripts are (see `shell`), once written.
    pub(crate) shell_dir: Option<PathBuf>,
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

impl Server {
    pub(crate) fn emit(&self, e: Event) {
        let _ = self.events.send(e);
    }

    pub(crate) fn info(&self) -> ServerInfo {
        // Read each lock into a local: two guards on one mutex in one
        // expression would deadlock.
        let (awake_mode, awake) = {
            let a = lock(&self.awake);
            (a.mode, a.active())
        };
        let listeners = lock(&self.listeners).clone();
        let state = lock(&self.state);
        ServerInfo {
            version: ferrumterm_protocol::build_version().to_owned(),
            revision: ferrumterm_protocol::SERVER_REVISION,
            protocol: ferrumterm_protocol::PROTOCOL_VERSION,
            pid: std::process::id(),
            started_ms: self.started_ms,
            socket: self.config.socket.display().to_string(),
            listeners,
            sessions: state.sessions.len(),
            blocks: state.blocks.len(),
            clients: self.clients.load(Ordering::Relaxed),
            awake_mode,
            awake,
        }
    }

    /// Re-evaluates the keep-awake assertion.
    pub(crate) fn update_awake(&self) {
        let working = lock(&self.state).blocks.values().any(|b| {
            lock(&b.status)
                .as_ref()
                .is_some_and(|s| s.state == ProgramState::Working)
        });
        lock(&self.awake).update(working);
    }

    fn handle_internal(&self, msg: Internal) {
        match msg {
            Internal::Status(id, update) => {
                let status = match update {
                    StatusUpdate::Set(s) => Some(s),
                    StatusUpdate::Clear => None,
                };
                self.set_status(id, status, StatusSource::Program);
            }
            Internal::Title(block) => {
                tracing::trace!(%block, "title changed");
                self.emit(Event::Changed);
            }
            Internal::Bell(block) => self.emit(Event::Bell { block }),
            Internal::Exited(id, code) => self.block_exited(id, code),
        }
    }

    pub(crate) fn set_status(
        &self,
        id: BlockId,
        status: Option<ferrumterm_protocol::ProgramStatus>,
        source: StatusSource,
    ) {
        let (block, label) = {
            let state = lock(&self.state);
            let Some(b) = state.blocks.get(&id).cloned() else {
                return;
            };
            let label = lock(&b.label)
                .clone()
                .or_else(|| lock(&b.agent).clone())
                .unwrap_or_else(|| b.title());
            (b, label)
        };
        {
            let mut d = lock(&block.detect);
            match source {
                StatusSource::Program => {
                    d.explicit = true;
                    d.detected = false;
                }
                StatusSource::Detected if d.explicit => return,
                StatusSource::Detected => d.detected = status.is_some(),
            }
        }
        let status = status.map(|mut s| {
            s.updated_ms = now_ms();
            s
        });
        let previous = std::mem::replace(&mut *lock(&block.status), status.clone());
        if let Some(s) = &status
            && self.config.notify
            && self.windows.load(Ordering::Relaxed) == 0
            && matches!(
                s.state,
                ProgramState::Done | ProgramState::Blocked | ProgramState::Error
            )
            && previous.as_ref().map(|p| p.state) != Some(s.state)
        {
            system::notify(id, &label, s);
        }
        self.emit(Event::Status { block: id, status });
        self.update_awake();
    }

    /// One round of agent status detection (see `detect`) over every block;
    /// true while detection says an agent is working (its turn ends on a
    /// timer, so look again soon).
    pub(crate) fn detect_tick(&self) -> bool {
        let blocks: Vec<Arc<block::Block>> = lock(&self.state).blocks.values().cloned().collect();
        let mut changed = false;
        let mut working = false;
        for b in blocks {
            if lock(&b.exited).is_some() {
                continue;
            }
            changed |= Self::update_agent(&b);
            working |= self.detect_block(&b) == Some(ProgramState::Working);
        }
        if changed {
            self.emit(Event::Changed);
        }
        working
    }

    /// Looks up the foreground program when it changed; true when the
    /// block's agent changed.
    fn update_agent(b: &block::Block) -> bool {
        let fg = b.foreground();
        {
            let mut d = lock(&b.detect);
            if d.foreground == fg {
                return false;
            }
            d.foreground = fg;
            d.explicit = false;
            d.busy_at = None;
            // The Enter that started it is not a prompt to it.
            d.submitted_at = None;
        }
        let agent = fg
            .and_then(detect::process_command)
            .and_then(|line| ferrumterm_protocol::agents::agent_from_command_line(&line))
            .or_else(|| {
                // `ps` failed: the program the block started, while it runs.
                fg.filter(|pid| b.pid.is_some_and(|own| own as i32 == *pid))
                    .and_then(|_| ferrumterm_protocol::agents::agent_of_command(&b.command))
            })
            .map(str::to_owned);
        lock(&b.detect).indicates = agent.as_deref().is_some_and(detect::shows_indicator);
        let mut current = lock(&b.agent);
        if *current == agent {
            return false;
        }
        *current = agent;
        true
    }

    /// Classifies one block; returns the state detection gave it.
    fn detect_block(&self, b: &block::Block) -> Option<ProgramState> {
        let detected = lock(&b.detect).detected;
        if lock(&b.agent).is_none() {
            // The agent left the foreground: drop what detection said.
            if detected {
                self.set_status(b.id, None, StatusSource::Detected);
            }
            return None;
        }
        if lock(&b.detect).explicit {
            return None;
        }
        let (screen, title) = {
            let term = lock(&b.term);
            (term.capture(false), term.title().to_owned())
        };
        let quiet_ms = lock(&b.last_output).map(|at| at.elapsed().as_millis() as u64);
        let signal = detect::observe(&detect::Observation {
            screen: &screen,
            title: &title,
            quiet_ms,
        });
        let clock = {
            let mut d = lock(&b.detect);
            // Once an agent shows an indicator, it is what tells work apart.
            d.indicates |= signal == detect::Signal::Busy;
            let now = std::time::Instant::now();
            let mut clock = detect::Clock {
                indicates: d.indicates,
                ..detect::Clock::default()
            };
            if clock.is_busy(&signal) {
                d.busy_at = Some(now);
            }
            let ms = |at: Option<std::time::Instant>| {
                at.map(|at| now.saturating_duration_since(at).as_millis() as u64)
            };
            clock.since_busy_ms = ms(d.busy_at);
            clock.since_submit_ms = ms(d.submitted_at);
            clock
        };
        let current = lock(&b.status).clone();
        let previous = current.as_ref().filter(|_| detected).map(|s| s.state);
        let next = detect::step(previous, &signal, &clock);
        let mut status = ferrumterm_protocol::ProgramStatus::new(next);
        if let detect::Signal::Question { line, kind } = signal {
            status.kind = Some(kind.to_owned());
            status.message = Some(line);
        }
        let same = current
            .as_ref()
            .is_some_and(|c| detected && c.state == status.state && c.message == status.message);
        if !same {
            self.set_status(b.id, Some(status), StatusSource::Detected);
        }
        Some(next)
    }

    fn block_exited(&self, id: BlockId, code: i32) {
        {
            let mut exits = lock(&self.exits);
            exits.push_back((id, code));
            if exits.len() > EXIT_MEMORY {
                exits.pop_front();
            }
        }
        let keep = {
            let state = lock(&self.state);
            state.blocks.get(&id).map(|b| {
                *lock(&b.exited) = Some(code);
                // Transient states end with the process; done/error stay.
                let mut st = lock(&b.status);
                if st.as_ref().is_some_and(|s| !s.state.sticky()) {
                    *st = None;
                }
                b.keep_open
            })
        };
        match keep {
            None => {}
            Some(true) => self.emit(Event::Changed),
            Some(false) => {
                ops::remove_block(self, id);
            }
        }
        self.emit(Event::Exited { block: id, code });
        self.update_awake();
    }
}

/// Who set a block's status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusSource {
    /// The program itself (OSC 7501, `fterm status`, hooks): always wins.
    Program,
    /// Guessed by `detect`.
    Detected,
}

/// Runs the server until `Shutdown` or a termination signal.
pub async fn run(config: ServerConfig) -> Result<(), ServerError> {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    // Survive the terminal that started us closing: handling SIGHUP replaces its default (exit).
    let mut hup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())?;
    tokio::spawn(async move { while hup.recv().await.is_some() {} });
    run_until(config, async move {
        tokio::select! {
            _ = term.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
    })
    .await
}

/// Runs the server until `Shutdown` or until `stop` completes, without
/// touching the process's signal handlers. For hosting a server inside
/// another program (the embedded terminal's private fallback server).
pub async fn run_until(
    config: ServerConfig,
    stop: impl std::future::Future<Output = ()>,
) -> Result<(), ServerError> {
    std::fs::create_dir_all(&config.state_dir)?;
    let token = load_or_create_token(&config.state_dir)?;
    let (internal, mut internal_rx) = mpsc::unbounded_channel();
    let (events, _) = broadcast::channel(4096);
    let awake = system::Awake::new(config.awake);
    let shell_dir = config.state_dir.join("shell-integration");
    let shell_dir = match shell::install(&shell_dir) {
        Ok(()) => Some(shell_dir),
        Err(error) => {
            tracing::warn!(%error, "could not write the shell integration scripts");
            None
        }
    };
    let server = Arc::new(Server {
        state: Mutex::new(State::default()),
        events,
        token,
        started_ms: now_ms(),
        clients: AtomicUsize::new(0),
        windows: AtomicUsize::new(0),
        stopping: AtomicBool::new(false),
        wake: Arc::default(),
        exits: Mutex::new(VecDeque::new()),
        internal,
        awake: Mutex::new(awake),
        listeners: Mutex::new(Vec::new()),
        forwards: Mutex::new(ForwardTable::default()),
        shutdown: Notify::new(),
        shell_dir,
        config,
    });
    server.update_awake();
    listen::start(server.clone()).await?;
    tracing::info!(socket = %server.config.socket.display(), "FerrumTerm server ready");

    let s = server.clone();
    tokio::spawn(async move {
        while let Some(msg) = internal_rx.recv().await {
            s.handle_internal(msg);
        }
    });

    // Agent status detection keeps running with no client connected. It
    // looks when an agent prints or Enter is typed, every `WORKING_TICK`
    // while one works (to end its turn on time) and else once a second.
    let weak = Arc::downgrade(&server);
    let wake = server.wake.clone();
    std::thread::Builder::new()
        .name("fterm-detect".into())
        .spawn(move || {
            let mut tick = detect::IDLE_TICK;
            loop {
                if wake.wait(tick) {
                    std::thread::sleep(detect::SETTLE);
                }
                let Some(server) = weak.upgrade() else { break };
                if server.stopping.load(Ordering::Relaxed) {
                    break;
                }
                tick = if server.detect_tick() {
                    detect::WORKING_TICK
                } else {
                    detect::IDLE_TICK
                };
            }
        })?;

    tokio::select! {
        () = server.shutdown.notified() => {}
        () = stop => {}
    }
    server.stopping.store(true, Ordering::Relaxed);
    tracing::info!("shutting down");
    server.emit(Event::Bye);
    for b in lock(&server.state).blocks.values() {
        b.kill();
    }
    lock(&server.forwards).stop_all();
    lock(&server.awake).update(false);
    let _ = std::fs::remove_file(&server.config.socket);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    Ok(())
}
