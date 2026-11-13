//! JSON-RPC 2.0 over newline-delimited JSON (MCP's stdio transport and
//! ACP). Both sides may send requests: outgoing requests are matched to
//! responses by id, incoming requests and notifications go to a
//! [`Handler`]. Notifications are handled in order; incoming requests run
//! concurrently (a permission prompt must not stall the stream). Lines
//! that are not JSON (a server printing a banner) are skipped.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{message} ({code})")]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(-32601, format!("method not found: {method}"))
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(-32602, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(-32603, message)
    }

    pub fn closed() -> Self {
        Self::new(-32000, "the connection closed")
    }

    fn from_value(value: &Value) -> Self {
        Self {
            code: value["code"].as_i64().unwrap_or(-32603),
            message: value["message"].as_str().unwrap_or("error").to_string(),
            data: value.get("data").cloned(),
        }
    }
}

#[async_trait::async_trait]
pub trait Handler: Send + Sync {
    async fn request(&self, method: &str, params: Value) -> Result<Value, RpcError>;
    async fn notification(&self, method: &str, params: Value);
}

/// Answers `ping`, refuses everything else.
pub struct BasicHandler;

#[async_trait::async_trait]
impl Handler for BasicHandler {
    async fn request(&self, method: &str, _: Value) -> Result<Value, RpcError> {
        match method {
            "ping" => Ok(json!({})),
            other => Err(RpcError::method_not_found(other)),
        }
    }

    async fn notification(&self, _: &str, _: Value) {}
}

type Pending = HashMap<u64, oneshot::Sender<Result<Value, RpcError>>>;
type Writer = tokio::sync::Mutex<Box<dyn AsyncWrite + Send + Unpin>>;

pub struct Connection {
    writer: Writer,
    pending: Mutex<Pending>,
    next_id: AtomicU64,
    closed: AtomicBool,
}

impl Connection {
    /// Starts reading `reader` on the current tokio runtime.
    pub fn start<R, W>(reader: R, writer: W, handler: Arc<dyn Handler>) -> Arc<Self>
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let connection = Arc::new(Self {
            writer: tokio::sync::Mutex::new(Box::new(writer)),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        });
        let this = connection.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Value>(line) {
                    Ok(message) => this.dispatch(message, &handler).await,
                    Err(_) => tracing::debug!("skipping non-JSON line: {line:.200}"),
                }
            }
            this.close();
        });
        connection
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let pending: Pending =
            std::mem::take(&mut *self.pending.lock().unwrap_or_else(PoisonError::into_inner));
        for (_, tx) in pending {
            let _ = tx.send(Err(RpcError::closed()));
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    async fn dispatch(self: &Arc<Self>, message: Value, handler: &Arc<dyn Handler>) {
        let method = message["method"].as_str().map(str::to_string);
        let id = message.get("id").cloned().filter(|id| !id.is_null());
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match (method, id) {
            (Some(method), Some(id)) => {
                let this = self.clone();
                let handler = handler.clone();
                tokio::spawn(async move {
                    let reply = match handler.request(&method, params).await {
                        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": {
                            "code": e.code, "message": e.message, "data": e.data,
                        }}),
                    };
                    let _ = this.write(&reply).await;
                });
            }
            (Some(method), None) => handler.notification(&method, params).await,
            (None, Some(id)) => {
                let Some(id) = id.as_u64() else { return };
                let tx = self
                    .pending
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&id);
                if let Some(tx) = tx {
                    let result = match message.get("error").filter(|e| !e.is_null()) {
                        Some(error) => Err(RpcError::from_value(error)),
                        None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                    };
                    let _ = tx.send(result);
                }
            }
            (None, None) => {}
        }
    }

    async fn write(&self, message: &Value) -> Result<(), RpcError> {
        if self.is_closed() {
            return Err(RpcError::closed());
        }
        let mut line =
            serde_json::to_vec(message).map_err(|e| RpcError::internal(e.to_string()))?;
        line.push(b'\n');
        let mut writer = self.writer.lock().await;
        writer
            .write_all(&line)
            .await
            .map_err(|_| RpcError::closed())?;
        writer.flush().await.map_err(|_| RpcError::closed())
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, tx);
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if let Err(e) = self.write(&message).await {
            self.pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&id);
            return Err(e);
        }
        rx.await.unwrap_or_else(|_| Err(RpcError::closed()))
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<(), RpcError> {
        self.write(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
            .await
    }
}

/// A child process speaking JSON-RPC on stdin/stdout; stderr is kept
/// (last lines) for error messages. Killed when dropped.
pub struct RpcProcess {
    pub connection: Arc<Connection>,
    child: tokio::sync::Mutex<tokio::process::Child>,
    stderr: Arc<Mutex<Vec<String>>>,
}

impl RpcProcess {
    pub fn spawn(
        command: &str,
        args: &[String],
        env: &BTreeMap<String, String>,
        cwd: &Path,
        handler: Arc<dyn Handler>,
    ) -> Result<Self, String> {
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args)
            .envs(env)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("could not start {command}: {e}"))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr_lines = Arc::new(Mutex::new(Vec::new()));
        if let Some(stderr) = child.stderr.take() {
            let lines = stderr_lines.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let mut lines = lines.lock().unwrap_or_else(PoisonError::into_inner);
                    lines.push(line);
                    let len = lines.len();
                    if len > 50 {
                        lines.drain(..len - 50);
                    }
                }
            });
        }
        Ok(Self {
            connection: Connection::start(stdout, stdin, handler),
            child: tokio::sync::Mutex::new(child),
            stderr: stderr_lines,
        })
    }

    /// The process's last stderr lines.
    pub fn stderr_tail(&self) -> String {
        self.stderr
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .join("\n")
    }

    pub async fn kill(&self) {
        let _ = self.child.lock().await.kill().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;

    #[async_trait::async_trait]
    impl Handler for Echo {
        async fn request(&self, method: &str, params: Value) -> Result<Value, RpcError> {
            match method {
                "echo" => Ok(params),
                other => Err(RpcError::method_not_found(other)),
            }
        }
        async fn notification(&self, _: &str, _: Value) {}
    }

    #[test]
    fn two_peers_talk_both_ways() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("rt");
        rt.block_on(async {
            let (a, b) = tokio::io::duplex(1024);
            let (a_read, a_write) = tokio::io::split(a);
            let (b_read, b_write) = tokio::io::split(b);
            let left = Connection::start(a_read, a_write, Arc::new(Echo));
            let right = Connection::start(b_read, b_write, Arc::new(BasicHandler));
            assert_eq!(
                right.request("echo", json!({"x": 1})).await,
                Ok(json!({"x": 1}))
            );
            assert_eq!(left.request("ping", json!({})).await, Ok(json!({})));
            let err = left.request("nope", json!({})).await.expect_err("error");
            assert_eq!(err.code, -32601);
            drop(right);
        });
    }
}
