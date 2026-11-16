//! End-to-end agent status detection: stand-in agents (shell scripts named
//! `codex` / `claude` that draw what the real ones do) in real PTYs, driven
//! like a user would, with the status changes timed.

#![allow(clippy::unwrap_used)]

use ferrumterm_client::{Client, Endpoint};
use ferrumterm_protocol::{
    AwakeMode, BlockId, Event, Input, ProgramState, Request, Response, Spawn,
};
use ferrumterm_server::{ServerConfig, run};
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;

/// How long the stand-in agents work on a prompt.
const TURN: Duration = Duration::from_millis(1_500);

/// Codex: an idle screen that keeps printing (here: re-setting its title),
/// and "esc to interrupt" while it works.
const CODEX: &str = r#"#!/bin/sh
( while :; do printf '\033]0;Ready\007'; sleep 0.25; done ) &
trap 'kill $! 2>/dev/null' EXIT
printf '› '
while read line; do
  i=0
  while [ $i -lt 6 ]; do
    printf '\r\033[2K• Working (%ss • esc to interrupt)' $i
    sleep 0.25
    i=$((i+1))
  done
  printf '\r\033[2K• %s: done\n› ' "$line"
done
"#;

/// Claude Code: a braille spinner in the title while it works.
const CLAUDE: &str = r#"#!/bin/sh
printf '\033]0;✳ Claude Code\007> '
while read line; do
  for f in ⠂ ⠐ ⠂ ⠐ ⠂ ⠐; do printf '\033]0;%s %s\007' "$f" "$line"; sleep 0.25; done
  printf '\033]0;✳ %s\007● Done.\n> ' "$line"
done
"#;

async fn start(dir: &std::path::Path) -> (Client, UnboundedReceiver<Event>) {
    let socket = dir.join("s.sock");
    let config = ServerConfig {
        state_dir: dir.to_path_buf(),
        socket: socket.clone(),
        tcp: Vec::new(),
        awake: AwakeMode::Never,
        notify: false,
    };
    tokio::spawn(async move { run(config).await.unwrap() });
    for _ in 0..100 {
        if let Ok(connected) = Client::connect(&Endpoint::Unix(socket.clone()), "test").await {
            return connected;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server did not start");
}

/// Waits for `block`'s next status event; `None` on timeout.
async fn next_status(
    events: &mut UnboundedReceiver<Event>,
    block: BlockId,
    timeout: Duration,
) -> Option<ProgramState> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Some(Event::Status { block: b, status })) if b == block => {
                return status.map(|s| s.state);
            }
            Ok(Some(_)) => {}
            _ => return None,
        }
    }
}

/// Starts the stand-in `agent`, lets it settle, submits a prompt and checks
/// the status follows: working at once, done soon after the turn, nothing
/// in between and nothing while idle.
async fn follows_a_turn(agent: &str, script: &str) {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join(agent);
    std::fs::write(&program, script).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (c, mut events) = start(dir.path()).await;
    c.request(Request::Watch).await.unwrap();
    let Response::Created { block, .. } = c
        .request(Request::NewSession {
            name: None,
            spawn: Spawn {
                command: vec![program.display().to_string()],
                ..Spawn::default()
            },
        })
        .await
        .unwrap()
    else {
        panic!()
    };

    // Idle, however much it prints.
    let mut idle = Vec::new();
    while let Some(s) = next_status(&mut events, block, Duration::from_millis(2_500)).await {
        idle.push(s);
    }
    assert!(
        !idle.contains(&ProgramState::Working),
        "{agent}: working before any prompt: {idle:?}"
    );

    let submitted = Instant::now();
    c.request(Request::Input {
        target: Some(block.to_string()),
        input: vec![Input::Text("fix it".into()), Input::Key("Enter".into())],
    })
    .await
    .unwrap();
    let working = next_status(&mut events, block, Duration::from_secs(5)).await;
    let to_working = submitted.elapsed();
    assert_eq!(working, Some(ProgramState::Working), "{agent}");
    assert!(
        to_working < Duration::from_millis(700),
        "{agent}: working after {to_working:?}"
    );

    let done = next_status(&mut events, block, Duration::from_secs(10)).await;
    let to_done = submitted.elapsed();
    assert_eq!(
        done,
        Some(ProgramState::Done),
        "{agent}: no flicker before done"
    );
    assert!(
        to_done < TURN + Duration::from_secs(1),
        "{agent}: done {:?} after the turn ended",
        to_done.saturating_sub(TURN)
    );
    // And it stays done while it sits there.
    assert_eq!(
        next_status(&mut events, block, Duration::from_millis(1_500)).await,
        None,
        "{agent}"
    );
}

/// Codex never left (or never entered) working: its idle redraws counted
/// as work and a submitted prompt did not.
#[tokio::test(flavor = "multi_thread")]
async fn codex_status_follows_its_turns() {
    follows_a_turn("codex", CODEX).await;
}

/// Claude Code went idle seconds after the turn ended.
#[tokio::test(flavor = "multi_thread")]
async fn claude_status_follows_its_turns() {
    follows_a_turn("claude", CLAUDE).await;
}
