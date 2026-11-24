//! One client connection: authentication, request dispatch, frame
//! subscriptions, event watching and asynchronous waits.

use crate::block::lock;
use crate::{Server, ops};
use ferrumterm_protocol::{
    BlockId, ClientMsg, Event, PROTOCOL_VERSION, ProgramState, Request, Response, ServerMsg,
    read_msg, write_msg,
};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

/// Coalescing delay between frames of one block (~120 fps cap).
const FRAME_COALESCE: Duration = Duration::from_millis(8);

fn token_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

pub async fn serve<R, W>(server: Arc<Server>, mut r: R, mut w: W, trusted: bool)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (tx, mut rx) = mpsc::channel::<ServerMsg>(512);
    let writer = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            if write_msg(&mut w, &m).await.is_err() {
                break;
            }
        }
    });
    server.clients.fetch_add(1, Ordering::Relaxed);
    let mut authed = trusted;
    let mut window = false;
    let mut subs: Vec<JoinHandle<()>> = Vec::new();
    let mut tasks: Vec<JoinHandle<()>> = Vec::new();

    loop {
        let msg: ClientMsg = match read_msg(&mut r).await {
            Ok(m) => m,
            Err(_) => break,
        };
        let id = msg.id;
        let reply = |res: Response| ServerMsg::Reply { id, res };
        if !authed {
            match &msg.req {
                Request::Hello { token: Some(t), .. } if token_eq(t, &server.token) => {
                    authed = true
                }
                _ => {
                    let _ = tx
                        .send(reply(Response::Error {
                            message: "authentication required: wrong or missing token".into(),
                        }))
                        .await;
                    tracing::warn!("rejected remote client (bad token)");
                    break;
                }
            }
        }
        let res = match msg.req {
            Request::Hello { protocol, .. } if protocol != PROTOCOL_VERSION => Response::Error {
                message: format!(
                    "protocol mismatch: server {PROTOCOL_VERSION}, client {protocol}; restart the server (`fterm server stop`)"
                ),
            },
            Request::Hello {
                client,
                version,
                revision,
                ..
            } => {
                if revision.is_some_and(|r| r > ferrumterm_protocol::SERVER_REVISION) {
                    tracing::warn!(
                        client,
                        client_version = version.as_deref().unwrap_or("?"),
                        server_version = ferrumterm_protocol::build_version(),
                        "a newer client connected; restart the server to pick up its features"
                    );
                }
                if client == ferrumterm_protocol::WINDOW_CLIENT && !window {
                    window = true;
                    server.windows.fetch_add(1, Ordering::Relaxed);
                }
                Response::Welcome {
                    info: server.info(),
                }
            }
            Request::ServerInfo => Response::Info {
                info: server.info(),
            },
            Request::List => Response::Sessions {
                sessions: lock(&server.state).list(),
            },
            Request::NewSession { name, spawn } => ops::new_session(&server, name, &spawn),
            Request::NewWindow {
                session,
                name,
                spawn,
            } => ops::new_window(&server, session.as_deref(), name, &spawn),
            Request::Split {
                target,
                dir,
                ratio,
                spawn,
                focus,
            } => ops::split(&server, target.as_deref(), dir, ratio, &spawn, focus),
            Request::Input { target, input } => {
                with_block(&server, target.as_deref(), |b| match b.input(&input) {
                    Ok(()) => Response::Ok,
                    Err(message) => Response::Error { message },
                })
            }
            Request::Capture {
                target,
                scrollback,
                unwrapped,
            } => with_block(&server, target.as_deref(), |b| {
                let term = lock(&b.term);
                Response::Text {
                    text: if unwrapped {
                        term.capture_unwrapped(scrollback)
                    } else {
                        term.capture(scrollback)
                    },
                }
            }),
            Request::Context {
                session,
                add,
                clear,
            } => ops::context(&server, session.as_deref(), add, clear),
            Request::Commands { target } => {
                with_block(&server, target.as_deref(), |b| Response::Commands {
                    commands: lock(&b.term).commands(),
                })
            }
            Request::CommandOutput { target, command } => {
                with_block(&server, target.as_deref(), |b| {
                    match lock(&b.term).command_output(command) {
                        Some(text) => Response::Text { text },
                        None => Response::Error {
                            message: format!("command {command} is no longer in the scrollback"),
                        },
                    }
                })
            }
            Request::Find {
                target,
                query,
                case_sensitive,
                regex,
            } => with_block(&server, target.as_deref(), |b| {
                match lock(&b.term).find(&query, case_sensitive, regex) {
                    Ok((matches, truncated)) => Response::Found { matches, truncated },
                    Err(message) => Response::Error { message },
                }
            }),
            Request::Scroll { target, lines } => with_block(&server, target.as_deref(), |b| {
                b.scroll(lines);
                Response::Ok
            }),
            Request::Inspect { target } => {
                let st = lock(&server.state);
                match st.resolve(&target).and_then(|t| st.inspect(t)) {
                    Some(item) => Response::Inspected { item },
                    None => Response::Error {
                        message: format!("nothing matches “{target}”"),
                    },
                }
            }
            Request::Resize { target, cols, rows } => ops::resize(&server, &target, cols, rows),
            Request::Focus { target } => ops::focus(&server, &target),
            Request::Close { target } => ops::close(&server, &target),
            Request::Rename { target, name } => ops::rename(&server, &target, &name),
            Request::SetStatus { target, status } => {
                let block = lock(&server.state).resolve_block(target.as_deref());
                match block {
                    Ok(b) => {
                        server.set_status(b, status, crate::StatusSource::Program);
                        Response::Ok
                    }
                    Err(message) => Response::Error { message },
                }
            }
            Request::Subscribe { blocks } => {
                for s in subs.drain(..) {
                    s.abort();
                }
                for b in blocks {
                    if let Some(h) = subscribe(&server, b, tx.clone()) {
                        subs.push(h);
                    }
                }
                Response::Ok
            }
            Request::Watch => {
                let mut events = server.events.subscribe();
                let tx = tx.clone();
                tasks.push(tokio::spawn(async move {
                    loop {
                        match events.recv().await {
                            Ok(e) => {
                                if tx.send(ServerMsg::Event(e)).await.is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => {
                                let _ = tx.send(ServerMsg::Event(Event::Changed)).await;
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }));
                Response::Ok
            }
            Request::Wait {
                target,
                status,
                timeout_ms,
            } => {
                // Replies later with the same id; the connection keeps serving.
                let s = server.clone();
                let tx = tx.clone();
                tasks.push(tokio::spawn(async move {
                    let res = wait(&s, target.as_deref(), &status, timeout_ms).await;
                    let _ = tx.send(ServerMsg::Reply { id, res }).await;
                }));
                continue;
            }
            Request::SetAwake { mode } => {
                lock(&server.awake).mode = mode;
                server.update_awake();
                Response::Ok
            }
            Request::ForwardStart {
                host,
                spec,
                command,
                env,
            } => {
                // Takes a moment (ssh must stay up); reply later with the same id.
                let s = server.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let res = crate::forward::start(s, host, spec, command, env).await;
                    let _ = tx.send(ServerMsg::Reply { id, res }).await;
                });
                continue;
            }
            Request::ForwardStop { key } => crate::forward::stop(&server, &key),
            Request::ForwardList => Response::Forwards {
                forwards: lock(&server.forwards).list(),
            },
            Request::Shutdown => {
                let _ = tx.send(reply(Response::Ok)).await;
                server.shutdown.notify_one();
                break;
            }
        };
        if tx.send(reply(res)).await.is_err() {
            break;
        }
    }
    for t in subs.into_iter().chain(tasks) {
        t.abort();
    }
    drop(tx);
    let _ = writer.await;
    server.clients.fetch_sub(1, Ordering::Relaxed);
    if window {
        server.windows.fetch_sub(1, Ordering::Relaxed);
    }
}

fn with_block(
    server: &Server,
    target: Option<&str>,
    f: impl FnOnce(&crate::block::Block) -> Response,
) -> Response {
    let block = {
        let st = lock(&server.state);
        st.resolve_block(target).and_then(|id| {
            st.blocks
                .get(&id)
                .cloned()
                .ok_or_else(|| format!("{id} is gone"))
        })
    };
    match block {
        Ok(b) => f(&b),
        Err(message) => Response::Error { message },
    }
}

/// Streams frames of `block` until it disappears or the subscription is replaced.
fn subscribe(
    server: &Server,
    block: BlockId,
    tx: mpsc::Sender<ServerMsg>,
) -> Option<JoinHandle<()>> {
    let b = lock(&server.state).blocks.get(&block).cloned()?;
    let mut rx = b.generation.subscribe();
    let weak = Arc::downgrade(&b);
    drop(b);
    Some(tokio::spawn(async move {
        loop {
            let Some(b) = weak.upgrade() else { break };
            let frame = lock(&b.term).frame();
            drop(b);
            if tx
                .send(ServerMsg::Event(Event::Frame { block, frame }))
                .await
                .is_err()
            {
                break;
            }
            if rx.changed().await.is_err() {
                break;
            }
            tokio::time::sleep(FRAME_COALESCE).await;
            rx.borrow_and_update();
        }
    }))
}

async fn wait(
    server: &Server,
    target: Option<&str>,
    states: &[ProgramState],
    timeout_ms: Option<u64>,
) -> Response {
    let mut events = server.events.subscribe();
    let id = {
        let st = lock(&server.state);
        match target.and_then(|t| t.parse::<BlockId>().ok()) {
            Some(b) => b,
            None => match st.resolve_block(target) {
                Ok(b) => b,
                Err(message) => return Response::Error { message },
            },
        }
    };
    let check = || -> Option<Response> {
        let st = lock(&server.state);
        if let Some(b) = st.blocks.get(&id) {
            if let Some(s) = lock(&b.status)
                .clone()
                .filter(|s| states.contains(&s.state))
            {
                return Some(Response::Waited {
                    exit: None,
                    status: Some(s),
                });
            }
            if let Some(code) = *lock(&b.exited) {
                return Some(Response::Waited {
                    exit: Some(code),
                    status: lock(&b.status).clone(),
                });
            }
            return None;
        }
        drop(st);
        let code = lock(&server.exits)
            .iter()
            .rev()
            .find(|(b, _)| *b == id)
            .map(|(_, c)| *c);
        Some(match code {
            Some(c) => Response::Waited {
                exit: Some(c),
                status: None,
            },
            None => Response::Error {
                message: format!("{id} does not exist"),
            },
        })
    };
    let body = async {
        loop {
            if let Some(r) = check() {
                return r;
            }
            match events.recv().await {
                Ok(Event::Exited { block, code }) if block == id => {
                    return Response::Waited {
                        exit: Some(code),
                        status: None,
                    };
                }
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => {
                    return Response::Error {
                        message: "server stopped".into(),
                    };
                }
            }
        }
    };
    match timeout_ms {
        Some(ms) => tokio::time::timeout(Duration::from_millis(ms), body)
            .await
            .unwrap_or_else(|_| Response::Error {
                message: format!("timed out after {ms} ms"),
            }),
        None => body.await,
    }
}
