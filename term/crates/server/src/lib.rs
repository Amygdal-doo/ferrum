//! The FerrumTerm server: a persistent multiplexer that owns every terminal.
//!
//! Clients come and go — the GPU app, `fterm` commands, `fterm attach` over
//! SSH, agents — while sessions and their programs keep running. Like tmux
//! and Rex, closing a window only disconnects a client.

mod block;
mod conn;
mod forward;
mod listen;
mod ops;
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
use std::sync::atomic::{AtomicUsize, Ordering};
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
    pub(crate) exits: Mutex<VecDeque<(BlockId, i32)>>,
    pub(crate) internal: mpsc::UnboundedSender<Internal>,
    pub(crate) awake: Mutex<system::Awake>,
    pub(crate) listeners: Mutex<Vec<String>>,
    pub(crate) forwards: Mutex<ForwardTable>,
    pub(crate) shutdown: Notify,
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
            version: env!("CARGO_PKG_VERSION").to_owned(),
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
                self.set_status(id, status);
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
    ) {
        let (block, label) = {
            let state = lock(&self.state);
            let Some(b) = state.blocks.get(&id).cloned() else {
                return;
            };
            let label = lock(&b.label).clone().unwrap_or_else(|| b.title());
            (b, label)
        };
        let status = status.map(|mut s| {
            s.updated_ms = now_ms();
            s
        });
        let previous = std::mem::replace(&mut *lock(&block.status), status.clone());
        if let Some(s) = &status
            && self.config.notify
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
    let server = Arc::new(Server {
        state: Mutex::new(State::default()),
        events,
        token,
        started_ms: now_ms(),
        clients: AtomicUsize::new(0),
        exits: Mutex::new(VecDeque::new()),
        internal,
        awake: Mutex::new(awake),
        listeners: Mutex::new(Vec::new()),
        forwards: Mutex::new(ForwardTable::default()),
        shutdown: Notify::new(),
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

    tokio::select! {
        () = server.shutdown.notified() => {}
        () = stop => {}
    }
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
