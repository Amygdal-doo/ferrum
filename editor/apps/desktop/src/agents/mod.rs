//! AI agent sessions: which agents exist and where their programs are,
//! the sessions of the open project (each a terminal tab running the
//! agent's CLI), the snapshot each took when it started, and what changed
//! since — rescanned when the file watcher sees writes. Accepting and
//! rejecting goes through [`AgentSessions::apply`]; the workspace keeps
//! open editors in step. The review UI is [`review::AgentReview`].

pub mod review;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ferrumeditor_core::agents::review::{self as core_review, FileChange, ReviewError, Written};
use ferrumeditor_core::agents::snapshot::Snapshot;
use ferrumeditor_core::agents::{self, AgentDef, AgentStatus};
use gpui::{App, Context, EventEmitter, Task};

use crate::lsp_store::LspStore;
use crate::settings::Settings;

pub use review::{AgentReview, ReviewEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentsEvent {
    /// The snapshot is taken: start the agent's terminal.
    Launch(u64),
    Notice(String),
    Error(String),
}

/// An accept / reject from the review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewAction {
    AcceptHunk { path: PathBuf, hunk: u64, hash: u64 },
    RejectHunk { path: PathBuf, hunk: u64, hash: u64 },
    AcceptFile { path: PathBuf, hash: u64 },
    RejectFile { path: PathBuf, hash: u64 },
    AcceptAll,
    RejectAll,
}

impl ReviewAction {
    /// Files (relative) the action rewrites on disk.
    pub fn writes(&self, changes: &[FileChange]) -> Vec<PathBuf> {
        match self {
            ReviewAction::RejectHunk { path, .. } | ReviewAction::RejectFile { path, .. } => {
                vec![path.clone()]
            }
            ReviewAction::RejectAll => changes.iter().map(|c| c.path.clone()).collect(),
            _ => Vec::new(),
        }
    }
}

pub struct AgentSession {
    pub id: u64,
    pub agent: AgentDef,
    pub root: PathBuf,
    /// The terminal tab running the agent.
    pub terminal: Option<u64>,
    pub snapshot: Option<Snapshot>,
    /// What changed since the snapshot, newest scan.
    pub changes: Vec<FileChange>,
    /// Files written since the last scan: their hunks are out of date.
    pub stale: BTreeSet<PathBuf>,
    pub prompt: Option<String>,
    pub stopped: bool,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    scanning: bool,
    rescan: bool,
}

impl AgentSession {
    pub fn status(&self, idle: Option<Duration>, threshold: Duration) -> AgentStatus {
        if self.terminal.is_none() && self.exit_code.is_none() && !self.stopped {
            return AgentStatus::Starting;
        }
        AgentStatus::of(
            self.exit_code,
            self.stopped,
            idle.unwrap_or_default(),
            threshold,
        )
    }

    pub fn change_counts(&self) -> (usize, usize, usize) {
        let files = self.changes.len();
        let added = self.changes.iter().map(|c| c.additions).sum();
        let removed = self.changes.iter().map(|c| c.deletions).sum();
        (files, added, removed)
    }
}

pub struct AgentSessions {
    pub sessions: Vec<AgentSession>,
    next_id: u64,
    /// PATH for finding agents (login shell, ~/.local/bin, npm global…).
    pub path_env: Option<String>,
    servers_dir: Option<PathBuf>,
    installing: Option<String>,
    tasks: Vec<Task<()>>,
    scans: std::collections::HashMap<u64, Task<()>>,
    _tick: Task<()>,
}

impl EventEmitter<AgentsEvent> for AgentSessions {}

impl AgentSessions {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let base = LspStore::global(cx).map(|s| s.read(cx).path_env());
        let servers_dir = cx
            .global::<Settings>()
            .files
            .as_ref()
            .map(|f| f.servers_dir.clone());
        let servers = servers_dir.clone();
        let path_task = cx.spawn(async move |this, cx| {
            let base = match base {
                Some(task) => task.await,
                None => std::env::var("PATH").unwrap_or_default(),
            };
            let path = cx
                .background_executor()
                .spawn(async move {
                    let home = directories_home();
                    // Tests don't ask the real npm.
                    let npm = if cfg!(test) {
                        None
                    } else {
                        agents::npm_global_bin(&base)
                    };
                    agents::search_path(&base, home.as_deref(), npm.as_deref(), servers.as_deref())
                })
                .await;
            this.update(cx, |this, cx| {
                this.path_env = Some(path);
                cx.notify();
            })
            .ok();
        });
        // Statuses move to "waiting for input" with time alone.
        let tick = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let live = this
                    .update(cx, |this, cx| {
                        if this
                            .sessions
                            .iter()
                            .any(|s| s.exit_code.is_none() && !s.stopped)
                        {
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !live {
                    break;
                }
            }
        });
        Self {
            sessions: Vec::new(),
            next_id: 1,
            path_env: None,
            servers_dir,
            installing: None,
            tasks: vec![path_task],
            scans: std::collections::HashMap::new(),
            _tick: tick,
        }
    }

    /// Built-ins plus `[[agents]]`.
    pub fn agents(cx: &App) -> Vec<AgentDef> {
        agents::all_agents(&cx.global::<Settings>().config.agents)
    }

    pub fn path(&self) -> String {
        self.path_env
            .clone()
            .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default())
    }

    pub fn locate(&self, agent: &AgentDef) -> Option<PathBuf> {
        agents::locate(agent, &self.path())
    }

    pub fn installing(&self) -> Option<&str> {
        self.installing.as_deref()
    }

    pub fn session(&self, id: u64) -> Option<&AgentSession> {
        self.sessions.iter().find(|s| s.id == id)
    }

    fn session_mut(&mut self, id: u64) -> Option<&mut AgentSession> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    pub fn by_terminal(&self, terminal: u64) -> Option<u64> {
        self.sessions
            .iter()
            .find(|s| s.terminal == Some(terminal))
            .map(|s| s.id)
    }

    /// The session to review: the newest with changes, else the newest.
    pub fn reviewable(&self) -> Option<u64> {
        self.sessions
            .iter()
            .rev()
            .find(|s| !s.changes.is_empty())
            .or_else(|| self.sessions.last())
            .map(|s| s.id)
    }

    /// Starts a session: snapshot first (in the background), then
    /// [`AgentsEvent::Launch`].
    pub fn start(
        &mut self,
        agent: AgentDef,
        root: PathBuf,
        prompt: Option<String>,
        cx: &mut Context<Self>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.sessions.push(AgentSession {
            id,
            agent,
            root: root.clone(),
            terminal: None,
            snapshot: None,
            changes: Vec::new(),
            stale: BTreeSet::new(),
            prompt,
            stopped: false,
            exit_code: None,
            error: None,
            scanning: false,
            rescan: false,
        });
        let task = cx.spawn(async move |this, cx| {
            let snapshot = cx
                .background_executor()
                .spawn(async move { Snapshot::take(&root) })
                .await;
            this.update(cx, |this, cx| {
                let Some(session) = this.session_mut(id) else {
                    return;
                };
                match snapshot {
                    Ok(snapshot) => {
                        session.snapshot = Some(snapshot);
                        cx.emit(AgentsEvent::Launch(id));
                    }
                    Err(error) => {
                        session.error = Some(error.clone());
                        session.stopped = true;
                        cx.emit(AgentsEvent::Error(format!(
                            "Could not snapshot the project: {error}"
                        )));
                    }
                }
                cx.notify();
            })
            .ok();
        });
        self.tasks.push(task);
        cx.notify();
        id
    }

    pub fn set_terminal(&mut self, id: u64, terminal: Option<u64>, cx: &mut Context<Self>) {
        if let Some(session) = self.session_mut(id) {
            session.terminal = terminal;
            session.exit_code = None;
            session.stopped = false;
        }
        cx.notify();
    }

    /// The agent's process ended: one last scan.
    pub fn exited(&mut self, terminal: u64, code: i32, cx: &mut Context<Self>) {
        let Some(id) = self.by_terminal(terminal) else {
            return;
        };
        if let Some(session) = self.session_mut(id) {
            session.exit_code = Some(code);
        }
        self.rescan(id, cx);
    }

    pub fn mark_stopped(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(session) = self.session_mut(id) {
            session.stopped = true;
        }
        self.rescan(id, cx);
    }

    pub fn remove(&mut self, id: u64, cx: &mut Context<Self>) {
        self.sessions.retain(|s| s.id != id);
        cx.notify();
    }

    /// The watcher saw writes: mark reviewed files stale and rescan.
    pub fn files_changed(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        let mut rescan = Vec::new();
        for session in &mut self.sessions {
            let Some(snapshot) = &session.snapshot else {
                continue;
            };
            let mut touched = false;
            for path in paths.iter().filter(|p| snapshot.covers(p)) {
                touched = true;
                if let Ok(rel) = path.strip_prefix(&snapshot.root)
                    && session.changes.iter().any(|c| c.path == rel)
                {
                    session.stale.insert(rel.to_path_buf());
                }
            }
            if touched {
                rescan.push(session.id);
            }
        }
        for id in rescan {
            self.rescan(id, cx);
        }
    }

    /// Recomputes the session's changes in the background (once more if
    /// asked again meanwhile).
    pub fn rescan(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(session) = self.session_mut(id) else {
            return;
        };
        let Some(snapshot) = session.snapshot.clone() else {
            return;
        };
        if session.scanning {
            session.rescan = true;
            return;
        }
        session.scanning = true;
        let task = cx.spawn(async move |this, cx| {
            let changes = cx
                .background_executor()
                .spawn(async move { core_review::changes(&snapshot) })
                .await;
            this.update(cx, |this, cx| {
                let Some(session) = this.session_mut(id) else {
                    return;
                };
                session.scanning = false;
                match changes {
                    Ok(changes) => {
                        session.changes = changes;
                        session.stale.clear();
                    }
                    Err(error) => session.error = Some(error),
                }
                if std::mem::take(&mut session.rescan) {
                    this.rescan(id, cx);
                }
                cx.notify();
            })
            .ok();
        });
        self.scans.insert(id, task);
    }

    /// Rescans right away on this thread (the review command and tests).
    pub fn rescan_now(&mut self, id: u64, cx: &mut Context<Self>) -> Result<(), String> {
        let session = self.session_mut(id).ok_or("no such session")?;
        let snapshot = session.snapshot.as_ref().ok_or("the session is starting")?;
        session.changes = core_review::changes(snapshot)?;
        session.stale.clear();
        cx.notify();
        Ok(())
    }

    /// Accepts or rejects; returns the files written (absolute) with what
    /// they now hold. Stale actions are refused and trigger a rescan.
    pub fn apply(
        &mut self,
        id: u64,
        action: &ReviewAction,
        cx: &mut Context<Self>,
    ) -> Result<Vec<(PathBuf, Written)>, String> {
        let session = self.session_mut(id).ok_or("no such session")?;
        let root = session.root.clone();
        let snapshot = session.snapshot.as_mut().ok_or("the session is starting")?;
        let mut written = Vec::new();
        let mut touched = Vec::new();
        let mut result: Result<(), ReviewError> = Ok(());
        let mut one = |snapshot: &mut Snapshot, action: &ReviewAction| -> Result<(), ReviewError> {
            match action {
                ReviewAction::AcceptHunk { path, hunk, hash } => {
                    core_review::accept_hunk_in(snapshot, path, *hunk, *hash)?;
                    touched.push(path.clone());
                }
                ReviewAction::RejectHunk { path, hunk, hash } => {
                    let w = core_review::reject_hunk_in(snapshot, path, *hunk, *hash)?;
                    written.push((root.join(path), w));
                    touched.push(path.clone());
                }
                ReviewAction::AcceptFile { path, hash } => {
                    core_review::accept_file_in(snapshot, path, *hash)?;
                    touched.push(path.clone());
                }
                ReviewAction::RejectFile { path, hash } => {
                    let w = core_review::reject_file_in(snapshot, path, *hash)?;
                    written.push((root.join(path), w));
                    touched.push(path.clone());
                }
                ReviewAction::AcceptAll | ReviewAction::RejectAll => {}
            }
            Ok(())
        };
        let all: Vec<ReviewAction> = match action {
            ReviewAction::AcceptAll => session
                .changes
                .iter()
                .map(|c| ReviewAction::AcceptFile {
                    path: c.path.clone(),
                    hash: c.current_hash,
                })
                .collect(),
            ReviewAction::RejectAll => session
                .changes
                .iter()
                .map(|c| ReviewAction::RejectFile {
                    path: c.path.clone(),
                    hash: c.current_hash,
                })
                .collect(),
            other => vec![other.clone()],
        };
        for action in &all {
            // Go on with the other files; report the first problem.
            if let Err(error) = one(snapshot, action)
                && result.is_ok()
            {
                result = Err(error);
            }
        }
        // Refresh the touched files now, the rest with a scan.
        for rel in touched {
            let change = core_review::file_change(snapshot, &rel);
            session.changes.retain(|c| c.path != rel);
            session.stale.remove(&rel);
            if let Some(change) = change {
                let at = session.changes.partition_point(|c| c.path < rel);
                session.changes.insert(at, change);
            }
        }
        if result.is_err() {
            self.rescan(id, cx);
        }
        cx.notify();
        result.map(|()| written).map_err(|e| e.to_string())
    }

    /// One-click install of an agent's npm package into the editor's
    /// tools folder (`servers/agent-<id>`).
    pub fn install(&mut self, agent: &AgentDef, cx: &mut Context<Self>) {
        let (Some(package), Some(servers)) = (agent.npm_package.clone(), self.servers_dir.clone())
        else {
            cx.emit(AgentsEvent::Notice(match &agent.install_hint {
                Some(hint) => format!("Install {} with: {hint}", agent.name),
                None => format!("{} is not installed", agent.name),
            }));
            return;
        };
        if self.installing.is_some() {
            return;
        }
        let name = agent.name.clone();
        let id = agent.id.clone();
        self.installing = Some(name.clone());
        cx.emit(AgentsEvent::Notice(format!(
            "Installing {name} ({package})…"
        )));
        let path_env = self.path();
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn({
                    let servers = servers.clone();
                    let path_env = path_env.clone();
                    async move {
                        let logs = servers.join("logs");
                        std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
                        let mut log = std::fs::File::create(logs.join(format!("agent-{id}.log")))
                            .map_err(|e| e.to_string())?;
                        ferrumeditor_core::lsp::install::Installer {
                            servers: servers.clone(),
                            path_env,
                            log: &mut log,
                        }
                        .npm_install(&servers.join(format!("agent-{id}")), &[package.as_str()])
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                this.installing = None;
                match result {
                    Ok(()) => {
                        this.path_env =
                            Some(agents::search_path(&path_env, None, None, Some(&servers)));
                        cx.emit(AgentsEvent::Notice(format!("Installed {name}")));
                    }
                    Err(error) => cx.emit(AgentsEvent::Error(format!(
                        "Could not install {name}: {error} (log: servers/logs/)"
                    ))),
                }
                cx.notify();
            })
            .ok();
        });
        self.tasks.push(task);
        cx.notify();
    }
}

fn directories_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// `path` relative to `root` for display.
pub fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}
