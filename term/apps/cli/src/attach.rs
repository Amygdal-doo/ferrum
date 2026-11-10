//! `fterm attach`: tmux-style client in the current terminal. Renders the
//! session's active window (all its splits) from server frames and forwards
//! keys. Prefix `C-b`, then: d detach · c new window · % split right ·
//! " split down · o next block · n/p next/prev window · 0-9 window ·
//! x close block · [ scroll up · ] back to bottom · C-b send C-b.

use crate::util::{Ctx, passthrough_env};
use crossterm::event::{self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{
    Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::{cursor, execute, queue, terminal};
use ferrumterm_client::Client;
use ferrumterm_protocol::{
    BlockId, Direction, Event, Frame, Input, Rect, Request, Response, SessionInfo, Spawn,
    WindowInfo, flags,
};
use std::collections::HashMap;
use std::io::{Write, stdout};
use std::process::ExitCode;
use std::time::Duration;
use tokio::sync::mpsc;

struct View {
    session: SessionInfo,
    frames: HashMap<BlockId, Frame>,
    subscribed: Vec<BlockId>,
    prefix: bool,
    message: Option<String>,
    size: (u16, u16),
}

impl View {
    fn window(&self) -> Option<&WindowInfo> {
        self.session
            .windows
            .iter()
            .find(|w| w.id == self.session.active_window)
            .or(self.session.windows.first())
    }

    fn content(&self) -> Rect {
        Rect {
            x: 0,
            y: 0,
            cols: self.size.0,
            rows: self.size.1.saturating_sub(1).max(1),
        }
    }
}

fn matches_session(s: &SessionInfo, t: &str) -> bool {
    s.id.to_string() == t
        || s.name == t
        || s.windows.iter().any(|w| {
            w.id.to_string() == t
                || w.name == t
                || w.blocks
                    .iter()
                    .any(|b| b.id.to_string() == t || b.label.as_deref() == Some(t))
        })
}

async fn list(c: &Client) -> Result<Vec<SessionInfo>, String> {
    match c.request(Request::List).await.map_err(|e| e.to_string())? {
        Response::Sessions { sessions } => Ok(sessions),
        _ => Err("unexpected reply".into()),
    }
}

pub async fn run(ctx: &Ctx, target: Option<String>) -> Result<ExitCode, String> {
    if ctx.is_local() && std::env::var("FTERM_BLOCK").is_ok() && target.is_none() {
        return Err("already inside FerrumTerm; name a session to attach a nested view (fterm attach <session>)".into());
    }
    let (c, mut events) = ctx.connect().await?;
    let mut sessions = list(&c).await?;
    if sessions.is_empty() {
        let spawn = Spawn {
            cwd: std::env::current_dir()
                .ok()
                .map(|d| d.display().to_string()),
            env: passthrough_env(),
            ..Spawn::default()
        };
        c.request(Request::NewSession { name: None, spawn })
            .await
            .map_err(|e| e.to_string())?;
        sessions = list(&c).await?;
    }
    let session = match &target {
        Some(t) => sessions
            .into_iter()
            .find(|s| matches_session(s, t))
            .ok_or_else(|| format!("no session matches {t}"))?,
        None => sessions.into_iter().next().ok_or("no sessions")?,
    };
    c.request(Request::Watch).await.map_err(|e| e.to_string())?;

    let (key_tx, mut keys) = mpsc::unbounded_channel();
    std::thread::spawn(move || {
        while let Ok(ev) = event::read() {
            if key_tx.send(ev).is_err() {
                break;
            }
        }
    });

    terminal::enable_raw_mode().map_err(|e| e.to_string())?;
    let mut out = stdout();
    let _ = execute!(
        out,
        terminal::EnterAlternateScreen,
        event::EnableBracketedPaste,
        cursor::Hide
    );
    let size = terminal::size().unwrap_or((80, 24));
    let mut view = View {
        session,
        frames: HashMap::new(),
        subscribed: Vec::new(),
        prefix: false,
        message: None,
        size,
    };
    let result = event_loop(&c, &mut events, &mut keys, &mut view).await;
    let _ = execute!(
        out,
        ResetColor,
        cursor::Show,
        event::DisableBracketedPaste,
        terminal::LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
    match result {
        Ok(Some(reason)) => {
            println!("[{reason}]");
            Ok(ExitCode::SUCCESS)
        }
        Ok(None) => Ok(ExitCode::SUCCESS),
        Err(e) => Err(e),
    }
}

/// `Ok(Some(reason))` when the session ends or the user detaches.
async fn event_loop(
    c: &Client,
    events: &mut mpsc::UnboundedReceiver<Event>,
    keys: &mut mpsc::UnboundedReceiver<TermEvent>,
    view: &mut View,
) -> Result<Option<String>, String> {
    sync(c, view).await?;
    let mut dirty = true;
    let mut tick = tokio::time::interval(Duration::from_millis(16));
    loop {
        tokio::select! {
            ev = events.recv() => match ev {
                None | Some(Event::Bye) => return Ok(Some("server stopped".into())),
                Some(Event::Frame { block, frame }) => {
                    view.frames.insert(block, frame);
                    dirty = true;
                }
                Some(Event::Changed | Event::Exited { .. } | Event::Status { .. }) => {
                    if !refresh(c, view).await? {
                        return Ok(Some(format!("session {} ended", view.session.name)));
                    }
                    dirty = true;
                }
                Some(Event::Bell { .. } | Event::Forwards) => {}
            },
            k = keys.recv() => {
                let Some(k) = k else { return Ok(None) };
                if let Some(reason) = handle_key(c, view, k).await? {
                    return Ok(Some(reason));
                }
                dirty = true;
            }
            _ = tick.tick() => {
                if dirty {
                    draw(view).map_err(|e| e.to_string())?;
                    dirty = false;
                }
            }
        }
    }
}

/// Re-reads the session; `false` when it no longer exists.
async fn refresh(c: &Client, view: &mut View) -> Result<bool, String> {
    let id = view.session.id;
    match list(c).await?.into_iter().find(|s| s.id == id) {
        Some(s) => {
            view.session = s;
            sync(c, view).await?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Sizes the active window to this terminal and subscribes to its blocks.
async fn sync(c: &Client, view: &mut View) -> Result<(), String> {
    let Some(w) = view.window().cloned() else {
        return Ok(());
    };
    let area = view.content();
    if (w.cols, w.rows) != (area.cols, area.rows) {
        c.request(Request::Resize {
            target: w.id.to_string(),
            cols: area.cols,
            rows: area.rows,
        })
        .await
        .map_err(|e| e.to_string())?;
    }
    let blocks = w.layout.blocks();
    if blocks != view.subscribed {
        view.frames.retain(|b, _| blocks.contains(b));
        c.request(Request::Subscribe {
            blocks: blocks.clone(),
        })
        .await
        .map_err(|e| e.to_string())?;
        view.subscribed = blocks;
    }
    Ok(())
}

fn key_input(k: &KeyEvent) -> Option<Input> {
    let m = k.modifiers;
    let mut prefix = String::new();
    if m.contains(KeyModifiers::CONTROL) {
        prefix.push_str("C-");
    }
    if m.contains(KeyModifiers::ALT) {
        prefix.push_str("M-");
    }
    let named = |name: &str, shift_ok: bool| {
        let s = if shift_ok && m.contains(KeyModifiers::SHIFT) {
            "S-"
        } else {
            ""
        };
        Some(Input::Key(format!("{prefix}{s}{name}")))
    };
    match k.code {
        KeyCode::Char(ch) if prefix.is_empty() => Some(Input::Text(ch.to_string())),
        KeyCode::Char(ch) => Some(Input::Key(format!("{prefix}{ch}"))),
        KeyCode::Enter => named("Enter", false),
        KeyCode::Tab => named("Tab", true),
        KeyCode::BackTab => Some(Input::Key("S-Tab".into())),
        KeyCode::Backspace => named("BSpace", false),
        KeyCode::Esc => named("Escape", false),
        KeyCode::Left => named("Left", true),
        KeyCode::Right => named("Right", true),
        KeyCode::Up => named("Up", true),
        KeyCode::Down => named("Down", true),
        KeyCode::Home => named("Home", true),
        KeyCode::End => named("End", true),
        KeyCode::PageUp => named("PageUp", true),
        KeyCode::PageDown => named("PageDown", true),
        KeyCode::Insert => named("Insert", false),
        KeyCode::Delete => named("Delete", true),
        KeyCode::F(n) => named(&format!("F{n}"), true),
        _ => None,
    }
}

async fn handle_key(c: &Client, view: &mut View, ev: TermEvent) -> Result<Option<String>, String> {
    let req = |r| c.request(r);
    let focused = view.window().map(|w| w.focused);
    match ev {
        TermEvent::Resize(cols, rows) => {
            view.size = (cols, rows);
            sync(c, view).await?;
        }
        TermEvent::Paste(text) => {
            if let Some(b) = focused {
                let _ = req(Request::Input {
                    target: Some(b.to_string()),
                    input: vec![Input::Paste(text)],
                })
                .await;
            }
        }
        TermEvent::Key(k) if k.kind != KeyEventKind::Release => {
            let is_prefix =
                k.code == KeyCode::Char('b') && k.modifiers.contains(KeyModifiers::CONTROL);
            if view.prefix {
                view.prefix = false;
                view.message = None;
                return prefix_command(c, view, k, focused).await;
            }
            if is_prefix {
                view.prefix = true;
                return Ok(None);
            }
            let Some(b) = focused else { return Ok(None) };
            if k.modifiers.contains(KeyModifiers::SHIFT)
                && matches!(k.code, KeyCode::PageUp | KeyCode::PageDown)
            {
                let rows = i32::from(view.content().rows / 2);
                let lines = if k.code == KeyCode::PageUp {
                    rows
                } else {
                    -rows
                };
                let _ = req(Request::Scroll {
                    target: Some(b.to_string()),
                    lines: Some(lines),
                })
                .await;
            } else if let Some(input) = key_input(&k)
                && let Err(e) = req(Request::Input {
                    target: Some(b.to_string()),
                    input: vec![input],
                })
                .await
            {
                view.message = Some(e.to_string());
            }
        }
        _ => {}
    }
    Ok(None)
}

async fn prefix_command(
    c: &Client,
    view: &mut View,
    k: KeyEvent,
    focused: Option<BlockId>,
) -> Result<Option<String>, String> {
    let sid = view.session.id.to_string();
    let target = focused.map(|b| b.to_string());
    let spawn = || Spawn {
        env: passthrough_env(),
        ..Spawn::default()
    };
    let windows: Vec<_> = view.session.windows.iter().map(|w| w.id).collect();
    let current = windows
        .iter()
        .position(|w| *w == view.session.active_window)
        .unwrap_or(0);
    let result = match k.code {
        KeyCode::Char('d') => return Ok(Some(format!("detached from {}", view.session.name))),
        KeyCode::Char('b') if k.modifiers.contains(KeyModifiers::CONTROL) => {
            c.request(Request::Input {
                target,
                input: vec![Input::Key("C-b".into())],
            })
            .await
        }
        KeyCode::Char('c') => {
            c.request(Request::NewWindow {
                session: Some(sid),
                name: None,
                spawn: spawn(),
            })
            .await
        }
        KeyCode::Char('%') => {
            c.request(Request::Split {
                target,
                dir: Direction::Right,
                ratio: None,
                spawn: spawn(),
                focus: true,
            })
            .await
        }
        KeyCode::Char('"') => {
            c.request(Request::Split {
                target,
                dir: Direction::Down,
                ratio: None,
                spawn: spawn(),
                focus: true,
            })
            .await
        }
        KeyCode::Char('x') => match target {
            Some(t) => c.request(Request::Close { target: t }).await,
            None => return Ok(None),
        },
        KeyCode::Char('o') | KeyCode::Right | KeyCode::Down | KeyCode::Left | KeyCode::Up => {
            let Some(w) = view.window() else {
                return Ok(None);
            };
            let blocks = w.layout.blocks();
            let i = blocks.iter().position(|b| Some(*b) == focused).unwrap_or(0);
            let back = matches!(k.code, KeyCode::Left | KeyCode::Up);
            let next = if back {
                (i + blocks.len() - 1) % blocks.len()
            } else {
                (i + 1) % blocks.len()
            };
            c.request(Request::Focus {
                target: blocks[next].to_string(),
            })
            .await
        }
        KeyCode::Char('n') | KeyCode::Char('p') => {
            let n = windows.len().max(1);
            let next = if k.code == KeyCode::Char('n') {
                (current + 1) % n
            } else {
                (current + n - 1) % n
            };
            c.request(Request::Focus {
                target: windows[next].to_string(),
            })
            .await
        }
        KeyCode::Char(d @ '0'..='9') => {
            let idx = (d as usize - '0' as usize).saturating_sub(1);
            match windows.get(if d == '0' { 9 } else { idx }) {
                Some(w) => {
                    c.request(Request::Focus {
                        target: w.to_string(),
                    })
                    .await
                }
                None => return Ok(None),
            }
        }
        KeyCode::Char('[') | KeyCode::PageUp => {
            let rows = i32::from(view.content().rows / 2);
            c.request(Request::Scroll {
                target,
                lines: Some(rows),
            })
            .await
        }
        KeyCode::Char(']') => {
            c.request(Request::Scroll {
                target,
                lines: None,
            })
            .await
        }
        _ => {
            view.message = Some("C-b: d detach · c window · % │ split · \" ─ split · o next · n/p window · x close · [ ] scroll".into());
            return Ok(None);
        }
    };
    if let Err(e) = result {
        view.message = Some(e.to_string());
    }
    refresh(c, view)
        .await
        .map(|alive| (!alive).then(|| "session ended".into()))
}

fn color(c: Option<u32>) -> Color {
    c.map_or(Color::Reset, |v| Color::Rgb {
        r: (v >> 16) as u8,
        g: (v >> 8) as u8,
        b: v as u8,
    })
}

fn draw(view: &View) -> std::io::Result<()> {
    let mut out = stdout();
    queue!(
        out,
        terminal::BeginSynchronizedUpdate,
        cursor::Hide,
        ResetColor,
        SetAttribute(Attribute::Reset)
    )?;
    let area = view.content();
    let Some(w) = view.window() else {
        return Ok(());
    };
    let rects = w.layout.rects(area);
    let mut cursor_at = None;
    for (b, r) in &rects {
        let frame = view.frames.get(b);
        for row in 0..r.rows {
            queue!(out, cursor::MoveTo(r.x, r.y + row))?;
            let mut used = 0u16;
            if let Some(line) = frame.and_then(|f| f.lines.get(usize::from(row))) {
                for s in line {
                    if used >= r.cols {
                        break;
                    }
                    let text: String = s.t.chars().take(usize::from(r.cols - used)).collect();
                    used += text.chars().count() as u16;
                    queue!(
                        out,
                        SetForegroundColor(color(s.fg)),
                        SetBackgroundColor(color(s.bg))
                    )?;
                    for (bit, attr) in [
                        (flags::BOLD, Attribute::Bold),
                        (flags::ITALIC, Attribute::Italic),
                        (flags::UNDERLINE, Attribute::Underlined),
                        (flags::DIM, Attribute::Dim),
                        (flags::STRIKE, Attribute::CrossedOut),
                    ] {
                        if s.f & bit != 0 {
                            queue!(out, SetAttribute(attr))?;
                        }
                    }
                    queue!(out, Print(text), SetAttribute(Attribute::Reset))?;
                }
            }
            queue!(
                out,
                ResetColor,
                Print(" ".repeat(usize::from(r.cols.saturating_sub(used))))
            )?;
        }
        if Some(*b) == Some(w.focused)
            && let Some(cur) = frame.and_then(|f| f.cursor)
        {
            cursor_at = Some((r.x + cur.col, r.y + cur.row));
        }
        // Dividers to the right of / below this block.
        let divider = if Some(*b) == Some(w.focused) {
            Color::DarkCyan
        } else {
            Color::DarkGrey
        };
        if r.x + r.cols < area.cols {
            for y in r.y..r.y + r.rows {
                queue!(
                    out,
                    cursor::MoveTo(r.x + r.cols, y),
                    SetForegroundColor(divider),
                    Print("│")
                )?;
            }
        }
        if r.y + r.rows < area.rows {
            queue!(
                out,
                cursor::MoveTo(r.x, r.y + r.rows),
                SetForegroundColor(divider),
                Print("─".repeat(usize::from(r.cols)))
            )?;
        }
    }
    status_line(&mut out, view, w)?;
    if let Some((x, y)) = cursor_at {
        queue!(out, cursor::MoveTo(x, y), cursor::Show)?;
    }
    queue!(out, terminal::EndSynchronizedUpdate)?;
    out.flush()
}

fn status_line(out: &mut impl Write, view: &View, w: &WindowInfo) -> std::io::Result<()> {
    let (cols, rows) = view.size;
    let tabs: String = view
        .session
        .windows
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let badge = x
                .blocks
                .iter()
                .filter_map(|b| b.status.as_ref())
                .map(|s| s.state.glyph())
                .next()
                .unwrap_or("");
            format!(
                " {}:{}{}{} ",
                i + 1,
                x.name,
                if x.id == w.id { "*" } else { "" },
                badge
            )
        })
        .collect();
    let focused = w.blocks.iter().find(|b| b.id == w.focused);
    let right = match (&view.message, view.prefix) {
        (Some(m), _) => m.clone(),
        (None, true) => "C-b …".into(),
        (None, false) => focused
            .map(|b| {
                let st = b
                    .status
                    .as_ref()
                    .map(|s| {
                        format!(
                            " {} {}{}",
                            s.state.glyph(),
                            s.state.as_str(),
                            s.message
                                .as_deref()
                                .map(|m| format!(": {m}"))
                                .unwrap_or_default()
                        )
                    })
                    .unwrap_or_default();
                format!("{}{st}", b.id)
            })
            .unwrap_or_default(),
    };
    let left = format!(" [{}]{tabs}", view.session.name);
    let width = usize::from(cols);
    let mut line: String = left.chars().take(width).collect();
    let pad = width.saturating_sub(line.chars().count() + right.chars().count() + 1);
    line.push_str(&" ".repeat(pad));
    line.push_str(
        &right
            .chars()
            .take(width.saturating_sub(line.chars().count() + 1))
            .collect::<String>(),
    );
    line.push(' ');
    queue!(
        out,
        cursor::MoveTo(0, rows.saturating_sub(1)),
        SetBackgroundColor(Color::Rgb {
            r: 0x2a,
            g: 0x2d,
            b: 0x33
        }),
        SetForegroundColor(Color::Rgb {
            r: 0xd8,
            g: 0xd8,
            b: 0xd8
        }),
        Print(line.chars().take(width).collect::<String>()),
        ResetColor
    )
}
