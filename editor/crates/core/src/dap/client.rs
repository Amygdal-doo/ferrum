//! A debug adapter connection: the adapter process, a writer thread, a
//! reader thread and the table of requests waiting for a response. The
//! framing is the language servers' (`Content-Length`, see
//! [`crate::lsp::transport`]); messages are DAP's (`seq` / `type`).
//!
//! Adapters talk over stdio (lldb-dap) or TCP (`dlv dap`, CodeLLDB,
//! js-debug: the client picks a free port, starts the adapter listening on
//! it and connects). Events and requests from the adapter (`runInTerminal`,
//! `startDebugging`) arrive on the [`DapEvent`] channel.

use std::collections::HashMap;
use std::future::Future;
use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::channel::{mpsc::UnboundedSender, oneshot};
use serde_json::{Value, json};

use crate::lsp::transport::{read_message, write_message};

const STDERR_LIMIT: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum DapEvent {
    Event {
        event: String,
        body: Value,
    },
    /// A reverse request; answer with [`DapClient::respond`].
    Request {
        seq: i64,
        command: String,
        arguments: Value,
    },
    /// The connection ended; `message` says why when known.
    Closed {
        message: String,
    },
}

/// How the adapter is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Stdio,
    /// The adapter listens on a port: `{port}` in its arguments is
    /// replaced by a free one.
    Tcp,
}

/// How to start an adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterCommand {
    /// Shown in messages ("lldb-dap", "dlv dap").
    pub name: String,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub path_env: Option<String>,
    pub transport: Transport,
}

type Reply = Result<Value, String>;

struct Shared {
    tx: mpsc::Sender<Value>,
    pending: Mutex<HashMap<i64, oneshot::Sender<Reply>>>,
    next_seq: AtomicI64,
    closed: AtomicBool,
    stderr: Mutex<String>,
}

impl Shared {
    fn send(&self, mut message: Value) -> i64 {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        message["seq"] = json!(seq);
        self.tx.send(message).ok();
        seq
    }

    fn fail_pending(&self, why: &str) {
        self.closed.store(true, Ordering::SeqCst);
        for (_, reply) in lock(&self.pending).drain() {
            reply.send(Err(why.to_string())).ok();
        }
    }
}

/// Kills the adapter when the last [`DapClient`] clone goes away.
struct ChildGuard(Mutex<Option<Child>>);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = lock(&self.0).take() {
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                child.kill().ok();
                child.wait().ok();
            });
        }
    }
}

#[derive(Clone)]
pub struct DapClient {
    shared: Arc<Shared>,
    guard: Arc<ChildGuard>,
    /// Where a TCP adapter listens (child sessions connect there too).
    address: Option<SocketAddr>,
}

impl DapClient {
    /// Starts the adapter and connects to it.
    pub fn spawn(
        command: &AdapterCommand,
        events: UnboundedSender<DapEvent>,
    ) -> Result<Self, String> {
        let port = match command.transport {
            Transport::Stdio => None,
            Transport::Tcp => Some(free_port().map_err(|e| format!("no free port: {e}"))?),
        };
        let args: Vec<String> = command
            .args
            .iter()
            .map(|a| match port {
                Some(port) => a.replace("{port}", &port.to_string()),
                None => a.clone(),
            })
            .collect();
        let mut process = Command::new(&command.program);
        process
            .args(&args)
            .stdin(if port.is_some() {
                Stdio::null()
            } else {
                Stdio::piped()
            })
            .stdout(if port.is_some() {
                Stdio::null()
            } else {
                Stdio::piped()
            })
            .stderr(Stdio::piped());
        if let Some(cwd) = &command.cwd {
            process.current_dir(cwd);
        }
        if let Some(path) = &command.path_env {
            process.env("PATH", path);
        }
        let mut child = process
            .spawn()
            .map_err(|e| format!("could not start {}: {e}", command.name))?;
        let stderr = child.stderr.take();
        let client = match port {
            None => {
                let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
                    return Err("adapter pipes missing".into());
                };
                Self::start(&command.name, stdout, stdin, events, Some(child))
            }
            Some(port) => {
                let address = SocketAddr::from(([127, 0, 0, 1], port));
                let stream = match connect(address, &mut child, Duration::from_secs(15)) {
                    Ok(stream) => stream,
                    Err(error) => {
                        child.kill().ok();
                        child.wait().ok();
                        return Err(format!("{}: {error}", command.name));
                    }
                };
                let mut client = Self::connect_stream(&command.name, stream, events, Some(child))?;
                client.address = Some(address);
                client
            }
        };
        if let Some(stderr) = stderr {
            let shared = client.shared.clone();
            std::thread::Builder::new()
                .name("dap-stderr".into())
                .spawn(move || collect_stderr(stderr, &shared))
                .ok();
        }
        Ok(client)
    }

    /// Another connection to a TCP adapter (js-debug child sessions).
    pub fn connect(
        address: SocketAddr,
        name: &str,
        events: UnboundedSender<DapEvent>,
    ) -> Result<Self, String> {
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))
            .map_err(|e| format!("{name}: {e}"))?;
        let mut client = Self::connect_stream(name, stream, events, None)?;
        client.address = Some(address);
        Ok(client)
    }

    fn connect_stream(
        name: &str,
        stream: TcpStream,
        events: UnboundedSender<DapEvent>,
        child: Option<Child>,
    ) -> Result<Self, String> {
        let reader = stream.try_clone().map_err(|e| e.to_string())?;
        Ok(Self::start(name, reader, stream, events, child))
    }

    /// Runs the protocol over any pair of streams (tests use pipes).
    pub fn start(
        name: &str,
        reader: impl Read + Send + 'static,
        writer: impl Write + Send + 'static,
        events: UnboundedSender<DapEvent>,
        child: Option<Child>,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<Value>();
        let shared = Arc::new(Shared {
            tx,
            pending: Mutex::new(HashMap::new()),
            next_seq: AtomicI64::new(1),
            closed: AtomicBool::new(false),
            stderr: Mutex::new(String::new()),
        });
        let mut writer = writer;
        std::thread::Builder::new()
            .name(format!("dap-write-{name}"))
            .spawn(move || {
                for message in rx {
                    if write_message(&mut writer, &message).is_err() {
                        break;
                    }
                }
            })
            .ok();
        let reader_shared = shared.clone();
        std::thread::Builder::new()
            .name(format!("dap-read-{name}"))
            .spawn(move || read_loop(reader, &reader_shared, &events))
            .ok();
        Self {
            shared,
            guard: Arc::new(ChildGuard(Mutex::new(child))),
            address: None,
        }
    }

    pub fn address(&self) -> Option<SocketAddr> {
        self.address
    }

    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::SeqCst)
    }

    /// The process id of the adapter, if the client started it.
    pub fn adapter_pid(&self) -> Option<u32> {
        lock(&self.guard.0).as_ref().map(Child::id)
    }

    pub fn stderr_tail(&self) -> String {
        lock(&self.shared.stderr).clone()
    }

    /// Sends a request; resolves with the response `body`, or its error
    /// message.
    pub fn request(
        &self,
        command: &str,
        arguments: Value,
    ) -> impl Future<Output = Result<Value, String>> + Send + 'static {
        let (tx, rx) = oneshot::channel();
        // Register before sending so a fast reply finds its slot.
        let seq = self.shared.next_seq.fetch_add(1, Ordering::Relaxed);
        lock(&self.shared.pending).insert(seq, tx);
        if self.is_closed()
            && let Some(tx) = lock(&self.shared.pending).remove(&seq)
        {
            tx.send(Err("the debug adapter stopped".into())).ok();
        }
        self.shared
            .tx
            .send(json!({
                "seq": seq,
                "type": "request",
                "command": command,
                "arguments": arguments,
            }))
            .ok();
        async move {
            rx.await
                .unwrap_or_else(|_| Err("the debug adapter stopped".to_string()))
        }
    }

    /// Answers a reverse request.
    pub fn respond(&self, request_seq: i64, command: &str, success: bool, body: Value) {
        let mut message = json!({
            "type": "response",
            "request_seq": request_seq,
            "command": command,
            "success": success,
            "body": body,
        });
        if !success {
            message["message"] = json!("not supported");
        }
        self.shared.send(message);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// A port nothing listens on right now.
pub fn free_port() -> std::io::Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}

/// Connects once the adapter listens, giving up if it exits.
fn connect(address: SocketAddr, child: &mut Child, timeout: Duration) -> Result<TcpStream, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match TcpStream::connect_timeout(&address, Duration::from_millis(500)) {
            Ok(stream) => return Ok(stream),
            Err(error) => {
                if let Ok(Some(status)) = child.try_wait() {
                    return Err(format!("exited before listening ({status})"));
                }
                if Instant::now() > deadline {
                    return Err(format!("could not connect to {address}: {error}"));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
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

fn read_loop(reader: impl Read, shared: &Shared, events: &UnboundedSender<DapEvent>) {
    let mut reader = BufReader::new(reader);
    let mut why = String::new();
    loop {
        let message = match read_message(&mut reader) {
            Ok(Some(message)) => message,
            Ok(None) => break,
            Err(error) => {
                why = format!("bad message from the debug adapter: {error}");
                break;
            }
        };
        match message["type"].as_str() {
            Some("response") => {
                let reply = if message["success"].as_bool() == Some(true) {
                    Ok(message.get("body").cloned().unwrap_or(Value::Null))
                } else {
                    Err(error_text(&message))
                };
                if let Some(seq) = message["request_seq"].as_i64()
                    && let Some(tx) = lock(&shared.pending).remove(&seq)
                {
                    tx.send(reply).ok();
                }
            }
            Some("event") => {
                events
                    .unbounded_send(DapEvent::Event {
                        event: message["event"].as_str().unwrap_or_default().to_string(),
                        body: message.get("body").cloned().unwrap_or(Value::Null),
                    })
                    .ok();
            }
            Some("request") => {
                let seq = message["seq"].as_i64().unwrap_or(0);
                let command = message["command"].as_str().unwrap_or_default().to_string();
                let request = DapEvent::Request {
                    seq,
                    command: command.clone(),
                    arguments: message.get("arguments").cloned().unwrap_or(Value::Null),
                };
                if events.unbounded_send(request).is_err() {
                    shared.send(json!({"type": "response", "request_seq": seq,
                        "command": command, "success": false, "message": "no client"}));
                }
            }
            _ => {}
        }
    }
    shared.fail_pending("the debug adapter stopped");
    std::thread::sleep(Duration::from_millis(50));
    if why.is_empty() {
        let tail = lock(&shared.stderr);
        let last: Vec<&str> = tail.lines().rev().take(4).collect();
        why = last
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
    }
    events
        .unbounded_send(DapEvent::Closed { message: why })
        .ok();
}

/// The message of an error response (`message`, or the formatted
/// `body.error`).
fn error_text(message: &Value) -> String {
    let format = message["body"]["error"]["format"].as_str();
    let fallback = message["message"].as_str().unwrap_or("request failed");
    let mut text = format.unwrap_or(fallback).to_string();
    if let Some(variables) = message["body"]["error"]["variables"].as_object() {
        for (name, value) in variables {
            text = text.replace(&format!("{{{name}}}"), value.as_str().unwrap_or_default());
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use futures::channel::mpsc::unbounded;
    use futures::executor::block_on;

    /// A pipe adapter: answers `echo`, fails `fail`, sends an event and a
    /// reverse request after `poke`, and exits on `disconnect`.
    fn pipe_adapter() -> (
        DapClient,
        futures::channel::mpsc::UnboundedReceiver<DapEvent>,
    ) {
        let (client_read, mut adapter_write) = std::io::pipe().expect("pipe");
        let (adapter_read, client_write) = std::io::pipe().expect("pipe");
        std::thread::spawn(move || {
            let mut input = BufReader::new(adapter_read);
            let mut seq = 1000;
            let mut send = |mut m: Value, out: &mut std::io::PipeWriter| {
                seq += 1;
                m["seq"] = json!(seq);
                write_message(out, &m).expect("write");
            };
            while let Ok(Some(m)) = read_message(&mut input) {
                let command = m["command"].as_str().unwrap_or_default();
                let reply = |success: bool, body: Value| {
                    json!({"type": "response", "request_seq": m["seq"], "command": command,
                           "success": success, "body": body})
                };
                match (m["type"].as_str(), command) {
                    (Some("request"), "echo") => {
                        send(reply(true, m["arguments"].clone()), &mut adapter_write)
                    }
                    (Some("request"), "fail") => send(
                        json!({"type": "response", "request_seq": m["seq"], "command": "fail",
                               "success": false, "body": {"error": {"format": "no {what}", "variables": {"what": "luck"}}}}),
                        &mut adapter_write,
                    ),
                    (Some("request"), "poke") => {
                        send(reply(true, json!({})), &mut adapter_write);
                        send(
                            json!({"type": "event", "event": "output", "body": {"output": "hi\n"}}),
                            &mut adapter_write,
                        );
                        send(
                            json!({"type": "request", "command": "runInTerminal", "arguments": {"args": ["prog"]}}),
                            &mut adapter_write,
                        );
                    }
                    (Some("response"), "runInTerminal") => {
                        send(
                            json!({"type": "event", "event": "answered", "body": m["body"].clone()}),
                            &mut adapter_write,
                        );
                    }
                    (Some("request"), "disconnect") => {
                        send(reply(true, json!({})), &mut adapter_write);
                        break;
                    }
                    _ => {}
                }
            }
        });
        let (tx, rx) = unbounded();
        (
            DapClient::start("fake", client_read, client_write, tx, None),
            rx,
        )
    }

    #[test]
    fn requests_events_and_reverse_requests() {
        let (client, mut events) = pipe_adapter();
        assert_eq!(
            block_on(client.request("echo", json!({"x": 1}))),
            Ok(json!({"x": 1}))
        );
        assert_eq!(
            block_on(client.request("fail", Value::Null)),
            Err("no luck".into())
        );
        block_on(client.request("poke", Value::Null)).expect("poke");
        assert_eq!(
            block_on(events.next()),
            Some(DapEvent::Event {
                event: "output".into(),
                body: json!({"output": "hi\n"})
            })
        );
        let Some(DapEvent::Request {
            seq,
            command,
            arguments,
        }) = block_on(events.next())
        else {
            panic!("expected a reverse request");
        };
        assert_eq!(
            (command.as_str(), &arguments["args"][0]),
            ("runInTerminal", &json!("prog"))
        );
        client.respond(seq, "runInTerminal", true, json!({"processId": 7}));
        assert_eq!(
            block_on(events.next()),
            Some(DapEvent::Event {
                event: "answered".into(),
                body: json!({"processId": 7})
            })
        );
        block_on(client.request("disconnect", Value::Null)).expect("disconnect");
        assert!(block_on(client.request("echo", Value::Null)).is_err());
        loop {
            match block_on(events.next()) {
                Some(DapEvent::Closed { .. }) | None => break,
                Some(_) => {}
            }
        }
        assert!(client.is_closed());
    }

    #[test]
    fn missing_adapter_is_an_error() {
        let (tx, _rx) = unbounded();
        let command = AdapterCommand {
            name: "nope".into(),
            program: "/nonexistent/ferrum-no-adapter".into(),
            args: vec![],
            cwd: None,
            path_env: None,
            transport: Transport::Stdio,
        };
        assert!(DapClient::spawn(&command, tx).is_err());
    }
}
