//! Test helpers: waiting for session events, and re-running this test
//! binary as a fake server process (MCP / ACP) selected by an env var.

#![allow(dead_code)]

use std::io::{BufRead, Write};
use std::time::Duration;

use ferrumagent_core::session::SessionEvent;
use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use serde_json::{Value, json};

pub struct Events {
    rx: UnboundedReceiver<SessionEvent>,
    rt: tokio::runtime::Runtime,
    pub seen: Vec<SessionEvent>,
}

impl Events {
    pub fn new(rx: UnboundedReceiver<SessionEvent>) -> Self {
        Self {
            rx,
            rt: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("rt"),
            seen: Vec::new(),
        }
    }

    /// The first event (from now) matching `f`; panics after 20 s.
    pub fn wait<T>(&mut self, what: &str, mut f: impl FnMut(&SessionEvent) -> Option<T>) -> T {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            let next = self
                .rt
                .block_on(async { tokio::time::timeout(left, self.rx.next()).await });
            match next {
                Ok(Some(event)) => {
                    let hit = f(&event);
                    self.seen.push(event);
                    if let Some(hit) = hit {
                        return hit;
                    }
                }
                Ok(None) => panic!("events ended while waiting for {what}"),
                Err(_) => panic!("timed out waiting for {what}; saw {:#?}", self.seen),
            }
        }
    }

    pub fn idle(&mut self) {
        self.wait("the turn to end", |e| {
            matches!(e, SessionEvent::Busy(false)).then_some(())
        });
    }
}

/// This test binary, re-run as a fake process that serves `role`.
pub fn fake_process(test_name: &str) -> (String, Vec<String>) {
    let exe = std::env::current_exe().expect("current exe");
    (
        exe.to_string_lossy().to_string(),
        vec![
            test_name.into(),
            "--exact".into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ],
    )
}

/// A line-delimited JSON-RPC peer on stdin/stdout for fake processes.
pub struct Peer {
    stdin: std::io::StdinLock<'static>,
    next: u64,
}

impl Peer {
    pub fn new() -> Self {
        // libtest has printed "test name ... " without a newline.
        println!();
        Self {
            stdin: std::io::stdin().lock(),
            next: 1000,
        }
    }

    pub fn read(&mut self) -> Option<Value> {
        loop {
            let mut line = String::new();
            if self.stdin.read_line(&mut line).ok()? == 0 {
                return None;
            }
            if let Ok(v) = serde_json::from_str(line.trim()) {
                return Some(v);
            }
        }
    }

    pub fn send(&mut self, value: Value) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{value}");
        let _ = out.flush();
    }

    pub fn reply(&mut self, id: &Value, result: Value) {
        self.send(json!({"jsonrpc": "2.0", "id": id, "result": result}));
    }

    pub fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    /// Sends a request and reads until its answer (anything else that
    /// arrives meanwhile is returned in `others`).
    pub fn call(&mut self, method: &str, params: Value, others: &mut Vec<Value>) -> Value {
        self.next += 1;
        let id = self.next;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        while let Some(message) = self.read() {
            if message.get("method").is_none() && message["id"].as_u64() == Some(id) {
                return message;
            }
            others.push(message);
        }
        Value::Null
    }
}
