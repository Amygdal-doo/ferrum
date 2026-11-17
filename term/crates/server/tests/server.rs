//! End-to-end: a real server with real PTYs, driven through the client.

#![allow(clippy::unwrap_used)]

use ferrumterm_client::{Client, Endpoint};
use ferrumterm_protocol::{
    AwakeMode, Direction, Event, Input, Inspected, ProgramState, Request, Response, Spawn,
};
use ferrumterm_server::{ServerConfig, run};
use std::time::Duration;

struct Harness {
    _dir: tempfile::TempDir,
    client: Client,
    events: tokio::sync::mpsc::UnboundedReceiver<Event>,
    port: Option<u16>,
    state: std::path::PathBuf,
}

async fn start(tcp: bool) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("s.sock");
    let port = tcp.then(|| {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    });
    let config = ServerConfig {
        state_dir: dir.path().to_path_buf(),
        socket: socket.clone(),
        tcp: port
            .map(|p| vec![format!("127.0.0.1:{p}").parse().unwrap()])
            .unwrap_or_default(),
        awake: AwakeMode::Never,
        notify: false,
    };
    tokio::spawn(async move { run(config).await.unwrap() });
    let endpoint = Endpoint::Unix(socket);
    for _ in 0..100 {
        if let Ok((client, events)) = Client::connect(&endpoint, "test").await {
            return Harness {
                state: dir.path().to_path_buf(),
                _dir: dir,
                client,
                events,
                port,
            };
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server did not start");
}

fn sh(script: &str) -> Spawn {
    Spawn {
        command: vec!["/bin/sh".into(), "-c".into(), script.into()],
        ..Spawn::default()
    }
}

async fn capture(c: &Client, target: &str) -> String {
    match c
        .request(Request::Capture {
            target: Some(target.into()),
            scrollback: false,
        })
        .await
        .unwrap()
    {
        Response::Text { text } => text,
        r => panic!("{r:?}"),
    }
}

async fn eventually(c: &Client, target: &str, needle: &str) -> String {
    for _ in 0..200 {
        let t = capture(c, target).await;
        if t.contains(needle) {
            return t;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!(
        "{needle:?} never appeared in {target}: {:?}",
        capture(c, target).await
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sessions_input_capture_split_and_close() {
    let h = start(false).await;
    let c = &h.client;
    let Response::Created { session, block, .. } = c
        .request(Request::NewSession {
            name: Some("work".into()),
            spawn: sh("echo FTERM=$FTERM_BLOCK; cat"),
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    eventually(c, &block.to_string(), &format!("FTERM={block}")).await;
    // Input goes to the program; `cat` echoes it.
    c.request(Request::Input {
        target: Some("work".into()),
        input: vec![Input::Text("hello".into()), Input::Key("Enter".into())],
    })
    .await
    .unwrap();
    eventually(c, "work", "hello\nhello").await;
    assert!(
        c.request(Request::NewSession {
            name: Some("work".into()),
            spawn: sh("true")
        })
        .await
        .is_err(),
        "duplicate name"
    );

    // Split right and size the window: both halves get their share.
    let Response::Created { block: right, .. } = c
        .request(Request::Split {
            target: Some(block.to_string()),
            dir: Direction::Right,
            ratio: None,
            spawn: sh("cat"),
            focus: true,
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    c.request(Request::Resize {
        target: session.to_string(),
        cols: 101,
        rows: 30,
    })
    .await
    .unwrap();
    let Response::Inspected {
        item: Inspected::Session(s),
    } = c
        .request(Request::Inspect {
            target: "work".into(),
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    let w = &s.windows[0];
    assert_eq!(w.focused, right);
    let sizes: Vec<_> = w.blocks.iter().map(|b| (b.cols, b.rows)).collect();
    assert_eq!(sizes, [(50, 30), (50, 30)]);

    c.request(Request::Rename {
        target: right.to_string(),
        name: "logs".into(),
    })
    .await
    .unwrap();
    c.request(Request::Input {
        target: Some("logs".into()),
        input: vec![Input::Text("x\r".into())],
    })
    .await
    .unwrap();
    eventually(c, "logs", "x").await;

    // Closing one block collapses the layout; closing the session removes it.
    c.request(Request::Close {
        target: right.to_string(),
    })
    .await
    .unwrap();
    let Response::Sessions { sessions } = c.request(Request::List).await.unwrap() else {
        panic!()
    };
    assert_eq!(sessions[0].windows[0].blocks.len(), 1);
    c.request(Request::Close {
        target: "work".into(),
    })
    .await
    .unwrap();
    let Response::Sessions { sessions } = c.request(Request::List).await.unwrap() else {
        panic!()
    };
    assert!(sessions.is_empty());
    assert!(
        c.request(Request::Capture {
            target: Some("work".into()),
            scrollback: false
        })
        .await
        .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn wait_exit_status_and_frames() {
    let mut h = start(false).await;
    let c = h.client.clone();
    c.request(Request::Watch).await.unwrap();

    // A program that reports status via OSC 7501, then exits 3.
    let script = r"printf '\033]7501;state=working:progress=50\033\\'; sleep 0.3; printf '\033]7501;state=done\007'; sleep 0.3; exit 3";
    let Response::Created { block, .. } = c
        .request(Request::NewSession {
            name: None,
            spawn: Spawn {
                keep_open: true,
                ..sh(script)
            },
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    let r = c
        .request_timeout(
            Request::Wait {
                target: Some(block.to_string()),
                status: vec![ProgramState::Done],
                timeout_ms: Some(5000),
            },
            Duration::from_secs(6),
        )
        .await
        .unwrap();
    assert!(
        matches!(&r, Response::Waited { status: Some(s), .. } if s.state == ProgramState::Done),
        "{r:?}"
    );
    let r = c
        .request(Request::Wait {
            target: Some(block.to_string()),
            status: vec![],
            timeout_ms: Some(5000),
        })
        .await
        .unwrap();
    assert!(matches!(r, Response::Waited { exit: Some(3), .. }), "{r:?}");
    // keep_open: the block stays with its exit code and sticky `done` status.
    let Response::Inspected {
        item: Inspected::Block(b),
    } = c
        .request(Request::Inspect {
            target: block.to_string(),
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(b.exited, Some(3));
    assert_eq!(b.status.map(|s| s.state), Some(ProgramState::Done));

    // Watch saw the status changes.
    let mut seen = Vec::new();
    while let Ok(e) = h.events.try_recv() {
        if let Event::Status {
            status: Some(s), ..
        } = e
        {
            seen.push(s.state);
        }
    }
    assert!(
        seen.contains(&ProgramState::Working) && seen.contains(&ProgramState::Done),
        "{seen:?}"
    );

    // A short-lived block without keep_open is removed, but `wait` still knows its exit code.
    let Response::Created { block: quick, .. } = c
        .request(Request::NewSession {
            name: None,
            spawn: sh("exit 7"),
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    tokio::time::sleep(Duration::from_millis(400)).await;
    let r = c
        .request(Request::Wait {
            target: Some(quick.to_string()),
            status: vec![],
            timeout_ms: Some(2000),
        })
        .await
        .unwrap();
    assert!(matches!(r, Response::Waited { exit: Some(7), .. }), "{r:?}");

    // Frames stream for subscribed blocks.
    let Response::Created { block: live, .. } = c
        .request(Request::NewSession {
            name: Some("live".into()),
            spawn: sh("echo frame-test; cat"),
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    c.request(Request::Subscribe { blocks: vec![live] })
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let e = tokio::time::timeout_at(deadline, h.events.recv())
            .await
            .unwrap()
            .unwrap();
        if let Event::Frame { block, frame } = e
            && block == live
            && frame.line_text(0).contains("frame-test")
        {
            assert_eq!((frame.cols, frame.rows), (120, 36));
            break;
        }
    }

    // Status can also be set directly (agent hooks).
    c.request(Request::SetStatus {
        target: Some("live".into()),
        status: Some(ferrumterm_protocol::ProgramStatus::new(
            ProgramState::Blocked,
        )),
    })
    .await
    .unwrap();
    let Response::Sessions { sessions } = c.request(Request::List).await.unwrap() else {
        panic!()
    };
    let live_session = sessions.iter().find(|s| s.name == "live").unwrap();
    assert_eq!(live_session.summary_status(), Some(ProgramState::Blocked));
}

#[tokio::test(flavor = "multi_thread")]
async fn tcp_requires_token() {
    let h = start(true).await;
    let addr = format!("127.0.0.1:{}", h.port.unwrap());
    let bad = Client::connect(
        &Endpoint::Tcp {
            addr: addr.clone(),
            token: Some("wrong".into()),
        },
        "t",
    )
    .await;
    assert!(bad.is_err(), "bad token rejected");
    let none = Client::connect(
        &Endpoint::Tcp {
            addr: addr.clone(),
            token: None,
        },
        "t",
    )
    .await;
    assert!(none.is_err(), "missing token rejected");
    let token = std::fs::read_to_string(h.state.join("token")).unwrap();
    let (ok, _) = Client::connect(
        &Endpoint::Tcp {
            addr,
            token: Some(token),
        },
        "t",
    )
    .await
    .unwrap();
    let info = ok.info().await.unwrap();
    assert!(
        info.listeners
            .iter()
            .any(|l| l.starts_with("tcp://127.0.0.1"))
    );
    assert_eq!(info.clients, 2);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(h.state.join("token"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
    drop(h.client);
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Port forwards are server-owned child processes. A stand-in command
/// replaces `ssh -N` so no connection is made.
#[tokio::test(flavor = "multi_thread")]
async fn forwards_run_stop_fail_and_refuse_taken_ports() {
    use ferrumterm_protocol::{ForwardSpec, ForwardState};
    let mut h = start(false).await;
    let c = &h.client;
    c.request(Request::Watch).await.unwrap();
    let spec = ForwardSpec::parse(&format!("L {}:localhost:80", free_port())).unwrap();
    let key = ferrumterm_protocol::forward_key("box", &spec);
    let start = |spec: ForwardSpec, script: &str| Request::ForwardStart {
        host: "box".into(),
        spec,
        command: vec!["/bin/sh".into(), "-c".into(), script.into()],
        env: Vec::new(),
    };

    // Stays up → running, with a pid; stopping removes it.
    let Response::Forwards { forwards } = c
        .request(start(spec.clone(), "exec sleep 30"))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(forwards[0].key, key);
    assert!(forwards[0].pid.is_some());
    assert!(
        c.request(start(spec.clone(), "exec sleep 30"))
            .await
            .is_err(),
        "already running"
    );
    let Response::Forwards { forwards } = c.request(Request::ForwardList).await.unwrap() else {
        panic!()
    };
    assert_eq!(forwards.len(), 1);
    assert_eq!(forwards[0].state, ForwardState::Running);
    let mut saw_event = false;
    while let Ok(Some(e)) = tokio::time::timeout(Duration::from_millis(200), h.events.recv()).await
    {
        saw_event |= matches!(e, Event::Forwards);
    }
    assert!(saw_event, "watchers hear about forwards");
    c.request(Request::ForwardStop { key: key.clone() })
        .await
        .unwrap();
    let Response::Forwards { forwards } = c.request(Request::ForwardList).await.unwrap() else {
        panic!()
    };
    assert!(forwards.is_empty());

    // Exits at once → error with ssh's stderr, kept as a failed entry.
    let err = c
        .request(start(
            spec.clone(),
            "echo 'Permission denied (publickey).' >&2; exit 255",
        ))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("code 255") && err.contains("Permission denied"),
        "{err}"
    );
    let Response::Forwards { forwards } = c.request(Request::ForwardList).await.unwrap() else {
        panic!()
    };
    assert_eq!(forwards[0].state, ForwardState::Failed);
    c.request(Request::ForwardStop { key }).await.unwrap();

    // The local port is taken → refused before anything runs.
    let holder = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = holder.local_addr().unwrap().port();
    let taken = ForwardSpec::parse(&format!("L {port}:localhost:80")).unwrap();
    let err = c
        .request(start(taken, "exec sleep 30"))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(&format!("local port {port} is already in use")),
        "{err}"
    );
    drop(holder);
}

#[tokio::test(flavor = "multi_thread")]
async fn welcome_reports_the_build_and_revision() {
    let h = start(false).await;
    let welcome = h.client.server.clone().expect("welcome info kept");
    assert_eq!(welcome.version, ferrumterm_protocol::build_version());
    assert_eq!(welcome.revision, ferrumterm_protocol::SERVER_REVISION);
    assert_eq!(
        h.client.server_age(),
        ferrumterm_protocol::ServerAge::Current
    );
    let info = h.client.info().await.unwrap();
    assert_eq!(info.revision, ferrumterm_protocol::SERVER_REVISION);
    // A client from before the version fields still gets a welcome.
    let old = h
        .client
        .request(Request::Hello {
            client: "old".into(),
            token: None,
            protocol: ferrumterm_protocol::PROTOCOL_VERSION,
            version: None,
            revision: None,
        })
        .await
        .unwrap();
    assert!(matches!(old, Response::Welcome { .. }), "{old:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn find_searches_the_scrollback() {
    let h = start(false).await;
    let c = &h.client;
    let Response::Created { block, .. } = c
        .request(Request::NewSession {
            name: None,
            spawn: sh(
                "i=1; while [ $i -le 200 ]; do echo row-$i; i=$((i+1)); done; echo needle-x; cat",
            ),
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    let target = block.to_string();
    eventually(c, &target, "needle-x").await;
    let find = |query: &str, regex: bool| Request::Find {
        target: Some(target.clone()),
        query: query.into(),
        case_sensitive: false,
        regex,
    };
    // Far back in the scrollback: rows 1, 10..19 and 100..199.
    let Response::Found { matches, truncated } = c.request(find("ROW-1", false)).await.unwrap()
    else {
        panic!()
    };
    assert!(!truncated);
    assert_eq!(matches.len(), 111);
    let first = matches[0];
    assert_eq!((first.line, first.col, first.end_col), (0, 0, 5));
    let Response::Found { matches, .. } = c.request(find(r"needle-\w", true)).await.unwrap() else {
        panic!()
    };
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].line, 200);
    let err = c.request(find("(", true)).await.unwrap_err().to_string();
    assert!(err.contains("unclosed"), "{err}");
}
