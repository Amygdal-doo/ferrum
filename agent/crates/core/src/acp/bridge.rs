//! Our side of an ACP connection: maps `session/update` into
//! [`ThreadEvent`]s and serves the agent's requests — permission prompts
//! through the session's [`PermissionBroker`] and policy, file reads from
//! the project (sandboxed), file writes through the [`ChangeSet`], and
//! terminals as child processes whose output we keep.
//!
//! In Ask and Plan modes writes, terminals and edit/execute permissions
//! are refused, whatever the agent's own mode.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::sync::{oneshot, watch};

use crate::changeset::ChangeSet;
use crate::jsonrpc::{Handler, RpcError};
use crate::modes::Mode;
use crate::permissions::{Decision, PermissionBroker, PermissionRequest, Permissions, Policy};
use crate::provider::Usage;
use crate::thread::{DiffPreview, PlanEntry, ThreadEvent, ToolCallRecord, ToolStatus};
use crate::tools::{Access, Sandbox};

pub type Emit = Arc<dyn Fn(ThreadEvent) + Send + Sync>;
pub type Notify = Arc<dyn Fn() + Send + Sync>;

/// What a bridge needs from the session.
pub struct BridgeConfig {
    pub agent_id: String,
    pub sandbox: Sandbox,
    pub changes: Arc<Mutex<ChangeSet>>,
    pub emit: Emit,
    pub changes_updated: Notify,
    pub broker: Arc<dyn PermissionBroker>,
    pub permissions: Arc<Mutex<Permissions>>,
    pub mode: Arc<Mutex<Mode>>,
}

pub struct AcpBridge {
    config: BridgeConfig,
    session: Mutex<Option<String>>,
    terminals: Mutex<HashMap<String, Arc<Terminal>>>,
    next_id: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// ACP tool kind → our access class.
pub fn kind_access(kind: &str) -> Access {
    match kind {
        "read" | "search" | "think" | "fetch" | "switch_mode" => Access::Read,
        "edit" | "delete" | "move" => Access::Write,
        "execute" => Access::Execute,
        _ => Access::External,
    }
}

fn status(value: &Value) -> Option<ToolStatus> {
    Some(match value.as_str()? {
        "pending" => ToolStatus::Pending,
        "in_progress" => ToolStatus::Running,
        "completed" => ToolStatus::Completed,
        "failed" => ToolStatus::Failed,
        _ => return None,
    })
}

/// Text, diffs and terminal output of a tool call's `content`.
fn content(
    items: &Value,
    terminal: &dyn Fn(&str) -> Option<String>,
) -> (Option<String>, Vec<DiffPreview>) {
    let Some(items) = items.as_array() else {
        return (None, Vec::new());
    };
    let mut text = String::new();
    let mut diffs = Vec::new();
    for item in items {
        match item["type"].as_str() {
            Some("content") => {
                if let Some(t) = item["content"]["text"].as_str() {
                    text.push_str(t);
                    text.push('\n');
                }
            }
            Some("diff") => diffs.push(DiffPreview {
                path: PathBuf::from(item["path"].as_str().unwrap_or_default()),
                old_text: item["oldText"].as_str().map(str::to_string),
                new_text: item["newText"].as_str().unwrap_or_default().to_string(),
            }),
            Some("terminal") => {
                if let Some(out) = item["terminalId"].as_str().and_then(terminal) {
                    text.push_str(&out);
                }
            }
            _ => {}
        }
    }
    (
        (!text.is_empty()).then(|| text.trim_end().to_string()),
        diffs,
    )
}

/// One `session/update` → thread events.
pub fn map_update(update: &Value, terminal: &dyn Fn(&str) -> Option<String>) -> Vec<ThreadEvent> {
    let chunk_text = |c: &Value| -> Option<String> {
        match c["type"].as_str()? {
            "text" => c["text"].as_str().map(str::to_string),
            "resource_link" => Some(format!(
                "[{}]({})",
                c["name"].as_str().unwrap_or("link"),
                c["uri"].as_str().unwrap_or_default()
            )),
            "image" => Some("[image]".into()),
            _ => None,
        }
    };
    match update["sessionUpdate"].as_str().unwrap_or_default() {
        "agent_message_chunk" => chunk_text(&update["content"])
            .map(ThreadEvent::TextDelta)
            .into_iter()
            .collect(),
        "agent_thought_chunk" => chunk_text(&update["content"])
            .map(ThreadEvent::ThinkingDelta)
            .into_iter()
            .collect(),
        "tool_call" => {
            let kind = update["kind"].as_str().unwrap_or("other");
            let (output, diffs) = content(&update["content"], terminal);
            let raw_output = update.get("rawOutput").filter(|v| !v.is_null());
            vec![ThreadEvent::ToolCallStarted(ToolCallRecord {
                id: update["toolCallId"].as_str().unwrap_or_default().into(),
                name: kind.into(),
                title: update["title"].as_str().unwrap_or(kind).into(),
                input: update.get("rawInput").cloned().unwrap_or(Value::Null),
                access: Some(kind_access(kind)),
                status: status(&update["status"]).unwrap_or(ToolStatus::Pending),
                output: output.or_else(|| raw_output.map(value_text)),
                is_error: update["status"] == "failed",
                changed: Vec::new(),
                diffs,
            })]
        }
        "tool_call_update" => {
            let (output, diffs) = content(&update["content"], terminal);
            let raw_output = update.get("rawOutput").filter(|v| !v.is_null());
            let st = status(&update["status"]);
            vec![ThreadEvent::ToolCallUpdated {
                id: update["toolCallId"].as_str().unwrap_or_default().into(),
                status: st,
                title: update["title"].as_str().map(str::to_string),
                output: output.or_else(|| raw_output.map(value_text)),
                is_error: st.map(|s| s == ToolStatus::Failed),
                changed: Vec::new(),
                diffs,
            }]
        }
        "plan" => vec![ThreadEvent::PlanEntries(
            update["entries"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| PlanEntry {
                    content: e["content"].as_str().unwrap_or_default().into(),
                    status: e["status"].as_str().unwrap_or("pending").into(),
                    priority: e["priority"].as_str().unwrap_or_default().into(),
                })
                .collect(),
        )],
        "usage_update" => {
            let mut out = vec![ThreadEvent::Usage {
                usage: Usage::default(),
                context_tokens: update["used"].as_u64().unwrap_or(0),
            }];
            if let Some(size) = update["size"].as_u64() {
                out.push(ThreadEvent::ContextWindow(size));
            }
            out
        }
        _ => Vec::new(),
    }
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// A command the agent runs through `terminal/create`.
struct Terminal {
    output: Mutex<(Vec<u8>, bool)>,
    limit: usize,
    exit: watch::Receiver<Option<(Option<i32>, Option<String>)>>,
    kill: Mutex<Option<oneshot::Sender<()>>>,
}

impl Terminal {
    fn push(&self, bytes: &[u8]) {
        let mut out = lock(&self.output);
        out.0.extend_from_slice(bytes);
        if out.0.len() > self.limit {
            let mut cut = out.0.len() - self.limit;
            while cut < out.0.len() && (out.0[cut] & 0xC0) == 0x80 {
                cut += 1;
            }
            out.0.drain(..cut);
            out.1 = true;
        }
    }

    fn text(&self) -> (String, bool) {
        let out = lock(&self.output);
        (String::from_utf8_lossy(&out.0).to_string(), out.1)
    }

    fn exit_json(status: &(Option<i32>, Option<String>)) -> Value {
        json!({"exitCode": status.0, "signal": status.1})
    }
}

impl AcpBridge {
    pub fn new(config: BridgeConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            session: Mutex::new(None),
            terminals: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        })
    }

    pub fn set_session(&self, id: &str) {
        *lock(&self.session) = Some(id.into());
    }

    fn mode(&self) -> Mode {
        *lock(&self.config.mode)
    }

    fn terminal_output(&self, id: &str) -> Option<String> {
        let terminal = lock(&self.terminals).get(id).cloned()?;
        Some(terminal.text().0)
    }

    fn emit(&self, event: ThreadEvent) {
        (self.config.emit)(event);
    }

    fn path(&self, params: &Value, write: bool) -> Result<PathBuf, RpcError> {
        let raw = params["path"]
            .as_str()
            .ok_or_else(|| RpcError::invalid_params("missing path"))?;
        let sandbox = &self.config.sandbox;
        let resolved = if write {
            sandbox.resolve_for_write(raw)
        } else {
            sandbox.resolve(raw)
        };
        resolved.map_err(|e| RpcError::new(-32002, e.to_string()))
    }

    fn read_text_file(&self, params: &Value) -> Result<Value, RpcError> {
        let path = self.path(params, false)?;
        let text = std::fs::read_to_string(&path)
            .map_err(|e| RpcError::new(-32002, format!("{}: {e}", path.display())))?;
        let line = params["line"].as_u64().map_or(1, |l| l.max(1)) as usize;
        let content = match params["limit"].as_u64() {
            None if line == 1 => text,
            limit => text
                .split_inclusive('\n')
                .skip(line - 1)
                .take(limit.map_or(usize::MAX, |l| l as usize))
                .collect(),
        };
        Ok(json!({"content": content}))
    }

    fn write_text_file(&self, params: &Value) -> Result<Value, RpcError> {
        if self.mode() != Mode::Agent {
            return Err(RpcError::new(
                -32003,
                format!(
                    "{} mode is read-only: no files may be written",
                    self.mode().label()
                ),
            ));
        }
        let path = self.path(params, true)?;
        let content = params["content"]
            .as_str()
            .ok_or_else(|| RpcError::invalid_params("missing content"))?;
        let rel = self.config.sandbox.relative(&path);
        lock(&self.config.changes)
            .write(&rel, Some(content.as_bytes()))
            .map_err(|e| RpcError::internal(e.to_string()))?;
        (self.config.changes_updated)();
        Ok(json!({}))
    }

    async fn request_permission(&self, params: Value) -> Result<Value, RpcError> {
        let call = &params["toolCall"];
        let call_id = call["toolCallId"].as_str().unwrap_or_default().to_string();
        let kind = call["kind"].as_str().unwrap_or("other").to_string();
        let access = kind_access(&kind);
        let title = call["title"].as_str().unwrap_or(&kind).to_string();
        let options: Vec<(String, String)> = params["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| Some((o["optionId"].as_str()?.into(), o["kind"].as_str()?.into())))
            .collect();
        let pick = |kinds: &[&str]| -> Value {
            kinds
                .iter()
                .find_map(|k| options.iter().find(|(_, kind)| kind == k))
                .map(|(id, _)| json!({"outcome": {"outcome": "selected", "optionId": id}}))
                .unwrap_or_else(|| json!({"outcome": {"outcome": "cancelled"}}))
        };
        let allow = || pick(&["allow_once", "allow_always"]);
        let reject = || pick(&["reject_once", "reject_always"]);

        if !self.mode().allows(access) && access != Access::External {
            self.emit(ThreadEvent::ToolCallUpdated {
                id: call_id,
                status: Some(ToolStatus::Denied),
                title: None,
                output: Some(format!("Not allowed in {} mode.", self.mode().label())),
                is_error: Some(true),
                changed: Vec::new(),
                diffs: Vec::new(),
            });
            return Ok(reject());
        }
        let key = format!("acp:{}:{kind}", self.config.agent_id);
        let policy = lock(&self.config.permissions).policy(&key, access);
        match policy {
            Policy::Allow => return Ok(allow()),
            Policy::Deny => {
                self.emit(ThreadEvent::tool_status(&call_id, ToolStatus::Denied));
                return Ok(reject());
            }
            Policy::Ask => {}
        }
        self.emit(ThreadEvent::tool_status(
            &call_id,
            ToolStatus::AwaitingPermission,
        ));
        let decision = self
            .config
            .broker
            .ask(PermissionRequest {
                id: self.next_id.fetch_add(1, Ordering::SeqCst),
                tool_call_id: call_id.clone(),
                tool: key.clone(),
                title,
                input: call.get("rawInput").cloned().unwrap_or(Value::Null),
                access,
            })
            .await;
        lock(&self.config.permissions).record(&key, decision);
        Ok(match decision {
            Decision::Deny => {
                self.emit(ThreadEvent::tool_status(&call_id, ToolStatus::Denied));
                reject()
            }
            Decision::AllowOnce => {
                self.emit(ThreadEvent::tool_status(&call_id, ToolStatus::Running));
                allow()
            }
            Decision::AllowSession | Decision::AllowAlways => {
                self.emit(ThreadEvent::tool_status(&call_id, ToolStatus::Running));
                pick(&["allow_always", "allow_once"])
            }
        })
    }

    fn terminal_create(&self, params: &Value) -> Result<Value, RpcError> {
        if self.mode() != Mode::Agent {
            return Err(RpcError::new(
                -32003,
                format!(
                    "{} mode is read-only: no commands may run",
                    self.mode().label()
                ),
            ));
        }
        let command = params["command"]
            .as_str()
            .ok_or_else(|| RpcError::invalid_params("missing command"))?;
        let args: Vec<String> = params["args"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a.as_str().map(str::to_string))
            .collect();
        let env: BTreeMap<String, String> = params["env"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| Some((e["name"].as_str()?.into(), e["value"].as_str()?.into())))
            .collect();
        let cwd = match params["cwd"].as_str() {
            Some(cwd) => self
                .config
                .sandbox
                .resolve(cwd)
                .map_err(|e| RpcError::new(-32002, e.to_string()))?,
            None => self.config.sandbox.root().to_path_buf(),
        };
        let limit = params["outputByteLimit"]
            .as_u64()
            .map_or(1 << 20, |l| l as usize);
        let id = format!("term-{}", self.next_id.fetch_add(1, Ordering::SeqCst));
        let terminal =
            spawn_terminal(command, &args, &env, &cwd, limit).map_err(RpcError::internal)?;
        lock(&self.terminals).insert(id.clone(), terminal);
        Ok(json!({"terminalId": id}))
    }

    fn terminal(&self, params: &Value) -> Result<Arc<Terminal>, RpcError> {
        let id = params["terminalId"].as_str().unwrap_or_default();
        lock(&self.terminals)
            .get(id)
            .cloned()
            .ok_or_else(|| RpcError::invalid_params(format!("no terminal {id}")))
    }
}

fn spawn_terminal(
    command: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
    cwd: &Path,
    limit: usize,
) -> Result<Arc<Terminal>, String> {
    let mut cmd = tokio::process::Command::new(command);
    cmd.args(args)
        .envs(env)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| format!("{command}: {e}"))?;
    let (exit_tx, exit_rx) = watch::channel(None);
    let (kill_tx, kill_rx) = oneshot::channel::<()>();
    let terminal = Arc::new(Terminal {
        output: Mutex::new((Vec::new(), false)),
        limit,
        exit: exit_rx,
        kill: Mutex::new(Some(kill_tx)),
    });
    let readers: Vec<Box<dyn tokio::io::AsyncRead + Send + Unpin>> = [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Send + Unpin>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Send + Unpin>),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut reads = Vec::new();
    for mut reader in readers {
        let terminal = terminal.clone();
        reads.push(tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                terminal.push(&buf[..n]);
            }
        }));
    }
    tokio::spawn(async move {
        let status = tokio::select! {
            status = child.wait() => status.ok(),
            _ = kill_rx => {
                let _ = child.kill().await;
                child.wait().await.ok()
            }
        };
        for read in reads {
            let _ = read.await;
        }
        let code = status.and_then(|s| s.code());
        #[cfg(unix)]
        let signal = status
            .and_then(|s| std::os::unix::process::ExitStatusExt::signal(&s))
            .map(|s| s.to_string());
        #[cfg(not(unix))]
        let signal: Option<String> = None;
        let _ = exit_tx.send(Some((code, signal)));
    });
    Ok(terminal)
}

#[async_trait::async_trait]
impl Handler for AcpBridge {
    async fn request(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "session/request_permission" => self.request_permission(params).await,
            "fs/read_text_file" => self.read_text_file(&params),
            "fs/write_text_file" => self.write_text_file(&params),
            "terminal/create" => self.terminal_create(&params),
            "terminal/output" => {
                let terminal = self.terminal(&params)?;
                let (output, truncated) = terminal.text();
                let mut result = json!({"output": output, "truncated": truncated});
                if let Some(status) = terminal.exit.borrow().as_ref() {
                    result["exitStatus"] = Terminal::exit_json(status);
                }
                Ok(result)
            }
            "terminal/wait_for_exit" => {
                let terminal = self.terminal(&params)?;
                let mut exit = terminal.exit.clone();
                let status = exit
                    .wait_for(Option::is_some)
                    .await
                    .map_err(|_| RpcError::internal("terminal lost"))?
                    .clone()
                    .unwrap_or((None, None));
                Ok(Terminal::exit_json(&status))
            }
            "terminal/kill" => {
                let terminal = self.terminal(&params)?;
                if let Some(kill) = lock(&terminal.kill).take() {
                    let _ = kill.send(());
                }
                Ok(json!({}))
            }
            "terminal/release" => {
                let terminal = self.terminal(&params)?;
                if let Some(kill) = lock(&terminal.kill).take() {
                    let _ = kill.send(());
                }
                let id = params["terminalId"].as_str().unwrap_or_default();
                lock(&self.terminals).remove(id);
                Ok(json!({}))
            }
            other => Err(RpcError::method_not_found(other)),
        }
    }

    async fn notification(&self, method: &str, params: Value) {
        if method != "session/update" {
            return;
        }
        let ours = lock(&self.session).clone();
        if let (Some(ours), Some(theirs)) = (ours, params["sessionId"].as_str())
            && ours != theirs
        {
            return;
        }
        for event in map_update(&params["update"], &|id| self.terminal_output(id)) {
            self.emit(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_map_to_thread_events() {
        let none = |_: &str| -> Option<String> { None };
        assert_eq!(
            map_update(
                &json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "Hi"}}),
                &none
            ),
            [ThreadEvent::TextDelta("Hi".into())]
        );
        let started = map_update(
            &json!({"sessionUpdate": "tool_call", "toolCallId": "c1", "title": "Edit a.rs", "kind": "edit", "status": "pending",
                    "content": [{"type": "diff", "path": "/p/a.rs", "oldText": "x", "newText": "y"}], "rawInput": {"p": 1}}),
            &none,
        );
        match &started[0] {
            ThreadEvent::ToolCallStarted(call) => {
                assert_eq!(call.access, Some(Access::Write));
                assert_eq!(call.status, ToolStatus::Pending);
                assert_eq!(call.diffs[0].new_text, "y");
                assert_eq!(call.input, json!({"p": 1}));
            }
            other => panic!("{other:?}"),
        }
        let term = |id: &str| (id == "t1").then(|| "built ok".to_string());
        let updated = map_update(
            &json!({"sessionUpdate": "tool_call_update", "toolCallId": "c2", "status": "completed",
                    "content": [{"type": "content", "content": {"type": "text", "text": "done"}}, {"type": "terminal", "terminalId": "t1"}]}),
            &term,
        );
        assert!(
            matches!(&updated[0], ThreadEvent::ToolCallUpdated { status: Some(ToolStatus::Completed), output: Some(o), .. } if o == "done\nbuilt ok")
        );
        let plan = map_update(
            &json!({"sessionUpdate": "plan", "entries": [{"content": "Step", "priority": "high", "status": "in_progress"}]}),
            &none,
        );
        assert!(matches!(&plan[0], ThreadEvent::PlanEntries(e) if e[0].status == "in_progress"));
        let usage = map_update(
            &json!({"sessionUpdate": "usage_update", "used": 900, "size": 200000}),
            &none,
        );
        assert_eq!(usage.len(), 2);
        assert!(
            map_update(
                &json!({"sessionUpdate": "available_commands_update"}),
                &none
            )
            .is_empty()
        );
    }
}
