//! One block = one PTY + child process + emulator. Output is read on a
//! dedicated thread that feeds the emulator, answers terminal queries, bumps
//! the frame generation and reports status / title / exit to the server.

use ferrumterm_emulator::Terminal;
use ferrumterm_protocol::status::StatusUpdate;
use ferrumterm_protocol::{BlockId, Input, ProgramStatus, Spawn};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tokio::sync::{mpsc, watch};

/// Messages from block threads to the server loop.
#[derive(Debug)]
pub enum Internal {
    Status(BlockId, StatusUpdate),
    Title(BlockId),
    Bell(BlockId),
    Exited(BlockId, i32),
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Block {
    pub id: BlockId,
    pub label: Mutex<Option<String>>,
    pub command: Vec<String>,
    pub cwd: Option<String>,
    pub pid: Option<u32>,
    pub keep_open: bool,
    pub term: Mutex<Terminal>,
    pub status: Mutex<Option<ProgramStatus>>,
    pub exited: Mutex<Option<i32>>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    /// Bumped whenever the screen may have changed.
    pub generation: watch::Sender<u64>,
}

/// Where FerrumTerm-specific environment comes from.
pub struct SpawnContext<'a> {
    pub session: &'a str,
    pub window: &'a str,
    pub socket: &'a str,
}

impl Block {
    pub fn spawn(
        id: BlockId,
        spec: &Spawn,
        cols: u16,
        rows: u16,
        ctx: &SpawnContext<'_>,
        internal: mpsc::UnboundedSender<Internal>,
    ) -> Result<Arc<Self>, String> {
        let size = PtySize {
            rows: rows.max(1),
            cols: cols.max(2),
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = native_pty_system()
            .openpty(size)
            .map_err(|e| format!("could not open a pty: {e}"))?;
        let mut cmd = match spec.command.split_first() {
            None => CommandBuilder::new_default_prog(),
            Some((prog, args)) => {
                let mut c = CommandBuilder::new(prog);
                c.args(args);
                c
            }
        };
        let cwd = spec.cwd.clone().or_else(|| std::env::var("HOME").ok());
        if let Some(dir) = &cwd {
            cmd.cwd(dir);
        }
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "FerrumTerm");
        cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
        cmd.env("FTERM_SESSION", ctx.session);
        cmd.env("FTERM_WINDOW", ctx.window);
        cmd.env("FTERM_BLOCK", id.to_string());
        cmd.env("FTERM_SOCKET", ctx.socket);
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("could not start {:?}: {e}", spec.command))?;
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let killer = child.clone_killer();
        let pid = child.process_id();
        let (generation, _) = watch::channel(0u64);
        let block = Arc::new(Self {
            id,
            label: Mutex::new(spec.label.clone()),
            command: spec.command.clone(),
            cwd,
            pid,
            keep_open: spec.keep_open,
            term: Mutex::new(Terminal::new(cols, rows)),
            status: Mutex::new(None),
            exited: Mutex::new(None),
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            killer: Mutex::new(killer),
            generation,
        });

        let b = block.clone();
        let tx = internal.clone();
        std::thread::Builder::new()
            .name(format!("pty-{id}"))
            .spawn(move || b.read_loop(reader, &tx))
            .map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name(format!("wait-{id}"))
            .spawn(move || {
                let code = child.wait().map(|s| s.exit_code() as i32).unwrap_or(-1);
                // Let the reader drain the last output before reporting.
                std::thread::sleep(Duration::from_millis(60));
                let _ = internal.send(Internal::Exited(id, code));
            })
            .map_err(|e| e.to_string())?;
        Ok(block)
    }

    fn read_loop(&self, mut reader: Box<dyn Read + Send>, tx: &mpsc::UnboundedSender<Internal>) {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let feed = lock(&self.term).feed(&buf[..n]);
            if !feed.reply.is_empty() {
                let _ = lock(&self.writer).write_all(&feed.reply);
            }
            self.generation.send_modify(|g| *g = g.wrapping_add(1));
            for s in feed.status {
                let _ = tx.send(Internal::Status(self.id, s));
            }
            if feed.title_changed {
                let _ = tx.send(Internal::Title(self.id));
            }
            if feed.bell {
                let _ = tx.send(Internal::Bell(self.id));
            }
        }
    }

    /// Encodes and writes input. Typing scrolls the view back to the bottom.
    pub fn input(&self, inputs: &[Input]) -> Result<(), String> {
        let mut bytes = Vec::new();
        {
            let mut term = lock(&self.term);
            for i in inputs {
                match term.encode(i) {
                    Some(b) => bytes.extend(b),
                    None => return Err(format!("unknown key: {i:?}")),
                }
            }
            if term.scrolled() {
                term.scroll(None);
                self.generation.send_modify(|g| *g = g.wrapping_add(1));
            }
        }
        lock(&self.writer)
            .write_all(&bytes)
            .map_err(|e| format!("write failed: {e}"))
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let (cols, rows) = (cols.max(2), rows.max(1));
        {
            let mut term = lock(&self.term);
            if term.cols() == cols && term.rows() == rows {
                return;
            }
            term.resize(cols, rows);
        }
        let _ = lock(&self.master).resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        self.generation.send_modify(|g| *g = g.wrapping_add(1));
    }

    pub fn scroll(&self, lines: Option<i32>) {
        lock(&self.term).scroll(lines);
        self.generation.send_modify(|g| *g = g.wrapping_add(1));
    }

    pub fn kill(&self) {
        let _ = lock(&self.killer).kill();
    }

    #[must_use]
    pub fn title(&self) -> String {
        lock(&self.term).title().to_owned()
    }
}
