//! The debugger: breakpoints (saved per project), the running session
//! (the adapter connection plus `ferrumeditor_core::dap::DebugState`),
//! the debug console, watch expressions and expanded variables. It runs
//! the protocol — the handshake, configuration after `initialized`, the
//! stack after `stopped`, steps, evaluate — and tells the workspace where
//! execution stopped and what to run in the terminal (`runInTerminal`).
//! js-debug's child sessions (`startDebugging`) are extra connections to
//! the same adapter; requests go to the one that stopped last.

mod panel;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ferrumeditor_core::dap::state::{self, OutputLine};
use ferrumeditor_core::dap::{
    AdapterKind, Breakpoints, DapClient, DapEvent, DebugState, Followup, LaunchConfig, Phase,
    adapters, launch,
};
use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use gpui::{Context, EventEmitter, Task};
use serde_json::{Value, json};

use crate::lsp_store::LspStore;
use crate::settings::Settings;

pub use panel::{DebugPanel, DebugPanelEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebuggerEvent {
    /// Breakpoints, the session or the stack changed.
    Changed,
    /// Show this place (1-based line).
    Stopped {
        path: PathBuf,
        line: usize,
    },
    /// `runInTerminal`: run this command line in a terminal tab.
    RunInTerminal {
        title: String,
        command: String,
        cwd: PathBuf,
    },
    Notice(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watch {
    pub expression: String,
    pub value: Option<Result<String, String>>,
}

pub struct Running {
    id: u64,
    conns: Vec<DapClient>,
    /// The connection requests go to (the last that stopped).
    active: usize,
    pub adapter: AdapterKind,
    pub state: DebugState,
    pub config: LaunchConfig,
    _listeners: Vec<Task<()>>,
}

impl Running {
    fn client(&self) -> DapClient {
        self.conns[self.active.min(self.conns.len() - 1)].clone()
    }

    fn thread(&self) -> Option<i64> {
        self.state
            .thread_id
            .or_else(|| self.state.threads.first().map(|t| t.id))
    }
}

pub struct Debugger {
    pub breakpoints: Breakpoints,
    store: Option<PathBuf>,
    project: Option<PathBuf>,
    pub session: Option<Running>,
    /// Program output, REPL lines and messages; kept after a session.
    pub console: Vec<OutputLine>,
    pub watches: Vec<Watch>,
    /// Variables references shown expanded.
    pub expanded: HashSet<i64>,
    /// "Building with cargo…" and the like, before the session is up.
    pub status: Option<String>,
    last: Option<(LaunchConfig, PathBuf)>,
    servers: Option<PathBuf>,
    next_id: u64,
    installing: bool,
    tasks: Vec<Task<()>>,
}

impl EventEmitter<DebuggerEvent> for Debugger {}

const CONSOLE_LIMIT: usize = 5_000;

impl Debugger {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let files = cx.global::<Settings>().files.clone();
        Self {
            breakpoints: Breakpoints::default(),
            store: files.as_ref().map(|f| f.breakpoints.clone()),
            servers: files.map(|f| f.servers_dir),
            project: None,
            session: None,
            console: Vec::new(),
            watches: Vec::new(),
            expanded: HashSet::new(),
            status: None,
            last: None,
            next_id: 1,
            installing: false,
            tasks: Vec::new(),
        }
    }

    /// A project opened: load its breakpoints.
    pub fn set_project(&mut self, root: Option<PathBuf>, cx: &mut Context<Self>) {
        self.project = root.clone();
        self.breakpoints = match (&self.store, &root) {
            (Some(store), Some(root)) => Breakpoints::load(store, root),
            _ => Breakpoints::default(),
        };
        cx.emit(DebuggerEvent::Changed);
        cx.notify();
    }

    fn save_breakpoints(&self) {
        if let (Some(store), Some(root)) = (&self.store, &self.project)
            && let Err(error) = self.breakpoints.save(store, root)
        {
            eprintln!("ferrumeditor: could not save breakpoints: {error}");
        }
    }

    pub fn phase(&self) -> Option<Phase> {
        self.session.as_ref().map(|s| s.state.phase())
    }

    pub fn is_stopped(&self) -> bool {
        self.phase().is_some_and(|p| p.is_stopped())
    }

    /// The status bar text.
    pub fn status_label(&self) -> Option<String> {
        if let Some(status) = &self.status {
            return Some(status.clone());
        }
        let session = self.session.as_ref()?;
        Some(format!(
            "{}: {}",
            session.config.name,
            session.state.phase().label()
        ))
    }

    pub fn push_console(&mut self, category: &str, text: &str) {
        if let Some(last) = self.console.last_mut()
            && last.category == category
            && !last.text.ends_with('\n')
        {
            last.text.push_str(text);
        } else {
            self.console.push(OutputLine {
                category: category.into(),
                text: text.into(),
            });
        }
        if self.console.len() > CONSOLE_LIMIT {
            let extra = self.console.len() - CONSOLE_LIMIT;
            self.console.drain(..extra);
        }
    }

    /// Breakpoint lines (0-based) with their verified state while a
    /// session runs, and the 0-based line execution stopped at.
    pub fn marks_for(&self, path: &Path) -> (Vec<(usize, Option<bool>)>, Option<usize>) {
        let state = self.session.as_ref().map(|s| &s.state);
        let breakpoints = self
            .breakpoints
            .lines(path)
            .into_iter()
            .map(|line| {
                let verified = state.and_then(|s| s.verified(path, line));
                let verified = match (state, verified) {
                    (Some(_), None) => Some(false),
                    (_, v) => v,
                };
                (line.saturating_sub(1), verified)
            })
            .collect();
        let exec = state
            .filter(|s| s.phase().is_stopped())
            .and_then(|s| s.current_location())
            .filter(|(p, _)| p == path)
            .map(|(_, line)| line.saturating_sub(1));
        (breakpoints, exec)
    }

    /// F9 / gutter click at 1-based `line`.
    pub fn toggle_breakpoint(&mut self, path: &Path, line: usize, cx: &mut Context<Self>) {
        self.breakpoints.toggle(path, line);
        self.save_breakpoints();
        self.send_breakpoints(path.to_path_buf(), cx);
        cx.emit(DebuggerEvent::Changed);
        cx.notify();
    }

    pub fn remove_all_breakpoints(&mut self, cx: &mut Context<Self>) {
        let files = self.breakpoints.clear();
        self.save_breakpoints();
        for file in files {
            self.send_breakpoints(file, cx);
        }
        cx.emit(DebuggerEvent::Changed);
        cx.notify();
    }

    fn send_breakpoints(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(session) = &self.session else {
            return;
        };
        let lines = self.breakpoints.lines(&path);
        let (sid, clients) = (session.id, session.conns.clone());
        self.tasks.push(cx.spawn(async move |this, cx| {
            for client in clients {
                let result = client
                    .request("setBreakpoints", state::set_breakpoints_args(&path, &lines))
                    .await;
                if let Ok(body) = result {
                    this.update(cx, |d, cx| {
                        if let Some(s) = d.session.as_mut().filter(|s| s.id == sid) {
                            s.state.set_breakpoint_results(&path, &lines, &body);
                            cx.emit(DebuggerEvent::Changed);
                        }
                    })
                    .ok();
                }
            }
        }));
    }

    async fn path_env(
        base: Option<futures::future::Shared<Task<String>>>,
        servers: Option<PathBuf>,
    ) -> String {
        let base = match base {
            Some(task) => task.await,
            None => std::env::var("PATH").unwrap_or_default(),
        };
        match servers {
            Some(servers) => ferrumeditor_core::lsp::install::with_managed(&base, &servers),
            None => base,
        }
    }

    /// Starts `config` in `root` (stopping a running session first).
    pub fn start(&mut self, config: LaunchConfig, root: PathBuf, cx: &mut Context<Self>) {
        self.stop(cx);
        self.last = Some((config.clone(), root.clone()));
        self.console.clear();
        self.expanded.clear();
        self.status = Some(format!("Starting {}…", config.name));
        cx.notify();
        let base = LspStore::global(cx).map(|s| s.read(cx).path_env());
        let servers = self.servers.clone();
        let id = self.next_id;
        self.next_id += 1;
        let task = cx.spawn(async move |this, cx| {
            let path = Self::path_env(base, servers.clone()).await;
            let fail = |message: String, cx: &mut gpui::AsyncApp| {
                this.update(cx, |d, cx| {
                    d.status = None;
                    d.push_console("important", &format!("{message}\n"));
                    cx.emit(DebuggerEvent::Error(message));
                    cx.notify();
                })
                .ok();
            };
            let cwd = config.raw["cwd"]
                .as_str()
                .map_or_else(|| root.clone(), PathBuf::from);
            let (kind, command) =
                match adapters::adapter_for_config(&config.raw, &path, servers.as_deref(), &cwd) {
                    Ok(found) => found,
                    Err(missing) => {
                        let install = if missing.installable {
                            " Run \"Debug: Install Adapter\"."
                        } else {
                            ""
                        };
                        fail(format!("{missing}.{install}"), cx);
                        return;
                    }
                };
            let mut raw = config.raw.clone();
            if let Some(args) = launch::cargo_args(&raw) {
                this.update(cx, |d, cx| {
                    d.status = Some(format!("Building: cargo {}…", args.join(" ")));
                    cx.notify();
                })
                .ok();
                let build_path = path.clone();
                let build_dir = cwd.clone();
                let output = cx
                    .background_executor()
                    .spawn(async move {
                        std::process::Command::new("cargo")
                            .args(&args)
                            .current_dir(&build_dir)
                            .env("PATH", &build_path)
                            .stdin(std::process::Stdio::null())
                            .output()
                    })
                    .await;
                let output = match output {
                    Ok(output) => output,
                    Err(error) => {
                        fail(format!("Could not run cargo: {error}"), cx);
                        return;
                    }
                };
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                this.update(cx, |d, _| d.push_console("console", &stderr))
                    .ok();
                if !output.status.success() {
                    fail("The cargo build failed (see the debug console)".into(), cx);
                    return;
                }
                let artifacts = launch::cargo_executables(&String::from_utf8_lossy(&output.stdout));
                match launch::pick_executable(&raw, &artifacts) {
                    Some(program) => raw["program"] = json!(program.to_string_lossy()),
                    None => {
                        fail("cargo built no executable to debug".into(), cx);
                        return;
                    }
                }
            }
            let (tx, rx) = unbounded();
            let spawned = cx
                .background_executor()
                .spawn(async move { DapClient::spawn(&command, tx) })
                .await;
            let client = match spawned {
                Ok(client) => client,
                Err(error) => {
                    fail(error, cx);
                    return;
                }
            };
            let started = this.update(cx, |d, cx| {
                d.status = None;
                let mut running = Running {
                    id,
                    conns: vec![client.clone()],
                    active: 0,
                    adapter: kind,
                    state: DebugState::new(),
                    config: config.clone(),
                    _listeners: Vec::new(),
                };
                running._listeners.push(Self::listen(id, 0, rx, cx));
                d.session = Some(running);
                cx.emit(DebuggerEvent::Changed);
                cx.notify();
            });
            if started.is_err() {
                return;
            }
            let request = config.request.clone();
            let arguments = adapters::launch_arguments(kind, &raw);
            Self::handshake(
                this.clone(),
                id,
                client,
                kind.adapter_id(),
                request,
                arguments,
                cx,
            )
            .await;
        });
        self.tasks.push(task);
    }

    /// `initialize`, then `launch` / `attach` (answered by some adapters
    /// only after `configurationDone`, which the `initialized` event
    /// triggers meanwhile).
    async fn handshake(
        this: gpui::WeakEntity<Self>,
        id: u64,
        client: DapClient,
        adapter_id: &str,
        request: String,
        arguments: Value,
        cx: &mut gpui::AsyncApp,
    ) {
        let caps = client
            .request("initialize", state::initialize_args(adapter_id))
            .await;
        let caps = match caps {
            Ok(caps) => caps,
            Err(error) => {
                this.update(cx, |d, cx| {
                    d.end(id, Some(format!("initialize failed: {error}")), cx)
                })
                .ok();
                return;
            }
        };
        this.update(cx, |d, _| {
            if let Some(s) = d.session.as_mut().filter(|s| s.id == id) {
                s.state.set_capabilities(&caps);
            }
        })
        .ok();
        if let Err(error) = client.request(&request, arguments).await {
            this.update(cx, |d, cx| {
                d.end(id, Some(format!("{request} failed: {error}")), cx)
            })
            .ok();
        }
    }

    fn listen(
        id: u64,
        conn: usize,
        mut events: UnboundedReceiver<DapEvent>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                if this
                    .update(cx, |d, cx| d.on_dap_event(id, conn, event, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
    }

    fn on_dap_event(&mut self, id: u64, conn: usize, event: DapEvent, cx: &mut Context<Self>) {
        let Some(session) = self.session.as_mut().filter(|s| s.id == id) else {
            return;
        };
        match event {
            DapEvent::Event { event, body } => {
                if event == "output" {
                    let category = body["category"].as_str().unwrap_or("console");
                    if category != "telemetry" {
                        let text = body["output"].as_str().unwrap_or_default().to_string();
                        self.push_console(category, &text);
                    }
                    cx.notify();
                    return;
                }
                if event == "stopped" {
                    session.active = conn;
                }
                let followups = session.state.on_event(&event, &body);
                for followup in followups {
                    match followup {
                        Followup::Configure => self.configure(id, conn, cx),
                        Followup::FetchStack { thread_id } => self.fetch_stack(id, thread_id, cx),
                        Followup::FetchThreads => self.fetch_threads(id, cx),
                        Followup::End if conn == 0 => {
                            self.end(id, None, cx);
                            return;
                        }
                        Followup::End => {
                            // A child session ended; the parent goes on.
                            if let Some(s) = self.session.as_mut() {
                                s.active = 0;
                                if !s.state.phase().is_over() {
                                    s.state.resumed();
                                }
                            }
                        }
                    }
                }
                if event == "exited" {
                    let code = body["exitCode"].as_i64().unwrap_or(0);
                    self.push_console("important", &format!("Program exited with code {code}\n"));
                }
                cx.emit(DebuggerEvent::Changed);
            }
            DapEvent::Request {
                seq,
                command,
                arguments,
            } => {
                let client = session.conns[conn.min(session.conns.len() - 1)].clone();
                match command.as_str() {
                    "runInTerminal" => match state::run_in_terminal(&arguments) {
                        Some((title, command_line, cwd)) => {
                            client.respond(seq, "runInTerminal", true, json!({}));
                            cx.emit(DebuggerEvent::RunInTerminal {
                                title,
                                command: command_line,
                                cwd,
                            });
                        }
                        None => client.respond(seq, "runInTerminal", false, json!({})),
                    },
                    "startDebugging" => {
                        client.respond(seq, "startDebugging", true, json!({}));
                        self.start_child(id, &arguments, cx);
                    }
                    other => client.respond(seq, other, false, json!({})),
                }
            }
            DapEvent::Closed { message } => {
                if conn == 0 {
                    let why = (!message.is_empty()).then_some(message);
                    self.end(id, why, cx);
                }
            }
        }
        cx.notify();
    }

    /// js-debug: a child session on a new connection to the adapter.
    fn start_child(&mut self, id: u64, arguments: &Value, cx: &mut Context<Self>) {
        let Some(session) = self.session.as_mut().filter(|s| s.id == id) else {
            return;
        };
        let Some(address) = session.conns[0].address() else {
            self.push_console("important", "startDebugging needs a TCP adapter\n");
            return;
        };
        let (tx, rx) = unbounded();
        let client = match DapClient::connect(address, "child", tx) {
            Ok(client) => client,
            Err(error) => {
                self.push_console("important", &format!("child session: {error}\n"));
                return;
            }
        };
        let conn = session.conns.len();
        session.conns.push(client.clone());
        let listener = Self::listen(id, conn, rx, cx);
        session._listeners.push(listener);
        let adapter_id = session.adapter.adapter_id();
        let request = arguments["request"]
            .as_str()
            .unwrap_or("launch")
            .to_string();
        let configuration = arguments["configuration"].clone();
        self.tasks.push(cx.spawn(async move |this, cx| {
            Self::handshake(this, id, client, adapter_id, request, configuration, cx).await;
        }));
    }

    fn configure(&mut self, id: u64, conn: usize, cx: &mut Context<Self>) {
        let Some(session) = self.session.as_ref().filter(|s| s.id == id) else {
            return;
        };
        let client = session.conns[conn.min(session.conns.len() - 1)].clone();
        let done = session.state.capabilities["supportsConfigurationDoneRequest"] != false;
        let files: Vec<(PathBuf, Vec<usize>)> = self
            .breakpoints
            .files()
            .map(|(p, l)| (p.clone(), l.iter().copied().collect()))
            .collect();
        self.tasks.push(cx.spawn(async move |this, cx| {
            for (path, lines) in files {
                if let Ok(body) = client
                    .request("setBreakpoints", state::set_breakpoints_args(&path, &lines))
                    .await
                {
                    this.update(cx, |d, cx| {
                        if let Some(s) = d.session.as_mut().filter(|s| s.id == id) {
                            s.state.set_breakpoint_results(&path, &lines, &body);
                            cx.emit(DebuggerEvent::Changed);
                        }
                    })
                    .ok();
                }
            }
            client
                .request("setExceptionBreakpoints", json!({"filters": []}))
                .await
                .ok();
            if done {
                client.request("configurationDone", Value::Null).await.ok();
            }
            this.update(cx, |d, cx| {
                if let Some(s) = d.session.as_mut().filter(|s| s.id == id)
                    && s.state.phase() == Phase::Starting
                {
                    s.state.phase = Some(Phase::Running);
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn fetch_threads(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(client) = self
            .session
            .as_ref()
            .filter(|s| s.id == id)
            .map(Running::client)
        else {
            return;
        };
        self.tasks.push(cx.spawn(async move |this, cx| {
            if let Ok(body) = client.request("threads", Value::Null).await {
                this.update(cx, |d, cx| {
                    if let Some(s) = d.session.as_mut().filter(|s| s.id == id) {
                        s.state.set_threads(&body);
                    }
                    cx.notify();
                })
                .ok();
            }
        }));
    }

    /// After a stop: threads, the stack, the top frame's scopes and
    /// variables, the watches; then shows the place.
    fn fetch_stack(&mut self, id: u64, thread_id: Option<i64>, cx: &mut Context<Self>) {
        let Some(client) = self
            .session
            .as_ref()
            .filter(|s| s.id == id)
            .map(Running::client)
        else {
            return;
        };
        self.expanded.clear();
        self.tasks.push(cx.spawn(async move |this, cx| {
            let threads = client
                .request("threads", Value::Null)
                .await
                .unwrap_or_default();
            let thread = this
                .update(cx, |d, _| {
                    let s = d.session.as_mut().filter(|s| s.id == id)?;
                    s.state.set_threads(&threads);
                    thread_id.or_else(|| s.thread())
                })
                .ok()
                .flatten();
            let Some(thread) = thread else {
                return;
            };
            let Ok(stack) = client
                .request("stackTrace", state::stack_trace_args(thread))
                .await
            else {
                return;
            };
            let frame = this
                .update(cx, |d, _| {
                    let s = d.session.as_mut().filter(|s| s.id == id)?;
                    s.state.set_stack(thread, &stack);
                    s.state.selected_frame().map(|f| f.id)
                })
                .ok()
                .flatten();
            if let Some(frame) = frame {
                Self::load_frame(this.clone(), id, client, frame, cx).await;
            }
        }));
    }

    /// Scopes and their variables for `frame`, then the watches; ends by
    /// showing the frame's place.
    async fn load_frame(
        this: gpui::WeakEntity<Self>,
        id: u64,
        client: DapClient,
        frame: i64,
        cx: &mut gpui::AsyncApp,
    ) {
        if let Ok(scopes) = client.request("scopes", state::scopes_args(frame)).await {
            let refs = this
                .update(cx, |d, _| {
                    let Some(s) = d.session.as_mut().filter(|s| s.id == id) else {
                        return Vec::new();
                    };
                    s.state.set_scopes(&scopes);
                    s.state
                        .scopes
                        .iter()
                        .filter(|s| !s.expensive)
                        .map(|s| s.variables_reference)
                        .collect()
                })
                .unwrap_or_default();
            for reference in refs {
                if let Ok(vars) = client
                    .request("variables", state::variables_args(reference))
                    .await
                {
                    this.update(cx, |d, _| {
                        if let Some(s) = d.session.as_mut().filter(|s| s.id == id) {
                            s.state.set_variables(reference, &vars);
                        }
                        d.expanded.insert(reference);
                    })
                    .ok();
                }
            }
        }
        let watches: Vec<String> = this
            .update(cx, |d, _| {
                d.watches.iter().map(|w| w.expression.clone()).collect()
            })
            .unwrap_or_default();
        for (i, expression) in watches.into_iter().enumerate() {
            let result = client
                .request(
                    "evaluate",
                    state::evaluate_args(&expression, Some(frame), "watch"),
                )
                .await
                .map(|body| state::evaluate_result(&body).0);
            this.update(cx, |d, _| {
                if let Some(w) = d.watches.get_mut(i) {
                    w.value = Some(result);
                }
            })
            .ok();
        }
        this.update(cx, |d, cx| {
            if let Some((path, line)) = d
                .session
                .as_ref()
                .filter(|s| s.id == id)
                .and_then(|s| s.state.current_location())
            {
                cx.emit(DebuggerEvent::Stopped { path, line });
            }
            cx.emit(DebuggerEvent::Changed);
            cx.notify();
        })
        .ok();
    }

    /// A frame clicked in the call stack.
    pub fn select_frame(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let Some(frame) = session.state.frames.get(index).map(|f| f.id) else {
            return;
        };
        session.state.selected_frame = index;
        session.state.scopes.clear();
        session.state.variables.clear();
        let (id, client) = (session.id, session.client());
        self.tasks.push(cx.spawn(async move |this, cx| {
            Self::load_frame(this, id, client, frame, cx).await;
        }));
        cx.notify();
    }

    /// Expands (loading on first use) or collapses a variable.
    pub fn toggle_variable(&mut self, reference: i64, cx: &mut Context<Self>) {
        if !self.expanded.insert(reference) {
            self.expanded.remove(&reference);
            cx.notify();
            return;
        }
        let Some(session) = self.session.as_ref() else {
            return;
        };
        if session.state.variables.contains_key(&reference) {
            cx.notify();
            return;
        }
        let (id, client) = (session.id, session.client());
        self.tasks.push(cx.spawn(async move |this, cx| {
            if let Ok(vars) = client
                .request("variables", state::variables_args(reference))
                .await
            {
                this.update(cx, |d, cx| {
                    if let Some(s) = d.session.as_mut().filter(|s| s.id == id) {
                        s.state.set_variables(reference, &vars);
                    }
                    cx.notify();
                })
                .ok();
            }
        }));
    }

    /// `continue`, `next`, `stepIn`, `stepOut` or `pause`.
    pub fn step(&mut self, command: &'static str, cx: &mut Context<Self>) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let stopped = session.state.phase().is_stopped();
        if command == "pause" && stopped || command != "pause" && !stopped {
            return;
        }
        let Some(thread) = session.thread() else {
            return;
        };
        if command != "pause" {
            session.state.resumed();
        }
        let client = session.client();
        self.tasks.push(cx.spawn(async move |this, cx| {
            if let Err(error) = client.request(command, state::thread_args(thread)).await {
                this.update(cx, |d, cx| {
                    d.push_console("important", &format!("{command} failed: {error}\n"));
                    cx.notify();
                })
                .ok();
            }
        }));
        cx.emit(DebuggerEvent::Changed);
        cx.notify();
    }

    /// Debug console input: evaluated in the selected frame.
    pub fn evaluate(&mut self, expression: String, cx: &mut Context<Self>) {
        let expression = expression.trim().to_string();
        if expression.is_empty() {
            return;
        }
        self.push_console("repl", &format!("> {expression}\n"));
        let Some(session) = self.session.as_ref() else {
            self.push_console("important", "No debug session\n");
            cx.notify();
            return;
        };
        let frame = session.state.selected_frame().map(|f| f.id);
        let client = session.client();
        self.tasks.push(cx.spawn(async move |this, cx| {
            let result = client
                .request("evaluate", state::evaluate_args(&expression, frame, "repl"))
                .await;
            this.update(cx, |d, cx| {
                match result {
                    Ok(body) => {
                        let (text, _) = state::evaluate_result(&body);
                        d.push_console("result", &format!("{text}\n"));
                    }
                    Err(error) => d.push_console("stderr", &format!("{error}\n")),
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    pub fn add_watch(&mut self, expression: String, cx: &mut Context<Self>) {
        let expression = expression.trim().to_string();
        if expression.is_empty() || self.watches.iter().any(|w| w.expression == expression) {
            return;
        }
        self.watches.push(Watch {
            expression,
            value: None,
        });
        if let Some(index) = self.session.as_ref().map(|s| s.state.selected_frame) {
            self.select_frame(index, cx);
        }
        cx.notify();
    }

    pub fn remove_watch(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.watches.len() {
            self.watches.remove(index);
        }
        cx.notify();
    }

    /// Ends the session: `disconnect` (terminating the program), then the
    /// adapter is stopped.
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        self.status = None;
        let Some(session) = self.session.take() else {
            return;
        };
        let clients = session.conns.clone();
        let terminate = session.state.capabilities["supportsTerminateRequest"] == true;
        drop(session);
        let timer = cx.background_executor().timer(Duration::from_secs(2));
        self.tasks.push(cx.spawn(async move |_, _| {
            let mut disconnect = Box::pin(async move {
                for client in clients.iter().rev() {
                    if terminate {
                        client.request("terminate", json!({})).await.ok();
                    }
                    client
                        .request("disconnect", state::disconnect_args(true))
                        .await
                        .ok();
                }
                clients
            });
            // Don't wait forever on an adapter that hangs.
            futures::future::select(disconnect.as_mut(), Box::pin(timer)).await;
        }));
        self.push_console("important", "Debug session stopped\n");
        cx.emit(DebuggerEvent::Changed);
        cx.notify();
    }

    pub fn restart(&mut self, cx: &mut Context<Self>) {
        match self.last.clone() {
            Some((config, root)) => self.start(config, root, cx),
            None => cx.emit(DebuggerEvent::Notice("Nothing to restart".into())),
        }
    }

    fn end(&mut self, id: u64, why: Option<String>, cx: &mut Context<Self>) {
        if self.session.as_ref().is_none_or(|s| s.id != id) {
            return;
        }
        let session = self.session.take();
        if let Some(why) = why {
            self.push_console("important", &format!("{why}\n"));
            cx.emit(DebuggerEvent::Error(why));
        } else if let Some(session) = session {
            let label = session.state.exit_code.map_or_else(
                || "Debug session ended".to_string(),
                |code| format!("Debug session ended (exit code {code})"),
            );
            cx.emit(DebuggerEvent::Notice(label));
        }
        self.status = None;
        cx.emit(DebuggerEvent::Changed);
        cx.notify();
    }

    /// Installs dlv (`go install`) or js-debug (GitHub release) into the
    /// editor's tools folder.
    pub fn install_adapter(&mut self, adapter: AdapterKind, cx: &mut Context<Self>) {
        let Some(servers) = self.servers.clone() else {
            cx.emit(DebuggerEvent::Error(
                "No home folder to install into".into(),
            ));
            return;
        };
        if self.installing {
            return;
        }
        if adapter == AdapterKind::Debugpy {
            cx.emit(DebuggerEvent::Notice(
                "Install debugpy into the project's Python: pip install debugpy".into(),
            ));
            return;
        }
        if !matches!(adapter, AdapterKind::Delve | AdapterKind::JsDebug) {
            cx.emit(DebuggerEvent::Notice(format!(
                "{} comes with Xcode's Command Line Tools / LLVM; install those",
                adapter.name()
            )));
            return;
        }
        self.installing = true;
        self.status = Some(format!("Installing {}…", adapter.name()));
        let base = LspStore::global(cx).map(|s| s.read(cx).path_env());
        self.tasks.push(cx.spawn(async move |this, cx| {
            let path_env = Self::path_env(base, None).await;
            let result = cx
                .background_executor()
                .spawn(async move {
                    let logs = servers.join("logs");
                    std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
                    let mut log = std::fs::File::create(logs.join("debug-adapter.log"))
                        .map_err(|e| e.to_string())?;
                    let mut installer = ferrumeditor_core::lsp::install::Installer {
                        servers,
                        path_env,
                        log: &mut log,
                    };
                    match adapter {
                        AdapterKind::Delve => installer.install_delve(),
                        _ => installer.install_js_debug(),
                    }
                })
                .await;
            this.update(cx, |d, cx| {
                d.installing = false;
                d.status = None;
                match result {
                    Ok(()) => cx.emit(DebuggerEvent::Notice(format!(
                        "Installed {}",
                        adapter.name()
                    ))),
                    Err(error) => cx.emit(DebuggerEvent::Error(format!(
                        "Could not install {}: {error} (log: servers/logs/debug-adapter.log)",
                        adapter.name()
                    ))),
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }
}

/// Launch configurations for the project: launch.json first, then the
/// defaults for the current file.
pub fn configurations(root: &Path, file: Option<&Path>) -> (Vec<LaunchConfig>, Option<String>) {
    let vars = launch::Variables {
        root: root.to_path_buf(),
        file: file.map(Path::to_path_buf),
    };
    let (mut configs, error) = match launch::read_launch_json(&vars) {
        Ok(configs) => (configs, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    configs.extend(launch::default_configs(&vars));
    (configs, error)
}
