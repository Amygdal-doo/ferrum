//! The running language servers, shared by every window as a global
//! entity. Editors ask it for a server when they open a file; it finds the
//! program (on the login shell's PATH), starts one server per language and
//! project folder, and keeps the diagnostics each server publishes.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ferrumeditor_core::language::Language;
use ferrumeditor_core::lsp::servers::{self, Resolved, ServerDef};
use ferrumeditor_core::lsp::types::parse_publish_diagnostics;
use ferrumeditor_core::lsp::{Client, Diagnostic, Launch, PositionEncoding, ServerEvent, Severity};
use futures::future::Shared;
use futures::{FutureExt, StreamExt};
use gpui::{App, Context, Entity, EventEmitter, Global, Task};
use serde_json::Value;

use crate::settings::Settings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspEvent {
    /// Diagnostics for this file changed.
    Diagnostics(PathBuf),
    /// A server started, became ready, reported progress or stopped.
    Status,
    /// Servers were restarted: editors attach again.
    Restarted,
    /// A server was installed (by its id): its files can attach now.
    Installed(&'static str),
    /// A message for the window's notice.
    Notice { message: String, error: bool },
    /// A server asks to apply a `WorkspaceEdit` (often while running a
    /// code action's command); answer with [`LspStore::reply`].
    ApplyEdit {
        server_id: u64,
        request_id: Value,
        edit: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerState {
    Starting,
    Ready,
    Stopped(String),
}

struct Server {
    id: u64,
    def: &'static ServerDef,
    root: PathBuf,
    client: Client,
    state: ServerState,
    /// Work-done progress by token: (title, "title — message 42%").
    progress: BTreeMap<String, (String, String)>,
    _events: Task<()>,
}

/// A server an editor can talk to.
#[derive(Clone)]
pub struct Attachment {
    pub server_id: u64,
    pub client: Client,
}

/// What the status bar shows for a file's language server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerStatus {
    Starting(String),
    Ready {
        name: String,
        progress: Option<String>,
    },
    Stopped {
        name: String,
        message: String,
    },
    Missing {
        language: String,
        install: String,
    },
    Installing {
        what: String,
    },
    InstallFailed {
        language: String,
        error: String,
    },
}

pub struct LspStore {
    project: Option<PathBuf>,
    servers: Vec<Server>,
    next_id: u64,
    /// Servers whose program was not found, by server id.
    missing: HashMap<&'static str, Vec<String>>,
    diagnostics: BTreeMap<PathBuf, (PositionEncoding, Arc<Vec<Diagnostic>>)>,
    /// Problems from the last task's output (cargo, tsc, go…).
    build: BTreeMap<PathBuf, Arc<Vec<Diagnostic>>>,
    path_env: Shared<Task<String>>,
    /// Where servers are installed (none in tests without a home).
    servers_dir: Option<PathBuf>,
    /// Installs running, by server id: what is being installed.
    installing: HashMap<&'static str, String>,
    /// Installs tried this session (each runs once unless asked again).
    install_tried: std::collections::HashSet<&'static str>,
    install_failed: HashMap<&'static str, String>,
    install_tasks: Vec<Task<()>>,
}

impl EventEmitter<LspEvent> for LspStore {}

pub struct GlobalLsp(pub Entity<LspStore>);

impl Global for GlobalLsp {}

impl LspStore {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // The login shell can take a moment; ask once, in the background.
        let path_env = cx
            .background_executor()
            .spawn(async { servers::search_path(servers::login_shell_path().as_deref()) });
        let servers_dir = cx
            .global::<Settings>()
            .files
            .as_ref()
            .map(|f| f.servers_dir.clone());
        Self::with_path_env(path_env, servers_dir)
    }

    /// A store that runs servers with `path_env` as PATH (tests).
    #[cfg(test)]
    pub fn with_path(path_env: String) -> Self {
        Self::with_path_env(Task::ready(path_env), None)
    }

    /// Like [`Self::with_path`], installing servers into `servers_dir`.
    #[cfg(test)]
    pub fn with_install_dir(path_env: String, servers_dir: PathBuf) -> Self {
        Self::with_path_env(Task::ready(path_env), Some(servers_dir))
    }

    fn with_path_env(path_env: Task<String>, servers_dir: Option<PathBuf>) -> Self {
        let path_env = path_env.shared();
        Self {
            servers_dir,
            installing: HashMap::new(),
            install_tried: std::collections::HashSet::new(),
            install_failed: HashMap::new(),
            install_tasks: Vec::new(),
            project: None,
            servers: Vec::new(),
            next_id: 1,
            missing: HashMap::new(),
            diagnostics: BTreeMap::new(),
            build: BTreeMap::new(),
            path_env,
        }
    }

    /// The app-wide store (absent in tests and when language servers are
    /// turned off).
    pub fn global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalLsp>().map(|g| g.0.clone())
    }

    /// The login shell's PATH plus tool folders (resolves once asked).
    pub fn path_env(&self) -> Shared<Task<String>> {
        self.path_env.clone()
    }

    pub fn set_project(&mut self, root: Option<PathBuf>) {
        self.project = root;
    }

    /// Finds or starts the server for `path`. Resolves to `None` when the
    /// language has none, it is turned off or its program is missing.
    pub fn attach(
        &mut self,
        path: PathBuf,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Task<Option<Attachment>> {
        let Some(def) = servers::server_for(language) else {
            return Task::ready(None);
        };
        let path_env = self.path_env.clone();
        cx.spawn(async move |this, cx| {
            let path_env = path_env.await;
            this.update(cx, |this, cx| this.server_for(def, &path, &path_env, cx))
                .ok()
                .flatten()
        })
    }

    fn server_for(
        &mut self,
        def: &'static ServerDef,
        path: &Path,
        path_env: &str,
        cx: &mut Context<Self>,
    ) -> Option<Attachment> {
        let live = |s: &&Server| s.def.id == def.id && !matches!(s.state, ServerState::Stopped(_));
        let in_project = self.project.as_deref().is_some_and(|p| path.starts_with(p));
        // A file outside the project (a dependency opened by go to
        // definition) reuses any server of its language.
        let existing = self
            .servers
            .iter()
            .filter(live)
            .find(|s| path.starts_with(&s.root))
            .or_else(|| {
                (!in_project)
                    .then(|| self.servers.iter().find(live))
                    .flatten()
            });
        if let Some(server) = existing {
            return Some(Attachment {
                server_id: server.id,
                client: server.client.clone(),
            });
        }
        let config = cx.global::<Settings>().config.lsp.clone();
        let root = servers::root_for(def, path, self.project.as_deref());
        let managed = match &self.servers_dir {
            Some(dir) => ferrumeditor_core::lsp::install::with_managed(path_env, dir),
            None => path_env.to_string(),
        };
        let path_env = &servers::with_project_bins(&managed, &root, path);
        let (program, args) = match servers::resolve(def, &config, path_env) {
            Resolved::Found { program, args } => (program, args),
            Resolved::Disabled => return None,
            Resolved::Missing { tried } => {
                if self.missing.insert(def.id, tried).is_none() {
                    cx.emit(LspEvent::Status);
                }
                if config.auto_install {
                    self.install(def, false, cx);
                }
                return None;
            }
        };
        self.missing.remove(def.id);
        let options = config
            .servers
            .get(def.id)
            .and_then(|s| s.initialization_options.clone())
            .and_then(|o| serde_json::to_value(o).ok());
        let launch = Launch {
            program: program.clone(),
            args,
            path_env: Some(path_env.to_string()),
        };
        let name = program
            .file_name()
            .map_or_else(|| def.id.to_string(), |n| n.to_string_lossy().to_string());
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let id = self.next_id;
        self.next_id += 1;
        let (client, state) = match Client::spawn(&name, &launch, &root, options, tx) {
            Ok(client) => (client, ServerState::Starting),
            Err(error) => {
                self.missing
                    .insert(def.id, vec![format!("{name}: {error}")]);
                cx.emit(LspEvent::Status);
                return None;
            }
        };
        let events = cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                if this
                    .update(cx, |this, cx| this.on_server_event(id, event, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        self.servers.push(Server {
            id,
            def,
            root,
            client: client.clone(),
            state,
            progress: BTreeMap::new(),
            _events: events,
        });
        cx.emit(LspEvent::Status);
        Some(Attachment {
            server_id: id,
            client,
        })
    }

    fn on_server_event(&mut self, id: u64, event: ServerEvent, cx: &mut Context<Self>) {
        let Some(server) = self.servers.iter_mut().find(|s| s.id == id) else {
            return;
        };
        match event {
            ServerEvent::Ready => {
                server.state = ServerState::Ready;
                cx.emit(LspEvent::Status);
            }
            ServerEvent::Stopped { message } => {
                let never_ready = server.state == ServerState::Starting;
                // A rustup proxy without the component exits at once.
                let def = server.def;
                let unknown_binary = never_ready && message.contains("Unknown binary");
                if unknown_binary {
                    self.missing
                        .insert(def.id, vec![server.client.name().to_string()]);
                }
                server.state = ServerState::Stopped(if message.is_empty() {
                    "it exited".to_string()
                } else {
                    message
                });
                server.progress.clear();
                cx.emit(LspEvent::Status);
                if unknown_binary && cx.global::<Settings>().config.lsp.auto_install {
                    self.install(def, false, cx);
                }
            }
            ServerEvent::Request {
                id: request_id,
                params,
                ..
            } => {
                cx.emit(LspEvent::ApplyEdit {
                    server_id: id,
                    request_id,
                    edit: params.get("edit").cloned().unwrap_or(Value::Null),
                });
            }
            ServerEvent::Notification { method, params } => match method.as_str() {
                "textDocument/publishDiagnostics" => {
                    let encoding = server.client.position_encoding();
                    if let Some((uri, list)) = parse_publish_diagnostics(&params)
                        && let Some(path) = ferrumeditor_core::lsp::uri_to_path(&uri)
                    {
                        if list.is_empty() {
                            self.diagnostics.remove(&path);
                        } else {
                            self.diagnostics
                                .insert(path.clone(), (encoding, Arc::new(list)));
                        }
                        cx.emit(LspEvent::Diagnostics(path));
                    }
                }
                "$/progress" if update_progress(&mut server.progress, &params) => {
                    cx.emit(LspEvent::Status);
                }
                _ => {}
            },
        }
    }

    /// Installs `def`'s server in the background; files of its languages
    /// attach when it is done. Runs once per session unless `again`.
    pub fn install(&mut self, def: &'static ServerDef, again: bool, cx: &mut Context<Self>) {
        use ferrumeditor_core::lsp::install;
        let Some(servers_dir) = self.servers_dir.clone() else {
            return;
        };
        if self.installing.contains_key(def.id)
            || !install::can_install(def)
            || (!again && self.install_tried.contains(def.id))
        {
            return;
        }
        self.install_tried.insert(def.id);
        self.install_failed.remove(def.id);
        let path_env = self.path_env.clone();
        let log_path = Self::install_log(&servers_dir, def.id);
        self.installing.insert(
            def.id,
            def.commands.first().map_or(def.id, |c| c.0).to_string(),
        );
        cx.emit(LspEvent::Status);
        let task = cx.spawn(async move |this, cx| {
            let path_env = path_env.await;
            let what = install::describe(def, &path_env);
            this.update(cx, |this, cx| {
                this.installing.insert(def.id, what.clone());
                cx.emit(LspEvent::Status);
            })
            .ok();
            let result = cx
                .background_executor()
                .spawn(async move {
                    std::fs::create_dir_all(&servers_dir).map_err(|e| e.to_string())?;
                    if let Some(parent) = log_path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    let mut log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
                    install::Installer {
                        servers: servers_dir,
                        path_env,
                        log: &mut log,
                    }
                    .install(def)
                })
                .await;
            this.update(cx, |this, cx| {
                this.installing.remove(def.id);
                match result {
                    Ok(()) => {
                        this.missing.remove(def.id);
                        cx.emit(LspEvent::Notice {
                            message: format!(
                                "Installed {what}: completions, problems and go to definition are on"
                            ),
                            error: false,
                        });
                        cx.emit(LspEvent::Installed(def.id));
                    }
                    Err(error) => {
                        this.install_failed.insert(def.id, error.clone());
                        cx.emit(LspEvent::Notice {
                            message: format!(
                                "Could not install {what}: {error}. Run \"Language Server: Show Install Log\" for details."
                            ),
                            error: true,
                        });
                    }
                }
                cx.emit(LspEvent::Status);
            })
            .ok();
        });
        self.install_tasks.push(task);
    }

    fn install_log(servers_dir: &Path, id: &str) -> PathBuf {
        servers_dir.join("logs").join(format!("{id}.log"))
    }

    /// The install log of the server for `language`.
    pub fn install_log_for(&self, language: Language) -> Option<PathBuf> {
        let def = servers::server_for(language)?;
        let path = Self::install_log(self.servers_dir.as_deref()?, def.id);
        path.is_file().then_some(path)
    }

    /// Stops every server; open editors attach again.
    pub fn restart(&mut self, cx: &mut Context<Self>) {
        self.servers.clear();
        self.missing.clear();
        let paths: Vec<PathBuf> = std::mem::take(&mut self.diagnostics).into_keys().collect();
        for path in paths {
            cx.emit(LspEvent::Diagnostics(path));
        }
        cx.emit(LspEvent::Restarted);
        cx.emit(LspEvent::Status);
    }

    /// A file's diagnostics: from its language server, then from the last
    /// task (compiler columns count characters, close to UTF-16).
    pub fn diagnostics_for(&self, path: &Path) -> Vec<(PositionEncoding, Arc<Vec<Diagnostic>>)> {
        let server = self.diagnostics.get(path).cloned();
        let build = self
            .build
            .get(path)
            .map(|list| (PositionEncoding::Utf16, list.clone()));
        server.into_iter().chain(build).collect()
    }

    /// Every file's diagnostics (servers first, then the last task).
    pub fn all_diagnostics(
        &self,
    ) -> impl Iterator<Item = (&PathBuf, PositionEncoding, &Arc<Vec<Diagnostic>>)> {
        self.diagnostics
            .iter()
            .map(|(path, (encoding, list))| (path, *encoding, list))
            .chain(
                self.build
                    .iter()
                    .map(|(path, list)| (path, PositionEncoding::Utf16, list)),
            )
    }

    /// Replaces the task problems with those of the latest run.
    pub fn set_build_problems(
        &mut self,
        problems: Vec<ferrumeditor_core::tasks::BuildProblem>,
        source: &str,
        cx: &mut Context<Self>,
    ) {
        let mut touched: Vec<PathBuf> = std::mem::take(&mut self.build).into_keys().collect();
        let mut grouped: BTreeMap<PathBuf, Vec<Diagnostic>> = BTreeMap::new();
        for p in problems {
            let at = ferrumeditor_core::lsp::Position::new(
                p.line.saturating_sub(1),
                p.column.saturating_sub(1),
            );
            grouped.entry(p.path).or_default().push(Diagnostic {
                range: ferrumeditor_core::lsp::LspRange { start: at, end: at },
                severity: p.severity,
                message: p.message,
                source: Some(source.to_string()),
                code: None,
                raw: Value::Null,
            });
        }
        for (path, list) in grouped {
            touched.push(path.clone());
            self.build.insert(path, Arc::new(list));
        }
        touched.sort();
        touched.dedup();
        for path in touched {
            cx.emit(LspEvent::Diagnostics(path));
        }
        cx.emit(LspEvent::Status);
    }

    /// (errors, warnings) across the project.
    pub fn counts(&self) -> (usize, usize) {
        let mut errors = 0;
        let mut warnings = 0;
        let lists = self
            .diagnostics
            .values()
            .map(|(_, list)| list)
            .chain(self.build.values());
        for list in lists {
            for d in list.iter() {
                match d.severity {
                    Severity::Error => errors += 1,
                    Severity::Warning => warnings += 1,
                    _ => {}
                }
            }
        }
        (errors, warnings)
    }

    /// The client of a running server.
    pub fn client(&self, server_id: u64) -> Option<Client> {
        self.servers
            .iter()
            .find(|s| s.id == server_id)
            .map(|s| s.client.clone())
    }

    /// Answers a server's request (see [`LspEvent::ApplyEdit`]).
    pub fn reply(&self, server_id: u64, request_id: Value, result: Value) {
        if let Some(client) = self.client(server_id) {
            client.reply(request_id, result);
        }
    }

    /// Tells every running server that files changed on disk behind its
    /// back (edits applied to files that are not open).
    pub fn files_changed(&self, paths: &[PathBuf]) {
        if paths.is_empty() {
            return;
        }
        let changes: Vec<Value> = paths
            .iter()
            .map(
                |p| serde_json::json!({ "uri": ferrumeditor_core::lsp::path_to_uri(p), "type": 2 }),
            )
            .collect();
        for server in &self.servers {
            if !matches!(server.state, ServerState::Stopped(_)) {
                server.client.notify(
                    "workspace/didChangeWatchedFiles",
                    serde_json::json!({ "changes": changes }),
                );
            }
        }
    }

    pub fn has_servers(&self) -> bool {
        self.servers
            .iter()
            .any(|s| !matches!(s.state, ServerState::Stopped(_)))
    }

    /// Status of the server that handles `path` in `language`.
    pub fn status_for(&self, server_id: Option<u64>, language: Language) -> Option<ServerStatus> {
        let def = servers::server_for(language)?;
        if let Some(what) = self.installing.get(def.id) {
            return Some(ServerStatus::Installing { what: what.clone() });
        }
        if let Some(error) = self.install_failed.get(def.id)
            && self.missing.contains_key(def.id)
        {
            return Some(ServerStatus::InstallFailed {
                language: language.name().to_string(),
                error: error.clone(),
            });
        }
        if let Some(server) = server_id.and_then(|id| self.servers.iter().find(|s| s.id == id)) {
            let name = server.client.name().to_string();
            return Some(match &server.state {
                ServerState::Starting => ServerStatus::Starting(name),
                ServerState::Ready => ServerStatus::Ready {
                    name,
                    progress: server
                        .progress
                        .values()
                        .next_back()
                        .map(|(_, text)| text.clone()),
                },
                ServerState::Stopped(message) => {
                    if self.missing.contains_key(def.id) {
                        ServerStatus::Missing {
                            language: language.name().to_string(),
                            install: def.install.to_string(),
                        }
                    } else {
                        ServerStatus::Stopped {
                            name,
                            message: message.clone(),
                        }
                    }
                }
            });
        }
        self.missing
            .contains_key(def.id)
            .then(|| ServerStatus::Missing {
                language: language.name().to_string(),
                install: def.install.to_string(),
            })
    }
}

/// Applies a `$/progress` notification; true if the text changed.
fn update_progress(progress: &mut BTreeMap<String, (String, String)>, params: &Value) -> bool {
    let token = match &params["token"] {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return false,
    };
    let value = &params["value"];
    match value["kind"].as_str() {
        Some("end") => progress.remove(&token).is_some(),
        Some(kind @ ("begin" | "report")) => {
            let (previous_title, previous) = progress.get(&token).cloned().unwrap_or_default();
            let title = if kind == "begin" {
                value["title"].as_str().unwrap_or_default().to_string()
            } else {
                previous_title
            };
            let mut text = title.clone();
            if let Some(message) = value["message"].as_str().filter(|m| !m.is_empty()) {
                text = format!("{text} — {message}");
            }
            if let Some(percent) = value["percentage"].as_u64() {
                text = format!("{text} {percent}%");
            }
            let changed = previous != text;
            progress.insert(token, (title, text));
            changed
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn progress_tracks_begin_report_end() {
        let mut progress = BTreeMap::new();
        assert!(update_progress(
            &mut progress,
            &json!({"token": "t", "value": {"kind": "begin", "title": "Indexing"}})
        ));
        assert!(update_progress(
            &mut progress,
            &json!({"token": "t", "value": {"kind": "report", "message": "3/10 core", "percentage": 30}})
        ));
        assert_eq!(progress["t"].1, "Indexing — 3/10 core 30%");
        update_progress(
            &mut progress,
            &json!({"token": "t", "value": {"kind": "report", "percentage": 40}}),
        );
        assert_eq!(progress["t"].1, "Indexing 40%");
        assert!(update_progress(
            &mut progress,
            &json!({"token": "t", "value": {"kind": "end"}})
        ));
        assert!(progress.is_empty());
        assert!(!update_progress(&mut progress, &json!({"value": {}})));
    }
}
