//! A language server connection: the process, a writer thread, a reader
//! thread and the request table. The client sends `initialize` first and
//! holds every other message until the server has answered it, so callers
//! can open documents and send requests straight away.
//!
//! Server notifications (diagnostics, messages, progress) and the end of
//! the process arrive on the [`ServerEvent`] channel given at start.

use std::collections::HashMap;
use std::future::Future;
use std::io::{BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use futures::channel::{mpsc::UnboundedSender, oneshot};
use serde_json::{Value, json};

use super::position::PositionEncoding;
use super::transport::{read_message, write_message};

/// Most stderr kept for error reports.
const STDERR_LIMIT: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum ServerEvent {
    /// `initialize` succeeded; requests are now answered.
    Ready,
    Notification {
        method: String,
        params: Value,
    },
    /// A request the app answers itself with [`Client::reply`]
    /// (`workspace/applyEdit`).
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    /// The process ended or the connection broke. `message` explains why
    /// (an error reply to `initialize`, or the end of stderr).
    Stopped {
        message: String,
    },
}

type Reply = Result<Value, String>;

struct Outbox {
    ready: bool,
    queue: Vec<Value>,
    tx: mpsc::Sender<Value>,
}

struct Shared {
    name: String,
    outbox: Mutex<Outbox>,
    pending: Mutex<HashMap<i64, oneshot::Sender<Reply>>>,
    next_id: AtomicI64,
    stopped: AtomicBool,
    capabilities: Mutex<Option<Value>>,
    stderr: Mutex<String>,
}

impl Shared {
    fn send(&self, message: Value) {
        let mut outbox = lock(&self.outbox);
        if outbox.ready {
            outbox.tx.send(message).ok();
        } else {
            outbox.queue.push(message);
        }
    }

    fn send_now(&self, message: Value) {
        lock(&self.outbox).tx.send(message).ok();
    }

    fn fail_pending(&self, why: &str) {
        self.stopped.store(true, Ordering::SeqCst);
        for (_, reply) in lock(&self.pending).drain() {
            reply.send(Err(why.to_string())).ok();
        }
    }
}

/// Kills the server when the last [`Client`] clone goes away.
struct ChildGuard {
    child: Mutex<Option<Child>>,
    shared: Arc<Shared>,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.shared
            .send_now(json!({"jsonrpc": "2.0", "method": "exit"}));
        if let Some(mut child) = lock(&self.child).take() {
            // Give it a moment to exit cleanly, then make sure.
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                child.kill().ok();
                child.wait().ok();
            });
        }
    }
}

#[derive(Clone)]
pub struct Client {
    shared: Arc<Shared>,
    _guard: Arc<ChildGuard>,
}

/// How to start a server process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: std::path::PathBuf,
    pub args: Vec<String>,
    /// `PATH` for the process (servers often start helpers by name).
    pub path_env: Option<String>,
}

impl Client {
    /// Starts `launch` in `root` and begins the handshake.
    pub fn spawn(
        name: &str,
        launch: &Launch,
        root: &Path,
        initialization_options: Option<Value>,
        events: UnboundedSender<ServerEvent>,
    ) -> std::io::Result<Self> {
        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(path) = &launch.path_env {
            command.env("PATH", path);
        }
        let mut child = command.spawn()?;
        let stdin = child.stdin.take().ok_or_else(missing_pipe)?;
        let stdout = child.stdout.take().ok_or_else(missing_pipe)?;
        let stderr = child.stderr.take().ok_or_else(missing_pipe)?;
        let client = Self::start(
            name,
            stdout,
            stdin,
            root,
            initialization_options,
            events,
            Some(child),
        );
        let shared = client.shared.clone();
        std::thread::Builder::new()
            .name(format!("lsp-stderr-{name}"))
            .spawn(move || collect_stderr(stderr, &shared))?;
        Ok(client)
    }

    /// Runs the protocol over any pair of streams (tests use pipes).
    pub fn start(
        name: &str,
        reader: impl Read + Send + 'static,
        writer: impl Write + Send + 'static,
        root: &Path,
        initialization_options: Option<Value>,
        events: UnboundedSender<ServerEvent>,
        child: Option<Child>,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<Value>();
        let shared = Arc::new(Shared {
            name: name.to_string(),
            outbox: Mutex::new(Outbox {
                ready: false,
                queue: Vec::new(),
                tx,
            }),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicI64::new(1),
            stopped: AtomicBool::new(false),
            capabilities: Mutex::new(None),
            stderr: Mutex::new(String::new()),
        });
        let mut writer = writer;
        std::thread::Builder::new()
            .name(format!("lsp-write-{name}"))
            .spawn(move || {
                for message in rx {
                    if write_message(&mut writer, &message).is_err() {
                        break;
                    }
                }
            })
            .ok();
        // `initialize` is id 0 and skips the queue.
        shared.send_now(json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": initialize_params(root, initialization_options),
        }));
        let reader_shared = shared.clone();
        std::thread::Builder::new()
            .name(format!("lsp-read-{name}"))
            .spawn(move || read_loop(reader, &reader_shared, &events))
            .ok();
        Self {
            _guard: Arc::new(ChildGuard {
                child: Mutex::new(child),
                shared: shared.clone(),
            }),
            shared,
        }
    }

    pub fn name(&self) -> &str {
        &self.shared.name
    }

    /// The server's capabilities once `initialize` has been answered.
    pub fn capabilities(&self) -> Option<Value> {
        lock(&self.shared.capabilities).clone()
    }

    pub fn is_ready(&self) -> bool {
        lock(&self.shared.outbox).ready && !self.is_stopped()
    }

    pub fn is_stopped(&self) -> bool {
        self.shared.stopped.load(Ordering::SeqCst)
    }

    pub fn position_encoding(&self) -> PositionEncoding {
        self.capabilities().map_or(PositionEncoding::Utf16, |c| {
            PositionEncoding::from_capabilities(&c)
        })
    }

    /// The last lines the server wrote to stderr.
    pub fn stderr_tail(&self) -> String {
        lock(&self.shared.stderr).clone()
    }

    pub fn notify(&self, method: &str, params: Value) {
        self.shared.send(json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    /// Answers a [`ServerEvent::Request`].
    pub fn reply(&self, id: Value, result: Value) {
        self.shared.send_now(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }));
    }

    /// Sends a request; the future resolves with the result, or an error
    /// message if the server replied with one or stopped.
    pub fn request(
        &self,
        method: &str,
        params: Value,
    ) -> impl Future<Output = Result<Value, String>> + Send + 'static {
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        lock(&self.shared.pending).insert(id, tx);
        if self.shared.stopped.load(Ordering::SeqCst)
            && let Some(tx) = lock(&self.shared.pending).remove(&id)
        {
            tx.send(Err("the language server stopped".to_string())).ok();
        }
        self.shared.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        async move {
            rx.await
                .unwrap_or_else(|_| Err("the language server stopped".to_string()))
        }
    }
}

fn missing_pipe() -> std::io::Error {
    std::io::Error::other("language server pipe missing")
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

fn collect_stderr(mut stderr: impl Read, shared: &Shared) {
    let mut chunk = [0u8; 4096];
    while let Ok(n) = stderr.read(&mut chunk) {
        if n == 0 {
            break;
        }
        let mut tail = lock(&shared.stderr);
        tail.push_str(&String::from_utf8_lossy(&chunk[..n]));
        if tail.len() > STDERR_LIMIT {
            let mut cut = tail.len() - STDERR_LIMIT;
            while !tail.is_char_boundary(cut) {
                cut += 1;
            }
            tail.drain(..cut);
        }
    }
}

fn read_loop(reader: impl Read, shared: &Shared, events: &UnboundedSender<ServerEvent>) {
    let mut reader = BufReader::new(reader);
    let mut stop_reason: Option<String> = None;
    loop {
        let message = match read_message(&mut reader) {
            Ok(Some(message)) => message,
            Ok(None) => break,
            Err(error) => {
                stop_reason = Some(format!("bad message from the server: {error}"));
                break;
            }
        };
        let id = message.get("id").cloned();
        let method = message["method"].as_str().map(str::to_string);
        match (id, method) {
            // A request from the server: edits go to the app (which
            // answers), the rest get a default answer here.
            (Some(id), Some(method)) if method == "workspace/applyEdit" => {
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                let request = ServerEvent::Request {
                    id: id.clone(),
                    method,
                    params,
                };
                if events.unbounded_send(request).is_err() {
                    shared.send_now(
                        json!({"jsonrpc": "2.0", "id": id, "result": {"applied": false}}),
                    );
                }
            }
            (Some(id), Some(method)) => {
                shared.send_now(answer_server_request(id, &method, &message["params"]));
            }
            (None, Some(method)) => {
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                events
                    .unbounded_send(ServerEvent::Notification { method, params })
                    .ok();
            }
            (Some(id), None) => {
                let reply = match message.get("error") {
                    Some(error) => Err(error["message"]
                        .as_str()
                        .unwrap_or("request failed")
                        .to_string()),
                    None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                };
                if id.as_i64() == Some(0) {
                    match reply {
                        Ok(result) => {
                            *lock(&shared.capabilities) =
                                Some(result.get("capabilities").cloned().unwrap_or(json!({})));
                            shared.send_now(json!({
                                "jsonrpc": "2.0",
                                "method": "initialized",
                                "params": {},
                            }));
                            let mut outbox = lock(&shared.outbox);
                            outbox.ready = true;
                            for queued in std::mem::take(&mut outbox.queue) {
                                outbox.tx.send(queued).ok();
                            }
                            drop(outbox);
                            events.unbounded_send(ServerEvent::Ready).ok();
                        }
                        Err(error) => {
                            stop_reason = Some(format!("initialize failed: {error}"));
                            break;
                        }
                    }
                } else if let Some(id) = id.as_i64()
                    && let Some(tx) = lock(&shared.pending).remove(&id)
                {
                    tx.send(reply).ok();
                }
            }
            (None, None) => {}
        }
    }
    shared.fail_pending("the language server stopped");
    // Give the stderr thread a moment to collect the reason.
    std::thread::sleep(std::time::Duration::from_millis(100));
    let message = stop_reason.unwrap_or_else(|| {
        let tail = lock(&shared.stderr);
        let last: Vec<&str> = tail.lines().rev().take(4).collect();
        let last: Vec<&str> = last.into_iter().rev().collect();
        last.join("\n").trim().to_string()
    });
    events.unbounded_send(ServerEvent::Stopped { message }).ok();
}

/// Default answers to requests a server sends us.
fn answer_server_request(id: Value, method: &str, params: &Value) -> Value {
    let result = match method {
        "workspace/configuration" => {
            let count = params["items"].as_array().map_or(0, Vec::len);
            Value::Array(vec![Value::Null; count])
        }
        "window/workDoneProgress/create"
        | "client/registerCapability"
        | "client/unregisterCapability"
        | "window/showMessageRequest"
        | "workspace/codeLens/refresh"
        | "workspace/semanticTokens/refresh"
        | "workspace/inlayHint/refresh"
        | "workspace/diagnostic/refresh" => Value::Null,
        "workspace/workspaceFolders" => Value::Null,
        _ => {
            return json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("unsupported: {method}") },
            });
        }
    };
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn initialize_params(root: &Path, initialization_options: Option<Value>) -> Value {
    let root_uri = super::path_to_uri(root);
    let name = root
        .file_name()
        .map_or_else(|| "root".to_string(), |n| n.to_string_lossy().to_string());
    let mut params = json!({
        "processId": std::process::id(),
        "clientInfo": { "name": "FerrumIDE" },
        "rootUri": root_uri,
        "rootPath": root.to_string_lossy(),
        "workspaceFolders": [{ "uri": root_uri, "name": name }],
        "capabilities": {
            "general": { "positionEncodings": ["utf-8", "utf-16"] },
            "workspace": {
                "configuration": true,
                "workspaceFolders": true,
                "applyEdit": true,
                "workspaceEdit": {
                    "documentChanges": true,
                    "resourceOperations": ["create", "rename", "delete"],
                    "failureHandling": "abort",
                },
                "executeCommand": { "dynamicRegistration": false },
                "didChangeWatchedFiles": { "dynamicRegistration": false },
            },
            "window": { "workDoneProgress": true },
            "textDocument": {
                "synchronization": { "didSave": true, "dynamicRegistration": false },
                "publishDiagnostics": { "relatedInformation": false, "versionSupport": true },
                "completion": {
                    "completionItem": {
                        "snippetSupport": true,
                        "documentationFormat": ["markdown", "plaintext"],
                        "insertReplaceSupport": true,
                        "labelDetailsSupport": true,
                        "deprecatedSupport": true,
                        "resolveSupport": {
                            "properties": ["documentation", "detail", "additionalTextEdits"],
                        },
                    },
                    "contextSupport": true,
                },
                "hover": { "contentFormat": ["markdown", "plaintext"] },
                "definition": { "linkSupport": true },
                "references": { "dynamicRegistration": false },
                "rename": { "prepareSupport": true, "honorsChangeAnnotations": false },
                "codeAction": {
                    "codeActionLiteralSupport": {
                        "codeActionKind": {
                            "valueSet": [
                                "", "quickfix", "refactor", "refactor.extract",
                                "refactor.inline", "refactor.rewrite", "source",
                                "source.organizeImports",
                            ],
                        },
                    },
                    "isPreferredSupport": true,
                    "disabledSupport": true,
                    "dataSupport": true,
                    "resolveSupport": { "properties": ["edit", "command"] },
                },
                "formatting": { "dynamicRegistration": false },
                "signatureHelp": {
                    "signatureInformation": {
                        "documentationFormat": ["markdown", "plaintext"],
                        "parameterInformation": { "labelOffsetSupport": true },
                        "activeParameterSupport": true,
                    },
                    "contextSupport": true,
                },
            },
        },
    });
    if let Some(options) = initialization_options {
        params["initializationOptions"] = options;
    }
    params
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use futures::channel::mpsc::unbounded;
    use futures::executor::block_on;
    use std::io::BufReader;

    /// A scripted server on pipes: answers initialize, echoes requests and
    /// publishes diagnostics after `didOpen`.
    fn fake_server() -> (
        Client,
        futures::channel::mpsc::UnboundedReceiver<ServerEvent>,
    ) {
        let (client_read, mut server_write) = std::io::pipe().expect("test value");
        let (server_read, client_write) = std::io::pipe().expect("test value");
        std::thread::spawn(move || {
            let mut input = BufReader::new(server_read);
            let mut seen_initialized = false;
            let mut apply_request = Value::Null;
            while let Ok(Some(message)) = read_message(&mut input) {
                let method = message["method"].as_str().unwrap_or_default().to_string();
                match method.as_str() {
                    "initialize" => {
                        // Ask the client something before answering.
                        write_message(
                            &mut server_write,
                            &json!({"jsonrpc": "2.0", "id": "cfg", "method": "workspace/configuration",
                                    "params": {"items": [{}, {}]}}),
                        )
                        .expect("test value");
                        write_message(
                            &mut server_write,
                            &json!({"jsonrpc": "2.0", "id": 0,
                                    "result": {"capabilities": {"positionEncoding": "utf-8"}}}),
                        )
                        .expect("test value");
                    }
                    "initialized" => seen_initialized = true,
                    "textDocument/didOpen" => {
                        assert!(seen_initialized, "didOpen before initialized");
                        write_message(
                            &mut server_write,
                            &json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
                                    "params": {"uri": message["params"]["textDocument"]["uri"], "diagnostics": []}}),
                        )
                        .expect("test value");
                    }
                    "echo" => {
                        write_message(
                            &mut server_write,
                            &json!({"jsonrpc": "2.0", "id": message["id"], "result": message["params"]}),
                        )
                        .expect("test value");
                    }
                    "fail" => {
                        write_message(
                            &mut server_write,
                            &json!({"jsonrpc": "2.0", "id": message["id"],
                                    "error": {"code": 1, "message": "nope"}}),
                        )
                        .expect("test value");
                    }
                    "" if message["id"] == "cfg" => {
                        assert_eq!(message["result"], json!([null, null]));
                    }
                    // Asks the client to apply an edit, then reports the
                    // client's answer as this request's result.
                    "apply" => {
                        apply_request = message["id"].clone();
                        write_message(
                            &mut server_write,
                            &json!({"jsonrpc": "2.0", "id": "ae", "method": "workspace/applyEdit",
                                    "params": {"label": "fix", "edit": {"changes": {}}}}),
                        )
                        .expect("test value");
                    }
                    "" if message["id"] == "ae" => {
                        write_message(
                            &mut server_write,
                            &json!({"jsonrpc": "2.0", "id": apply_request, "result": message["result"]}),
                        )
                        .expect("test value");
                    }
                    "exit" => break,
                    _ => {}
                }
            }
        });
        let (tx, rx) = unbounded();
        let client = Client::start(
            "fake",
            client_read,
            client_write,
            Path::new("/tmp/project"),
            None,
            tx,
            None,
        );
        (client, rx)
    }

    #[test]
    fn handshake_queues_messages_until_ready() {
        let (client, mut events) = fake_server();
        // Sent before the server answered initialize: must be held back.
        client.notify(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": "file:///tmp/project/a.rs"}}),
        );
        let echo = client.request("echo", json!({"x": 1}));
        assert_eq!(block_on(echo), Ok(json!({"x": 1})));
        assert!(client.is_ready());
        assert_eq!(client.position_encoding(), PositionEncoding::Utf8);
        assert_eq!(block_on(events.next()), Some(ServerEvent::Ready));
        match block_on(events.next()) {
            Some(ServerEvent::Notification { method, params }) => {
                assert_eq!(method, "textDocument/publishDiagnostics");
                assert_eq!(params["uri"], "file:///tmp/project/a.rs");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(
            block_on(client.request("fail", Value::Null)),
            Err("nope".to_string())
        );
    }

    #[test]
    fn apply_edit_requests_reach_the_app() {
        let (client, mut events) = fake_server();
        let answer = client.request("apply", Value::Null);
        let request = loop {
            match block_on(events.next()) {
                Some(ServerEvent::Request { id, method, params }) => break (id, method, params),
                Some(_) => {}
                None => panic!("events ended"),
            }
        };
        assert_eq!(request.1, "workspace/applyEdit");
        assert_eq!(request.2["label"], "fix");
        client.reply(request.0, json!({"applied": true}));
        assert_eq!(block_on(answer), Ok(json!({"applied": true})));
    }

    #[test]
    fn initialize_advertises_editing_features() {
        let params = initialize_params(Path::new("/tmp/p"), None);
        let caps = &params["capabilities"];
        assert_eq!(caps["workspace"]["applyEdit"], true);
        assert_eq!(caps["workspace"]["workspaceEdit"]["documentChanges"], true);
        assert_eq!(caps["textDocument"]["rename"]["prepareSupport"], true);
        assert!(caps["textDocument"]["codeAction"]["codeActionLiteralSupport"].is_object());
        assert!(caps["textDocument"]["formatting"].is_object());
        assert!(caps["textDocument"]["signatureHelp"].is_object());
        assert!(caps["textDocument"]["references"].is_object());
    }

    #[test]
    fn stopping_fails_pending_requests() {
        let (client, mut events) = fake_server();
        block_on(client.request("echo", Value::Null)).expect("test value");
        client.notify("exit", Value::Null);
        let pending = client.request("never", Value::Null);
        assert!(block_on(pending).is_err());
        let mut stopped = false;
        while let Some(event) = block_on(events.next()) {
            if matches!(event, ServerEvent::Stopped { .. }) {
                stopped = true;
                break;
            }
        }
        assert!(stopped);
    }

    #[test]
    fn missing_program_is_a_spawn_error() {
        let (tx, _rx) = unbounded();
        let launch = Launch {
            program: "/nonexistent/ferrum-no-such-server".into(),
            args: vec![],
            path_env: None,
        };
        assert!(Client::spawn("x", &launch, Path::new("/"), None, tx).is_err());
    }
}
