//! What the editor knows about a debug session, updated from the
//! adapter's events and responses. Events return [`Followup`]s: the
//! requests the app sends next (configuration after `initialized`, the
//! stack after `stopped`). The arguments of every request are built here
//! too, so the protocol lives in one place without any I/O.
//!
//! Lines in this module are 1-based, as on the wire (`linesStartAt1`).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// `initialize` / `launch` sent, waiting for `initialized`.
    Starting,
    Running,
    Stopped {
        thread_id: Option<i64>,
        reason: String,
        description: Option<String>,
    },
    /// The debuggee ended (`exited`), the adapter may still be up.
    Exited {
        code: Option<i64>,
    },
    Terminated,
}

impl Phase {
    pub fn label(&self) -> String {
        match self {
            Phase::Starting => "starting".into(),
            Phase::Running => "running".into(),
            Phase::Stopped { reason, .. } => format!("paused ({reason})"),
            Phase::Exited { code: Some(code) } => format!("exited ({code})"),
            Phase::Exited { code: None } => "exited".into(),
            Phase::Terminated => "ended".into(),
        }
    }

    pub fn is_stopped(&self) -> bool {
        matches!(self, Phase::Stopped { .. })
    }

    pub fn is_over(&self) -> bool {
        matches!(self, Phase::Terminated)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackFrame {
    pub id: i64,
    pub name: String,
    pub path: Option<PathBuf>,
    pub line: usize,
    pub column: usize,
    /// `subtle` / `deemphasize` frames (library code) are drawn muted.
    pub subtle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub name: String,
    pub variables_reference: i64,
    pub expensive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    pub name: String,
    pub value: String,
    pub type_name: Option<String>,
    /// Non-zero: it has children, fetched with `variables`.
    pub variables_reference: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreakpointStatus {
    pub line: usize,
    pub verified: bool,
    pub message: Option<String>,
    pub id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputLine {
    /// `console`, `stdout`, `stderr`, `important`, or ours: `repl`, `result`.
    pub category: String,
    pub text: String,
}

/// Requests to send after an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Followup {
    /// `initialized`: breakpoints, exception breakpoints, `configurationDone`.
    Configure,
    /// `stopped`: threads, then the stack of this thread.
    FetchStack { thread_id: Option<i64> },
    /// `continued` / `thread`: the thread list changed.
    FetchThreads,
    /// `terminated`: the session is over.
    End,
}

#[derive(Debug, Clone, Default)]
pub struct DebugState {
    pub phase: Option<Phase>,
    pub capabilities: Value,
    pub threads: Vec<Thread>,
    /// The thread whose stack is shown.
    pub thread_id: Option<i64>,
    pub frames: Vec<StackFrame>,
    pub selected_frame: usize,
    pub scopes: Vec<Scope>,
    /// Children by `variablesReference`, fetched lazily.
    pub variables: HashMap<i64, Vec<Variable>>,
    pub output: Vec<OutputLine>,
    /// Per file, as the adapter reported them.
    pub breakpoints: BTreeMap<PathBuf, Vec<BreakpointStatus>>,
    pub exit_code: Option<i64>,
}

/// Most console lines kept.
const OUTPUT_LIMIT: usize = 5_000;

impl DebugState {
    pub fn new() -> Self {
        Self {
            phase: Some(Phase::Starting),
            ..Self::default()
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase.clone().unwrap_or(Phase::Starting)
    }

    pub fn push_output(&mut self, category: &str, text: &str) {
        // Adapters send partial lines; join onto the last of the same kind.
        if let Some(last) = self.output.last_mut()
            && last.category == category
            && !last.text.ends_with('\n')
        {
            last.text.push_str(text);
        } else {
            self.output.push(OutputLine {
                category: category.to_string(),
                text: text.to_string(),
            });
        }
        if self.output.len() > OUTPUT_LIMIT {
            let extra = self.output.len() - OUTPUT_LIMIT;
            self.output.drain(..extra);
        }
    }

    /// Applies an event; returns what to request next.
    pub fn on_event(&mut self, event: &str, body: &Value) -> Vec<Followup> {
        match event {
            "initialized" => vec![Followup::Configure],
            "stopped" => {
                let thread_id = body["threadId"].as_i64().or(self.thread_id);
                self.phase = Some(Phase::Stopped {
                    thread_id,
                    reason: body["reason"].as_str().unwrap_or("pause").to_string(),
                    description: body["description"]
                        .as_str()
                        .or_else(|| body["text"].as_str())
                        .map(str::to_string),
                });
                self.thread_id = thread_id;
                self.clear_stack();
                vec![Followup::FetchStack { thread_id }]
            }
            "continued" => {
                self.phase = Some(Phase::Running);
                self.clear_stack();
                vec![]
            }
            "thread" => vec![Followup::FetchThreads],
            "output" => {
                let category = body["category"].as_str().unwrap_or("console");
                if category != "telemetry" {
                    self.push_output(category, body["output"].as_str().unwrap_or_default());
                }
                vec![]
            }
            "breakpoint" => {
                self.on_breakpoint_event(body);
                vec![]
            }
            "exited" => {
                let code = body["exitCode"].as_i64();
                self.exit_code = code;
                self.phase = Some(Phase::Exited { code });
                self.clear_stack();
                vec![]
            }
            "terminated" => {
                self.phase = Some(Phase::Terminated);
                self.clear_stack();
                vec![Followup::End]
            }
            _ => vec![],
        }
    }

    /// The program is running again after a step / continue request.
    pub fn resumed(&mut self) {
        if !self.phase().is_over() {
            self.phase = Some(Phase::Running);
        }
        self.clear_stack();
    }

    fn clear_stack(&mut self) {
        self.frames.clear();
        self.selected_frame = 0;
        self.scopes.clear();
        self.variables.clear();
    }

    pub fn set_capabilities(&mut self, body: &Value) {
        self.capabilities = body.clone();
    }

    pub fn set_threads(&mut self, body: &Value) {
        self.threads = body["threads"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| {
                Some(Thread {
                    id: t["id"].as_i64()?,
                    name: t["name"].as_str().unwrap_or_default().to_string(),
                })
            })
            .collect();
        if self.thread_id.is_none() {
            self.thread_id = self.threads.first().map(|t| t.id);
        }
    }

    pub fn set_stack(&mut self, thread_id: i64, body: &Value) {
        self.thread_id = Some(thread_id);
        self.frames = body["stackFrames"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| {
                let hint = f["presentationHint"].as_str().unwrap_or_default();
                let source_hint = f["source"]["presentationHint"].as_str().unwrap_or_default();
                Some(StackFrame {
                    id: f["id"].as_i64()?,
                    name: f["name"].as_str().unwrap_or("?").to_string(),
                    path: f["source"]["path"].as_str().map(PathBuf::from),
                    line: f["line"].as_u64().unwrap_or(0) as usize,
                    column: f["column"].as_u64().unwrap_or(0) as usize,
                    subtle: hint == "subtle" || source_hint == "deemphasize",
                })
            })
            .collect();
        // Start at the first frame with source (skip runtime internals).
        self.selected_frame = self
            .frames
            .iter()
            .position(|f| f.path.is_some() && !f.subtle)
            .unwrap_or(0);
        self.scopes.clear();
        self.variables.clear();
    }

    pub fn selected_frame(&self) -> Option<&StackFrame> {
        self.frames.get(self.selected_frame)
    }

    /// File and 1-based line where execution is shown.
    pub fn current_location(&self) -> Option<(PathBuf, usize)> {
        let frame = self.selected_frame()?;
        Some((frame.path.clone()?, frame.line))
    }

    pub fn set_scopes(&mut self, body: &Value) {
        self.scopes = body["scopes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| Scope {
                name: s["name"].as_str().unwrap_or_default().to_string(),
                variables_reference: s["variablesReference"].as_i64().unwrap_or(0),
                expensive: s["expensive"].as_bool().unwrap_or(false),
            })
            .collect();
    }

    pub fn set_variables(&mut self, reference: i64, body: &Value) {
        self.variables.insert(reference, parse_variables(body));
    }

    /// The adapter's answer to `setBreakpoints` for `lines` of `path`.
    pub fn set_breakpoint_results(&mut self, path: &Path, lines: &[usize], body: &Value) {
        let reported = body["breakpoints"].as_array().cloned().unwrap_or_default();
        let statuses = lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let b = reported.get(i).cloned().unwrap_or(Value::Null);
                BreakpointStatus {
                    line: b["line"].as_u64().map_or(*line, |l| l as usize),
                    verified: b["verified"].as_bool().unwrap_or(false),
                    message: b["message"].as_str().map(str::to_string),
                    id: b["id"].as_i64(),
                }
            })
            .collect();
        self.breakpoints.insert(path.to_path_buf(), statuses);
    }

    fn on_breakpoint_event(&mut self, body: &Value) {
        let b = &body["breakpoint"];
        let Some(id) = b["id"].as_i64() else {
            return;
        };
        for status in self.breakpoints.values_mut().flatten() {
            if status.id == Some(id) {
                if let Some(verified) = b["verified"].as_bool() {
                    status.verified = verified;
                }
                if let Some(line) = b["line"].as_u64() {
                    status.line = line as usize;
                }
                status.message = b["message"].as_str().map(str::to_string);
            }
        }
    }

    /// Verified state of the breakpoint at 1-based `line` of `path`, if
    /// the adapter has said.
    pub fn verified(&self, path: &Path, line: usize) -> Option<bool> {
        self.breakpoints
            .get(path)?
            .iter()
            .find(|b| b.line == line)
            .map(|b| b.verified)
    }
}

pub fn parse_variables(body: &Value) -> Vec<Variable> {
    body["variables"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| Variable {
            name: v["name"].as_str().unwrap_or_default().to_string(),
            value: v["value"].as_str().unwrap_or_default().to_string(),
            type_name: v["type"].as_str().map(str::to_string),
            variables_reference: v["variablesReference"].as_i64().unwrap_or(0),
        })
        .collect()
}

/// `evaluate`'s answer as one line, and its children reference.
pub fn evaluate_result(body: &Value) -> (String, i64) {
    (
        body["result"].as_str().unwrap_or_default().to_string(),
        body["variablesReference"].as_i64().unwrap_or(0),
    )
}

// ----- Request arguments ----------------------------------------------------

pub fn initialize_args(adapter_id: &str) -> Value {
    json!({
        "clientID": "ferrum-editor",
        "clientName": "FerrumIDE",
        "adapterID": adapter_id,
        "locale": "en",
        "linesStartAt1": true,
        "columnsStartAt1": true,
        "pathFormat": "path",
        "supportsVariableType": true,
        "supportsVariablePaging": false,
        "supportsRunInTerminalRequest": true,
        "supportsStartDebuggingRequest": true,
        "supportsProgressReporting": false,
        "supportsInvalidatedEvent": false,
    })
}

/// `lines` are 1-based.
pub fn set_breakpoints_args(path: &Path, lines: &[usize]) -> Value {
    json!({
        "source": {
            "path": path.to_string_lossy(),
            "name": path.file_name().map(|n| n.to_string_lossy().to_string()),
        },
        "breakpoints": lines.iter().map(|l| json!({"line": l})).collect::<Vec<_>>(),
        "lines": lines,
        "sourceModified": false,
    })
}

pub fn thread_args(thread_id: i64) -> Value {
    json!({ "threadId": thread_id })
}

pub fn stack_trace_args(thread_id: i64) -> Value {
    json!({ "threadId": thread_id, "startFrame": 0, "levels": 200 })
}

pub fn scopes_args(frame_id: i64) -> Value {
    json!({ "frameId": frame_id })
}

pub fn variables_args(reference: i64) -> Value {
    json!({ "variablesReference": reference })
}

/// `context`: `repl`, `watch` or `hover`.
pub fn evaluate_args(expression: &str, frame_id: Option<i64>, context: &str) -> Value {
    let mut args = json!({ "expression": expression, "context": context });
    if let Some(frame) = frame_id {
        args["frameId"] = json!(frame);
    }
    args
}

pub fn disconnect_args(terminate: bool) -> Value {
    json!({ "restart": false, "terminateDebuggee": terminate })
}

/// A `runInTerminal` request as (title, shell command line, folder).
pub fn run_in_terminal(arguments: &Value) -> Option<(String, String, PathBuf)> {
    let words: Vec<String> = arguments["args"]
        .as_array()?
        .iter()
        .filter_map(|a| a.as_str().map(str::to_string))
        .collect();
    if words.is_empty() {
        return None;
    }
    let mut line = String::new();
    if let Some(env) = arguments["env"].as_object()
        && !env.is_empty()
    {
        line.push_str("env");
        for (key, value) in env {
            match value.as_str() {
                Some(value) => line.push_str(&format!(
                    " {}",
                    crate::agents::shell_quote(&format!("{key}={value}"))
                )),
                // `null` removes the variable.
                None => line.push_str(&format!(" -u {}", crate::agents::shell_quote(key))),
            }
        }
        line.push(' ');
    }
    line.push_str(&crate::agents::shell_line(&words));
    let title = arguments["title"]
        .as_str()
        .map_or_else(|| format!("debug: {}", words[0]), str::to_string);
    let cwd = arguments["cwd"]
        .as_str()
        .map(PathBuf::from)
        .unwrap_or_default();
    Some((title, line, cwd))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_drive_the_phase_and_followups() {
        let mut state = DebugState::new();
        assert_eq!(
            state.on_event("initialized", &json!({})),
            [Followup::Configure]
        );
        assert_eq!(
            state.on_event("stopped", &json!({"reason": "breakpoint", "threadId": 3})),
            [Followup::FetchStack { thread_id: Some(3) }]
        );
        assert_eq!(state.phase().label(), "paused (breakpoint)");
        state.set_stack(
            3,
            &json!({"stackFrames": [
                {"id": 1, "name": "memcpy", "line": 0, "source": {"presentationHint": "deemphasize"}},
                {"id": 2, "name": "main", "line": 7, "column": 5, "source": {"path": "/p/main.rs"}},
            ]}),
        );
        assert_eq!(state.selected_frame, 1, "skips frames without source");
        assert_eq!(
            state.current_location(),
            Some((PathBuf::from("/p/main.rs"), 7))
        );
        state.on_event("continued", &json!({"threadId": 3}));
        assert_eq!(state.phase(), Phase::Running);
        assert!(state.frames.is_empty());
        state.on_event("output", &json!({"category": "stdout", "output": "hel"}));
        state.on_event("output", &json!({"category": "stdout", "output": "lo\n"}));
        state.on_event("output", &json!({"category": "telemetry", "output": "x"}));
        assert_eq!(state.output.len(), 1);
        assert_eq!(state.output[0].text, "hello\n");
        state.on_event("exited", &json!({"exitCode": 4}));
        assert_eq!(state.phase().label(), "exited (4)");
        assert_eq!(state.on_event("terminated", &json!({})), [Followup::End]);
        assert!(state.phase().is_over());
    }

    #[test]
    fn breakpoints_are_verified_by_response_and_event() {
        let mut state = DebugState::new();
        let path = Path::new("/p/a.go");
        let args = set_breakpoints_args(path, &[3, 9]);
        assert_eq!(args["breakpoints"][1]["line"], 9);
        state.set_breakpoint_results(
            path,
            &[3, 9],
            &json!({"breakpoints": [{"id": 1, "verified": true, "line": 4}, {"id": 2, "verified": false}]}),
        );
        assert_eq!(state.verified(path, 4), Some(true), "moved by the adapter");
        assert_eq!(state.verified(path, 9), Some(false));
        state.on_event(
            "breakpoint",
            &json!({"reason": "changed", "breakpoint": {"id": 2, "verified": true}}),
        );
        assert_eq!(state.verified(path, 9), Some(true));
    }

    #[test]
    fn variables_and_run_in_terminal() {
        let vars = parse_variables(&json!({"variables": [
            {"name": "x", "value": "1", "type": "i32", "variablesReference": 0},
            {"name": "v", "value": "Vec(2)", "variablesReference": 12},
        ]}));
        assert_eq!(vars[1].variables_reference, 12);
        assert_eq!(vars[0].type_name.as_deref(), Some("i32"));
        let (title, line, cwd) = run_in_terminal(&json!({
            "kind": "integrated", "title": "Node", "cwd": "/p",
            "args": ["node", "--inspect", "my app.js"], "env": {"A": "1", "B": null}
        }))
        .expect("command");
        assert_eq!(title, "Node");
        assert_eq!(line, "env A=1 -u B node --inspect 'my app.js'");
        assert_eq!(cwd, PathBuf::from("/p"));
        assert!(run_in_terminal(&json!({"args": []})).is_none());
    }
}
