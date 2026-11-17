//! [`ChatSession`]: one project's chat — the current thread, the agent
//! behind it (a built-in provider + model, or an ACP agent), the mode,
//! queued messages, permission prompts, the change set with its review and
//! checkpoints, and thread history. A UI drives it with plain method calls
//! and listens to [`SessionEvent`]s on a `futures` channel.
//!
//! Work runs on the core tokio runtime ([`crate::runtime::handle`]). The
//! constructor takes the change-set snapshot and blocks: call it off the
//! UI thread.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::acp::{self, AcpAgentConfig, AcpBridge, AcpClient};
use crate::agent::{NativeAgent, TurnSink};
use crate::changeset::{ChangeSet, CheckpointId, CheckpointInfo};
use crate::context::{self, Mention};
use crate::host::{AppHost, Workspace};
use crate::mcp::{McpClient, McpConfig};
use crate::models::ModelStore;
use crate::modes::Mode;
use crate::permissions::{Approval, Decision, PermissionBroker, PermissionRequest, Permissions};
use crate::provider::ping::Pong;
use crate::provider::{self, Message, ModelInfo, Provider, StopReason};
use crate::review::FileChange;
use crate::secrets::{self, KeySource, SecretStore};
use crate::settings::AgentSettings;
use crate::store::{ThreadStore, ThreadSummary};
use crate::thread::{Attachment, Thread, ThreadEvent, title_from};
use crate::tools::{CommandRunner, ProcessRunner, Sandbox, ToolContext, ToolRegistry};

/// Which agent answers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentChoice {
    Native { provider: String, model: String },
    Acp { agent: String },
}

impl AgentChoice {
    /// `provider/model` or `acp:<id>` (stored on threads).
    pub fn key(&self) -> String {
        match self {
            AgentChoice::Native { provider, model } => format!("{provider}/{model}"),
            AgentChoice::Acp { agent } => format!("acp:{agent}"),
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        if let Some(agent) = key.strip_prefix("acp:") {
            return Some(AgentChoice::Acp {
                agent: agent.into(),
            });
        }
        let (provider, model) = key.split_once('/')?;
        Some(AgentChoice::Native {
            provider: provider.into(),
            model: model.into(),
        })
    }

    pub fn is_acp(&self) -> bool {
        matches!(self, AgentChoice::Acp { .. })
    }

    /// The FerrumAI placeholder: listed, never chosen or sent to.
    pub fn is_coming_soon(&self) -> bool {
        matches!(self, AgentChoice::Native { provider, .. }
            if crate::custom_models::is_ferrum_ai(provider))
    }
}

/// A row of the agent/model selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentOption {
    pub choice: AgentChoice,
    pub label: String,
    /// "Anthropic", "OpenAI", "External agents"…
    pub group: String,
    /// False for a placeholder that can't be chosen (FerrumAI).
    pub enabled: bool,
    /// Shown beside the label ("Coming soon").
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerStatus {
    pub name: String,
    /// Tool count, or why it failed.
    pub result: Result<usize, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// Apply to a copy of the thread with [`Thread::apply`].
    Thread(ThreadEvent),
    Permission(PermissionRequest),
    PermissionClosed(u64),
    ChangesUpdated,
    Busy(bool),
    Queue(Vec<String>),
    /// The current thread was replaced (new / opened): re-read it.
    ThreadSwitched,
    AgentChanged(AgentChoice),
    Notice(String),
    Mcp(Vec<McpServerStatus>),
    /// The saved thread list changed: a thread was saved for the first
    /// time or under a new title, renamed or deleted.
    ThreadsChanged,
}

#[derive(Clone)]
pub struct SessionConfig {
    pub project: PathBuf,
    /// `~/.ferrum/agent` (settings, permissions, MCP config, threads).
    pub home: PathBuf,
    pub settings: AgentSettings,
    pub secrets: Arc<dyn SecretStore>,
    /// `None`: the settings' default, else Anthropic's default model.
    pub agent: Option<AgentChoice>,
    pub mode: Mode,
    pub runner: Arc<dyn CommandRunner>,
    /// Used for every built-in agent request (tests, demos).
    pub provider_override: Option<Arc<dyn Provider>>,
    pub acp_agents: Vec<AcpAgentConfig>,
    pub mcp: McpConfig,
    /// `PATH` for agents and MCP servers (GUI apps lack the shell's).
    pub path_env: Option<String>,
    /// Resume this thread instead of starting a new one.
    pub thread_id: Option<String>,
    /// A code project, or none (advice and the host's tools only).
    pub workspace: Workspace,
    /// The app the chat is built into, when not a code editor.
    pub host: Option<AppHost>,
}

impl SessionConfig {
    /// Everything from `~/.ferrum/agent` and the OS keychain.
    pub fn load(project: &Path) -> Self {
        let home = crate::paths::agent_home();
        let mcp = McpConfig::load(&crate::paths::mcp_config(&home)).unwrap_or_else(|e| {
            tracing::warn!("MCP config: {e}");
            McpConfig::default()
        });
        Self {
            project: project.to_path_buf(),
            settings: AgentSettings::load(&crate::paths::settings_file(&home)),
            secrets: secrets::default_store(),
            agent: None,
            mode: Mode::Agent,
            runner: Arc::new(ProcessRunner),
            provider_override: None,
            acp_agents: acp::load_agents(&crate::paths::agents_file(&home)),
            mcp,
            path_env: None,
            thread_id: None,
            workspace: Workspace::Project,
            host: None,
            home,
        }
    }

    /// Isolated: `home` for files, memory for keys, no MCP servers.
    pub fn isolated(project: &Path, home: &Path) -> Self {
        Self {
            project: project.to_path_buf(),
            home: home.to_path_buf(),
            settings: AgentSettings::default(),
            secrets: Arc::new(secrets::MemoryStore::default()),
            agent: None,
            mode: Mode::Agent,
            runner: Arc::new(ProcessRunner),
            provider_override: None,
            acp_agents: acp::builtin_agents(),
            mcp: McpConfig::default(),
            path_env: None,
            thread_id: None,
            workspace: Workspace::Project,
            host: None,
        }
    }

    /// No project: the chat of `host`, keeping its threads in
    /// [`crate::host::app_folder`] under `home`.
    #[must_use]
    pub fn without_project(mut self, host: AppHost) -> Self {
        self.project = crate::host::app_folder(&self.home, &host.app);
        self.workspace = Workspace::Advice;
        self.host = Some(host);
        self
    }
}

/// What sibling sessions of one project share.
struct Shared {
    sandbox: Sandbox,
    project: PathBuf,
    home: PathBuf,
    settings: Arc<Mutex<AgentSettings>>,
    secrets: Arc<dyn SecretStore>,
    runner: Arc<dyn CommandRunner>,
    provider_override: Option<Arc<dyn Provider>>,
    acp_agents: Vec<AcpAgentConfig>,
    mcp: McpConfig,
    path_env: Option<String>,
    workspace: Workspace,
    host: Option<AppHost>,
    changes: Arc<Mutex<ChangeSet>>,
    permissions: Arc<Mutex<Permissions>>,
    tools: Arc<tokio::sync::Mutex<Option<Arc<ToolRegistry>>>>,
    mcp_clients: Arc<Mutex<Vec<Arc<McpClient>>>>,
}

struct Queued {
    text: String,
    mentions: Vec<Mention>,
}

struct Inner {
    me: Weak<Inner>,
    project: PathBuf,
    sandbox: Sandbox,
    home: PathBuf,
    runtime: Handle,
    agent: Mutex<AgentChoice>,
    /// Shared with sibling sessions.
    settings: Arc<Mutex<AgentSettings>>,
    mode: Arc<Mutex<Mode>>,
    thread: Mutex<Thread>,
    /// Set once the session's thread was deleted: nothing is saved again.
    discarded: AtomicBool,
    store: ThreadStore,
    /// Id and title of the thread as last saved (to tell the UI when the
    /// list changes).
    saved_as: Mutex<(String, String)>,
    changes: Arc<Mutex<ChangeSet>>,
    permissions: Arc<Mutex<Permissions>>,
    /// Open permission prompts and where their answers go.
    pending: Mutex<HashMap<u64, (PermissionRequest, oneshot::Sender<Decision>)>>,
    next_request: AtomicU64,
    events: UnboundedSender<SessionEvent>,
    turn: Mutex<Option<CancellationToken>>,
    queue: Mutex<VecDeque<Queued>>,
    secrets: Arc<dyn SecretStore>,
    runner: Arc<dyn CommandRunner>,
    provider_override: Option<Arc<dyn Provider>>,
    acp_agents: Vec<AcpAgentConfig>,
    mcp: McpConfig,
    path_env: Option<String>,
    workspace: Workspace,
    host: Option<AppHost>,
    /// Shared with sibling sessions: MCP servers connect once.
    tools: Arc<tokio::sync::Mutex<Option<Arc<ToolRegistry>>>>,
    mcp_clients: Arc<Mutex<Vec<Arc<McpClient>>>>,
    acp: tokio::sync::Mutex<Option<Arc<AcpClient>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Clone)]
pub struct ChatSession {
    inner: Arc<Inner>,
}

impl ChatSession {
    /// Opens the session: snapshots the project (blocks) and starts or
    /// resumes a thread.
    pub fn new(config: SessionConfig) -> Result<(Self, UnboundedReceiver<SessionEvent>), String> {
        if config.workspace == Workspace::Advice {
            std::fs::create_dir_all(&config.project)
                .map_err(|e| format!("{}: {e}", config.project.display()))?;
        }
        let sandbox = Sandbox::new(&config.project).map_err(|e| e.to_string())?;
        let project = sandbox.root().to_path_buf();
        let changes = match config.workspace {
            Workspace::Project => ChangeSet::take(&project)?,
            // Nothing to track (and never git: the folder may sit in a
            // repository of the home folder).
            Workspace::Advice => ChangeSet::new(crate::snapshot::Snapshot::take_files(&project)?),
        };
        let store = ThreadStore::new(&config.home, &project);
        let agent = config
            .agent
            .clone()
            .or_else(|| {
                config
                    .settings
                    .default_agent
                    .as_deref()
                    .and_then(AgentChoice::parse)
            })
            .unwrap_or_else(|| AgentChoice::Native {
                provider: "anthropic".into(),
                model: crate::settings::ANTHROPIC_MODELS[0].into(),
            });
        let mut thread = match config.thread_id.as_deref().map(|id| store.load(id)) {
            Some(Ok(thread)) => thread,
            _ => Thread::new(project.clone(), agent.key(), config.mode),
        };
        let agent = AgentChoice::parse(&thread.agent).unwrap_or(agent);
        thread.agent = agent.key();
        let mut permissions = Permissions::load(&crate::paths::permissions_file(&config.home));
        permissions.set_approval(config.settings.approval);
        let shared = Shared {
            sandbox,
            project,
            home: config.home,
            settings: Arc::new(Mutex::new(config.settings)),
            secrets: config.secrets,
            runner: config.runner,
            provider_override: config.provider_override,
            acp_agents: config.acp_agents,
            mcp: config.mcp,
            path_env: config.path_env,
            workspace: config.workspace,
            host: config.host,
            changes: Arc::new(Mutex::new(changes)),
            permissions: Arc::new(Mutex::new(permissions)),
            tools: Arc::new(tokio::sync::Mutex::new(None)),
            mcp_clients: Arc::new(Mutex::new(Vec::new())),
        };
        Ok(Self::assemble(shared, store, agent, thread))
    }

    /// Another thread of the same project that runs alongside this one:
    /// the saved thread `thread_id`, or a new thread in `mode`. It shares
    /// this session's change set (one review list and one checkpoint
    /// history for the project, no isolation between threads), its
    /// permission policy, settings and MCP tools, and has its own turn,
    /// queue, permission prompts and ACP agent process. It takes no
    /// snapshot, so it opens at once.
    pub fn sibling(
        &self,
        thread_id: Option<&str>,
        mode: Mode,
    ) -> Result<(Self, UnboundedReceiver<SessionEvent>), String> {
        let inner = &self.inner;
        let store = ThreadStore::new(&inner.home, &inner.project);
        let agent = self.agent();
        let mut thread = match thread_id {
            Some(id) => store.load(id).map_err(|e| e.to_string())?,
            None => Thread::new(inner.project.clone(), agent.key(), mode),
        };
        let agent = AgentChoice::parse(&thread.agent).unwrap_or(agent);
        thread.agent = agent.key();
        let shared = Shared {
            sandbox: inner.sandbox.clone(),
            project: inner.project.clone(),
            home: inner.home.clone(),
            settings: inner.settings.clone(),
            secrets: inner.secrets.clone(),
            runner: inner.runner.clone(),
            provider_override: inner.provider_override.clone(),
            acp_agents: inner.acp_agents.clone(),
            mcp: inner.mcp.clone(),
            path_env: inner.path_env.clone(),
            workspace: inner.workspace,
            host: inner.host.clone(),
            changes: inner.changes.clone(),
            permissions: inner.permissions.clone(),
            tools: inner.tools.clone(),
            mcp_clients: inner.mcp_clients.clone(),
        };
        Ok(Self::assemble(shared, store, agent, thread))
    }

    fn assemble(
        shared: Shared,
        store: ThreadStore,
        agent: AgentChoice,
        thread: Thread,
    ) -> (Self, UnboundedReceiver<SessionEvent>) {
        let (tx, rx) = unbounded();
        let saved_as = (thread.id.clone(), thread.title.clone());
        let inner = Arc::new_cyclic(|me| Inner {
            me: me.clone(),
            sandbox: shared.sandbox,
            home: shared.home,
            runtime: crate::runtime::handle(),
            agent: Mutex::new(agent),
            settings: shared.settings,
            mode: Arc::new(Mutex::new(thread.mode)),
            thread: Mutex::new(thread),
            discarded: AtomicBool::new(false),
            store,
            saved_as: Mutex::new(saved_as),
            changes: shared.changes,
            permissions: shared.permissions,
            pending: Mutex::new(HashMap::new()),
            next_request: AtomicU64::new(1),
            events: tx,
            turn: Mutex::new(None),
            queue: Mutex::new(VecDeque::new()),
            secrets: shared.secrets,
            runner: shared.runner,
            provider_override: shared.provider_override,
            acp_agents: shared.acp_agents,
            mcp: shared.mcp,
            path_env: shared.path_env,
            workspace: shared.workspace,
            host: shared.host,
            tools: shared.tools,
            mcp_clients: shared.mcp_clients,
            acp: tokio::sync::Mutex::new(None),
            project: shared.project,
        });
        (Self { inner }, rx)
    }

    /// Whether `other` is this same session.
    pub fn same(&self, other: &ChatSession) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    pub fn project(&self) -> &Path {
        &self.inner.project
    }

    /// A code project, or none.
    pub fn workspace(&self) -> Workspace {
        self.inner.workspace
    }

    /// A copy of the current thread.
    pub fn thread(&self) -> Thread {
        lock(&self.inner.thread).clone()
    }

    pub fn mode(&self) -> Mode {
        *lock(&self.inner.mode)
    }

    pub fn agent(&self) -> AgentChoice {
        lock(&self.inner.agent).clone()
    }

    pub fn is_busy(&self) -> bool {
        lock(&self.inner.turn).is_some()
    }

    pub fn queued(&self) -> Vec<String> {
        lock(&self.inner.queue)
            .iter()
            .map(|q| q.text.clone())
            .collect()
    }

    /// Sends a message, or queues it while a turn is running.
    pub fn send(&self, text: &str, mentions: Vec<Mention>) {
        if text.trim().is_empty() && mentions.is_empty() {
            return;
        }
        let queued = Queued {
            text: text.to_string(),
            mentions,
        };
        if self.is_busy() {
            lock(&self.inner.queue).push_back(queued);
            self.inner.emit_queue();
        } else {
            self.inner.start_turn(queued);
        }
    }

    pub fn remove_queued(&self, index: usize) {
        lock(&self.inner.queue).remove(index);
        self.inner.emit_queue();
    }

    /// Stops the running turn, answers open permission prompts with
    /// "deny" and drops queued messages.
    pub fn cancel(&self) {
        if let Some(token) = lock(&self.inner.turn).as_ref() {
            token.cancel();
        }
        for (_, (_, tx)) in lock(&self.inner.pending).drain() {
            let _ = tx.send(Decision::Deny);
        }
        lock(&self.inner.queue).clear();
        self.inner.emit_queue();
    }

    /// Stops this session for good because its thread is being deleted:
    /// like [`Self::cancel`], and the thread is never saved again (the
    /// cancelled turn would otherwise write it back as it ends).
    pub fn discard(&self) {
        self.inner.discarded.store(true, Ordering::SeqCst);
        self.cancel();
    }

    pub fn respond_permission(&self, id: u64, decision: Decision) {
        if let Some((_, tx)) = lock(&self.inner.pending).remove(&id) {
            let _ = tx.send(decision);
        }
    }

    /// The permission prompts still waiting for an answer, oldest first
    /// (a UI that starts showing this session re-reads them).
    pub fn open_permissions(&self) -> Vec<PermissionRequest> {
        let mut open: Vec<PermissionRequest> = lock(&self.inner.pending)
            .values()
            .map(|(request, _)| request.clone())
            .collect();
        open.sort_by_key(|r| r.id);
        open
    }

    pub fn set_mode(&self, mode: Mode) {
        *lock(&self.inner.mode) = mode;
        self.inner.emit_thread(ThreadEvent::ModeChanged(mode));
        let inner = self.inner.clone();
        self.inner.runtime.spawn(async move {
            let client = inner.acp.lock().await.clone();
            if let Some(client) = client
                && let Some(target) = client.mode_for(mode)
                && client.current_mode().as_deref() != Some(target.id.as_str())
            {
                let id = target.id.clone();
                if let Err(e) = client.set_mode(&id).await {
                    inner.notice(format!("{}: could not switch mode: {e}", client.agent.name));
                }
            }
        });
    }

    /// Switches the agent (refused while a turn runs).
    pub fn set_agent(&self, choice: AgentChoice) {
        if choice.is_coming_soon() {
            self.inner.notice(format!(
                "{} is coming soon.",
                crate::custom_models::FERRUM_AI_LABEL
            ));
            return;
        }
        if self.is_busy() {
            self.inner
                .notice("Stop the running turn before switching agents.".into());
            return;
        }
        *lock(&self.inner.agent) = choice.clone();
        {
            let mut thread = lock(&self.inner.thread);
            thread.agent = choice.key();
            thread.acp_session_id = None;
        }
        self.inner.drop_acp();
        self.inner.save();
        let _ = self
            .inner
            .events
            .unbounded_send(SessionEvent::AgentChanged(choice));
    }

    /// Plan mode's "Build": switches to Agent mode and asks for the plan
    /// to be carried out.
    pub fn build_plan(&self) {
        let thread = self.thread();
        let plan = thread.plan.clone().or_else(|| {
            thread
                .messages
                .iter()
                .rev()
                .find(|m| m.role == crate::provider::Role::Assistant && !m.text().is_empty())
                .map(|m| m.text())
        });
        self.set_mode(Mode::Agent);
        let text = match plan {
            Some(plan) => format!("Implement this plan:\n\n{plan}"),
            None => "Implement the plan we discussed.".into(),
        };
        self.send(&text, Vec::new());
    }

    // Threads

    pub fn threads(&self) -> Vec<ThreadSummary> {
        self.inner.store.list()
    }

    pub fn new_thread(&self) {
        self.new_thread_in(self.mode());
    }

    /// A new thread in `mode` (the current one is saved first).
    pub fn new_thread_in(&self, mode: Mode) {
        self.cancel();
        self.inner.save();
        *lock(&self.inner.mode) = mode;
        let thread = Thread::new(self.inner.project.clone(), self.agent().key(), mode);
        *lock(&self.inner.thread) = thread;
        self.inner.drop_acp();
        let _ = self
            .inner
            .events
            .unbounded_send(SessionEvent::ThreadSwitched);
    }

    pub fn open_thread(&self, id: &str) -> Result<(), String> {
        let thread = self.inner.store.load(id).map_err(|e| e.to_string())?;
        self.cancel();
        self.inner.save();
        if let Some(agent) = AgentChoice::parse(&thread.agent) {
            *lock(&self.inner.agent) = agent;
        }
        *lock(&self.inner.mode) = thread.mode;
        *lock(&self.inner.thread) = thread;
        self.inner.drop_acp();
        let _ = self
            .inner
            .events
            .unbounded_send(SessionEvent::ThreadSwitched);
        Ok(())
    }

    pub fn delete_thread(&self, id: &str) -> Result<(), String> {
        self.inner.store.delete(id).map_err(|e| e.to_string())?;
        if lock(&self.inner.thread).id == id {
            // Not saved again on the way out.
            lock(&self.inner.thread).messages.clear();
            self.new_thread();
        }
        self.inner.threads_changed();
        Ok(())
    }

    /// Retitles a thread: the current one (saved if it has messages) or a
    /// saved one.
    pub fn rename_thread(&self, id: &str, title: &str) -> Result<(), String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("The title is empty.".into());
        }
        if lock(&self.inner.thread).id == id {
            self.inner
                .emit_thread(ThreadEvent::TitleChanged(title.to_string()));
            self.inner.save();
        } else {
            self.inner
                .store
                .rename(id, title)
                .map_err(|e| e.to_string())?;
        }
        self.inner.threads_changed();
        Ok(())
    }

    // Review and checkpoints (blocking: git and disk)

    pub fn changes(&self) -> Result<Vec<FileChange>, String> {
        lock(&self.inner.changes).changes()
    }

    fn review<T>(&self, f: impl FnOnce(&mut ChangeSet) -> Result<T, String>) -> Result<T, String> {
        let result = f(&mut lock(&self.inner.changes));
        let _ = self
            .inner
            .events
            .unbounded_send(SessionEvent::ChangesUpdated);
        result
    }

    pub fn accept_hunk(&self, path: &Path, hunk: u64, hash: u64) -> Result<(), String> {
        self.review(|c| c.accept_hunk(path, hunk, hash).map_err(|e| e.to_string()))
    }

    pub fn reject_hunk(&self, path: &Path, hunk: u64, hash: u64) -> Result<(), String> {
        self.review(|c| {
            c.reject_hunk(path, hunk, hash)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
    }

    pub fn accept_file(&self, path: &Path, hash: u64) -> Result<(), String> {
        self.review(|c| c.accept_file(path, hash).map_err(|e| e.to_string()))
    }

    pub fn reject_file(&self, path: &Path, hash: u64) -> Result<(), String> {
        self.review(|c| {
            c.reject_file(path, hash)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
    }

    pub fn accept_all(&self) -> Result<usize, String> {
        self.review(ChangeSet::accept_all)
    }

    pub fn reject_all(&self) -> Result<usize, String> {
        self.review(ChangeSet::reject_all)
    }

    pub fn checkpoints(&self) -> Vec<CheckpointInfo> {
        lock(&self.inner.changes).checkpoints()
    }

    /// Puts the files back as they were before the user message
    /// `message_id` was sent. The conversation is kept; the restore can be
    /// undone until the next turn.
    pub fn restore_checkpoint(&self, message_id: u64) -> Result<Vec<PathBuf>, String> {
        let checkpoint = lock(&self.inner.thread)
            .messages
            .iter()
            .find(|m| m.id == message_id)
            .and_then(|m| m.checkpoint)
            .ok_or("that message has no checkpoint")?;
        self.restore_to_checkpoint(checkpoint)
    }

    /// Puts the files back as they were before the turn that started
    /// `checkpoint` (one of [`ChatSession::checkpoints`], of any thread of
    /// this session); later checkpoints are dropped. Undoable like
    /// [`ChatSession::restore_checkpoint`].
    pub fn restore_to_checkpoint(&self, checkpoint: CheckpointId) -> Result<Vec<PathBuf>, String> {
        if self.is_busy() {
            return Err("Stop the running turn first.".into());
        }
        let files = self.review(|c| c.restore(checkpoint).map_err(|e| e.to_string()))?;
        self.inner.notice(format!(
            "Restored {} file{} to before that message.",
            files.len(),
            if files.len() == 1 { "" } else { "s" }
        ));
        Ok(files)
    }

    pub fn can_undo_restore(&self) -> bool {
        lock(&self.inner.changes).can_undo_restore()
    }

    pub fn undo_restore(&self) -> Result<Vec<PathBuf>, String> {
        self.review(|c| c.undo_restore().map_err(|e| e.to_string()))
    }

    // Agents, models, settings, keys

    /// The settings, with the providers as last saved by any app.
    pub fn settings(&self) -> AgentSettings {
        self.inner.refresh_settings();
        lock(&self.inner.settings).clone()
    }

    /// Providers, models and keys, sharing this session's settings.
    pub fn models(&self) -> ModelStore {
        ModelStore::new(
            self.inner.home.clone(),
            self.inner.secrets.clone(),
            self.inner.settings.clone(),
            self.inner.provider_override.clone(),
            self.inner.acp_agents.clone(),
        )
    }

    /// Saves provider settings; the approval level is kept as it is (see
    /// [`Self::set_approval`]).
    pub fn update_settings(&self, mut settings: AgentSettings) -> Result<(), String> {
        settings.approval = self.approval();
        settings
            .save(&crate::paths::settings_file(&self.inner.home))
            .map_err(|e| e.to_string())?;
        *lock(&self.inner.settings) = settings;
        Ok(())
    }

    pub fn approval(&self) -> Approval {
        lock(&self.inner.permissions).approval()
    }

    /// Changes what runs without asking, now and in the saved settings.
    pub fn set_approval(&self, approval: Approval) -> Result<(), String> {
        lock(&self.inner.permissions).set_approval(approval);
        let mut settings = self.settings();
        settings.approval = approval;
        let saved = settings
            .save(&crate::paths::settings_file(&self.inner.home))
            .map_err(|e| e.to_string());
        *lock(&self.inner.settings) = settings;
        saved
    }

    pub fn acp_agents(&self) -> &[AcpAgentConfig] {
        &self.inner.acp_agents
    }

    /// FerrumAI (disabled), built-in providers' configured models, then
    /// external agents.
    pub fn agent_options(&self) -> Vec<AgentOption> {
        crate::models::agent_options(&self.settings(), &self.inner.acp_agents)
    }

    /// Where a provider's key comes from, and its hint. Blocking.
    pub fn key_status(&self, provider_id: &str) -> Option<(KeySource, String)> {
        self.models().key_status(provider_id)
    }

    /// Stores a key in the keychain (empty: removes it). Blocking.
    pub fn set_api_key(&self, provider_id: &str, key: &str) -> Result<(), String> {
        self.models().set_api_key(provider_id, key)
    }

    /// The models a provider reports (merged with the configured ones by
    /// the caller). Runs on the core runtime; await from any executor.
    pub fn fetch_models(
        &self,
        provider_id: &str,
    ) -> impl std::future::Future<Output = Result<Vec<ModelInfo>, String>> + Send + 'static {
        self.models().fetch_models(provider_id)
    }

    /// Sends a model a tiny request: its latency, or what is wrong.
    pub fn ping(
        &self,
        provider_id: &str,
        model: &str,
    ) -> impl std::future::Future<Output = Result<Pong, String>> + Send + 'static {
        self.models().ping(provider_id, model)
    }
}

impl Inner {
    fn arc(&self) -> Option<Arc<Inner>> {
        self.me.upgrade()
    }

    fn emit_thread(&self, event: ThreadEvent) {
        lock(&self.thread).apply(&event);
        let _ = self.events.unbounded_send(SessionEvent::Thread(event));
    }

    fn notice(&self, message: String) {
        let _ = self.events.unbounded_send(SessionEvent::Notice(message));
    }

    fn emit_queue(&self) {
        let queue = lock(&self.queue).iter().map(|q| q.text.clone()).collect();
        let _ = self.events.unbounded_send(SessionEvent::Queue(queue));
    }

    fn save(&self) {
        if self.discarded.load(Ordering::SeqCst) {
            return;
        }
        let thread = lock(&self.thread).clone();
        if thread.messages.is_empty() {
            return;
        }
        if let Err(e) = self.store.save(&thread) {
            tracing::warn!("saving thread: {e}");
            return;
        }
        let key = (thread.id, thread.title);
        if std::mem::replace(&mut *lock(&self.saved_as), key.clone()) != key {
            self.threads_changed();
        }
    }

    fn threads_changed(&self) {
        let _ = self.events.unbounded_send(SessionEvent::ThreadsChanged);
    }

    fn drop_acp(&self) {
        let Some(inner) = self.arc() else { return };
        self.runtime.spawn(async move {
            if let Some(client) = inner.acp.lock().await.take() {
                client.shutdown().await;
            }
        });
    }

    /// Takes over the providers and default agent another app (or this
    /// one's settings sheet) saved; the approval level stays this
    /// session's.
    fn refresh_settings(&self) {
        let Some(saved) = AgentSettings::read(&crate::paths::settings_file(&self.home)) else {
            return;
        };
        let mut settings = lock(&self.settings);
        settings.providers = saved.providers;
        settings.default_agent = saved.default_agent;
    }

    fn provider(&self, provider_id: &str) -> Result<Arc<dyn Provider>, String> {
        if crate::custom_models::is_ferrum_ai(provider_id) {
            return Err(format!(
                "{} is coming soon.",
                crate::custom_models::FERRUM_AI_LABEL
            ));
        }
        if let Some(provider) = &self.provider_override {
            return Ok(provider.clone());
        }
        self.refresh_settings();
        let settings = lock(&self.settings).clone();
        let provider = settings
            .provider(provider_id)
            .ok_or_else(|| format!("unknown provider `{provider_id}`"))?;
        provider::build(provider, self.secrets.as_ref()).map_err(|e| e.to_string())
    }

    fn start_turn(&self, queued: Queued) {
        let Some(inner) = self.arc() else { return };
        let token = CancellationToken::new();
        *lock(&self.turn) = Some(token.clone());
        let _ = self.events.unbounded_send(SessionEvent::Busy(true));
        self.runtime.spawn(async move {
            let stop = inner.run_turn(queued, &token).await;
            inner.emit_thread(ThreadEvent::TurnEnded(stop));
            *lock(&inner.turn) = None;
            inner.save();
            let _ = inner.events.unbounded_send(SessionEvent::ChangesUpdated);
            let next = lock(&inner.queue).pop_front();
            match next {
                Some(next) => {
                    inner.emit_queue();
                    inner.start_turn(next);
                }
                None => {
                    let _ = inner.events.unbounded_send(SessionEvent::Busy(false));
                }
            }
        });
    }

    async fn run_turn(self: &Arc<Self>, queued: Queued, token: &CancellationToken) -> StopReason {
        let mut mentions = queued.mentions;
        for m in context::parse_inline(&self.project, &queued.text) {
            if !mentions.contains(&m) {
                mentions.push(m);
            }
        }
        let http = provider::http_client();
        let mut attachments = Vec::new();
        for mention in &mentions {
            attachments.push(context::resolve(mention, &self.sandbox, &http).await);
        }
        let files: Vec<PathBuf> = mentions.iter().flat_map(Mention::files).collect();
        let checkpoint = lock(&self.changes).begin_checkpoint(title_from(&queued.text));
        let id = lock(&self.thread).next_message_id();
        self.emit_thread(ThreadEvent::UserMessage {
            id,
            text: queued.text.clone(),
            attachments: attachments.clone(),
            checkpoint: Some(checkpoint),
        });
        self.save();
        let mode = *lock(&self.mode);
        let agent = lock(&self.agent).clone();
        match agent {
            AgentChoice::Native { provider, model } => {
                self.run_native(&provider, &model, mode, &files, token)
                    .await
            }
            AgentChoice::Acp { agent } => {
                // An external agent has its own system prompt: what the
                // user sees in the host app goes along with the message.
                let context = self.host.as_ref().map(|h| (h.app.clone(), h.context.get()));
                if let Some((app, text)) = context.filter(|(_, text)| !text.trim().is_empty()) {
                    attachments.push(Attachment {
                        label: format!("{app}: what the user sees"),
                        text,
                    });
                }
                self.run_acp(&agent, &queued.text, &attachments, token)
                    .await
            }
        }
    }

    fn tool_context(&self) -> ToolContext {
        ToolContext {
            sandbox: self.sandbox.clone(),
            changes: self.changes.clone(),
            runner: self.runner.clone(),
        }
    }

    fn broker(self: &Arc<Self>) -> Arc<dyn PermissionBroker> {
        Arc::new(SessionBroker(Arc::downgrade(self)))
    }

    /// Built-ins plus MCP tools (servers connect on first use).
    async fn tools(self: &Arc<Self>) -> Arc<ToolRegistry> {
        let mut slot = self.tools.lock().await;
        if let Some(tools) = slot.as_ref() {
            return tools.clone();
        }
        let mut registry = ToolRegistry::new().with_host(
            self.host
                .as_ref()
                .map(|h| h.tools.clone())
                .unwrap_or_default(),
        );
        if self.workspace == Workspace::Advice {
            registry = registry.without_files();
        }
        let mut status = Vec::new();
        for (name, config) in self.mcp.enabled() {
            let mut config = config.clone();
            if let Some(path) = &self.path_env {
                config
                    .env
                    .entry("PATH".into())
                    .or_insert_with(|| path.clone());
            }
            let connect = async {
                let client = McpClient::connect(name, &config, &self.project).await?;
                let tools = client.list_tools().await?;
                Ok::<_, crate::mcp::McpError>((client, tools))
            };
            let result =
                match tokio::time::timeout(std::time::Duration::from_secs(30), connect).await {
                    Ok(Ok((client, tools))) => {
                        let count = tools.len();
                        registry.add_mcp(name, client.clone(), tools);
                        lock(&self.mcp_clients).push(client);
                        Ok(count)
                    }
                    Ok(Err(e)) => Err(e.to_string()),
                    Err(_) => Err("timed out".into()),
                };
            status.push(McpServerStatus {
                name: name.clone(),
                result,
            });
        }
        if !status.is_empty() {
            let _ = self.events.unbounded_send(SessionEvent::Mcp(status));
        }
        let tools = Arc::new(registry);
        *slot = Some(tools.clone());
        tools
    }

    async fn run_native(
        self: &Arc<Self>,
        provider_id: &str,
        model: &str,
        mode: Mode,
        files: &[PathBuf],
        token: &CancellationToken,
    ) -> StopReason {
        let inner = self.clone();
        let id = provider_id.to_string();
        let provider = tokio::task::spawn_blocking(move || inner.provider(&id))
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        let provider = match provider {
            Ok(p) => p,
            Err(e) => {
                self.emit_thread(ThreadEvent::Error(e.clone()));
                return StopReason::Error(e);
            }
        };
        let max_tokens = lock(&self.settings)
            .provider(provider_id)
            .map_or(16_000, |p| p.max_tokens);
        let root = self.project.clone();
        let rules = tokio::task::spawn_blocking(move || crate::rules::load(&root))
            .await
            .unwrap_or_default();
        let agent = NativeAgent {
            provider,
            model: model.to_string(),
            max_tokens,
            tools: self.tools().await,
            ctx: self.tool_context(),
            broker: self.broker(),
            permissions: self.permissions.clone(),
            rules,
            host: self.host.clone(),
            workspace: self.workspace,
        };
        let sink = Sink(self.clone());
        agent.run_turn(&sink, mode, files, token).await
    }

    async fn acp_client(self: &Arc<Self>, agent_id: &str) -> Result<Arc<AcpClient>, String> {
        let mut slot = self.acp.lock().await;
        if let Some(client) = slot.as_ref()
            && client.agent.id == agent_id
            && client.is_alive()
        {
            return Ok(client.clone());
        }
        if let Some(old) = slot.take() {
            old.shutdown().await;
        }
        let config = self
            .acp_agents
            .iter()
            .find(|a| a.id == agent_id)
            .cloned()
            .ok_or_else(|| format!("unknown agent `{agent_id}`"))?;
        let weak = Arc::downgrade(self);
        let emit_weak = weak.clone();
        let bridge = AcpBridge::new(acp::bridge::BridgeConfig {
            agent_id: agent_id.to_string(),
            sandbox: self.sandbox.clone(),
            changes: self.changes.clone(),
            emit: Arc::new(move |event| {
                if let Some(inner) = emit_weak.upgrade() {
                    inner.emit_thread(event);
                }
            }),
            changes_updated: Arc::new(move || {
                if let Some(inner) = weak.upgrade() {
                    let _ = inner.events.unbounded_send(SessionEvent::ChangesUpdated);
                }
            }),
            broker: self.broker(),
            permissions: self.permissions.clone(),
            mode: self.mode.clone(),
        });
        let resume = lock(&self.thread).acp_session_id.clone();
        let mcp = self.mcp.clone();
        let client = AcpClient::start(
            &config,
            &self.project,
            bridge,
            move |http| mcp.for_acp(http),
            resume.as_deref(),
            self.path_env.as_deref(),
        )
        .await?;
        lock(&self.thread).acp_session_id = Some(client.session_id.clone());
        let client = Arc::new(client);
        *slot = Some(client.clone());
        Ok(client)
    }

    async fn run_acp(
        self: &Arc<Self>,
        agent_id: &str,
        text: &str,
        attachments: &[Attachment],
        token: &CancellationToken,
    ) -> StopReason {
        let client = tokio::select! {
            c = self.acp_client(agent_id) => c,
            _ = token.cancelled() => return StopReason::Cancelled,
        };
        let client = match client {
            Ok(c) => c,
            Err(e) => {
                self.emit_thread(ThreadEvent::Error(e.clone()));
                return StopReason::Error(e);
            }
        };
        let mode = *lock(&self.mode);
        if let Some(target) = client.mode_for(mode)
            && client.current_mode().as_deref() != Some(target.id.as_str())
        {
            let id = target.id.clone();
            let _ = client.set_mode(&id).await;
        }
        let prompt = client.prompt(acp::prompt_blocks(text, attachments));
        tokio::pin!(prompt);
        let result = tokio::select! {
            r = &mut prompt => r,
            _ = token.cancelled() => {
                client.cancel().await;
                match tokio::time::timeout(std::time::Duration::from_secs(10), &mut prompt).await {
                    Ok(r) => r.map(|_| StopReason::Cancelled),
                    Err(_) => Ok(StopReason::Cancelled),
                }
            }
        };
        match result {
            Ok(stop) => stop,
            Err(e) => {
                self.emit_thread(ThreadEvent::Error(e.clone()));
                StopReason::Error(e)
            }
        }
    }
}

struct Sink(Arc<Inner>);

impl TurnSink for Sink {
    fn emit(&self, event: ThreadEvent) {
        self.0.emit_thread(event);
    }

    fn history(&self) -> Vec<Message> {
        lock(&self.0.thread).to_messages()
    }

    fn next_message_id(&self) -> u64 {
        lock(&self.0.thread).next_message_id()
    }

    fn changes_updated(&self) {
        let _ = self.0.events.unbounded_send(SessionEvent::ChangesUpdated);
    }
}

struct SessionBroker(Weak<Inner>);

#[async_trait::async_trait]
impl PermissionBroker for SessionBroker {
    async fn ask(&self, mut request: PermissionRequest) -> Decision {
        let Some(inner) = self.0.upgrade() else {
            return Decision::Deny;
        };
        request.id = inner.next_request.fetch_add(1, Ordering::SeqCst);
        let id = request.id;
        let (tx, rx) = oneshot::channel();
        lock(&inner.pending).insert(id, (request.clone(), tx));
        if inner
            .events
            .unbounded_send(SessionEvent::Permission(request))
            .is_err()
        {
            // No UI is listening any more (its view was dropped): nobody
            // can answer, so the turn is denied instead of waiting forever.
            lock(&inner.pending).remove(&id);
            return Decision::Deny;
        }
        drop(inner);
        let decision = rx.await.unwrap_or(Decision::Deny);
        if let Some(inner) = self.0.upgrade() {
            let _ = inner
                .events
                .unbounded_send(SessionEvent::PermissionClosed(id));
        }
        decision
    }
}
