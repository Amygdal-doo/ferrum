//! One terminal session: a local PTY running the user's shell, read on a
//! background thread into FerrumTerm's emulator (`ferrumterm-emulator`,
//! VT parsing on `alacritty_terminal`). The view is woken through a
//! channel whenever output arrives; the session keeps running while the
//! panel is hidden and is killed when dropped.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ferrumterm_emulator::Terminal;
use ferrumterm_protocol::{Frame, Input};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Session {
    term: Arc<Mutex<Terminal>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Option<Mutex<Box<dyn MasterPty + Send>>>,
    killer: Option<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
    exited: Arc<AtomicBool>,
    /// Shown as the tab title until the program sets one.
    pub shell: PathBuf,
}

impl Session {
    /// Starts `shell` (or `$SHELL` as a login shell when `None`) in `cwd`.
    pub fn spawn(
        shell: Option<&str>,
        cwd: Option<&Path>,
        cols: u16,
        rows: u16,
    ) -> Result<(Self, UnboundedReceiver<()>), String> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: rows.max(1),
                cols: cols.max(2),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("could not open a pty: {e}"))?;
        let configured = shell.map(str::trim).filter(|s| !s.is_empty());
        let mut cmd = match configured {
            // portable-pty runs the default program as a login shell.
            None => CommandBuilder::new_default_prog(),
            Some(program) => CommandBuilder::new(program),
        };
        let home = std::env::var_os("HOME").map(PathBuf::from);
        if let Some(dir) = cwd.map(Path::to_path_buf).or(home) {
            cmd.cwd(dir);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "FerrumEditor");
        cmd.env("TERM_PROGRAM_VERSION", crate::VERSION);
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("could not start the shell: {e}"))?;
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let killer = child.clone_killer();
        let (tx, rx) = unbounded();
        let session = Self {
            term: Arc::new(Mutex::new(Terminal::new(cols, rows))),
            writer: Arc::new(Mutex::new(writer)),
            master: Some(Mutex::new(pair.master)),
            killer: Some(Mutex::new(killer)),
            exited: Arc::new(AtomicBool::new(false)),
            shell: ferrumeditor_core::terminal::shell(configured),
        };
        let (term, out, exited) = (
            session.term.clone(),
            session.writer.clone(),
            session.exited.clone(),
        );
        std::thread::Builder::new()
            .name("ferrumeditor-pty".into())
            .spawn(move || read_loop(reader, &term, &out, &exited, &tx))
            .map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("ferrumeditor-pty-wait".into())
            .spawn(move || {
                let _ = child.wait();
            })
            .map_err(|e| e.to_string())?;
        Ok((session, rx))
    }

    /// A session without a process: input is captured in the returned
    /// buffer and output is fed with [`Session::feed`] (tests).
    #[cfg(test)]
    pub fn detached(cols: u16, rows: u16) -> (Self, Arc<Mutex<Vec<u8>>>) {
        #[derive(Clone)]
        struct Capture(Arc<Mutex<Vec<u8>>>);
        impl Write for Capture {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                lock(&self.0).extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let sent = Arc::new(Mutex::new(Vec::new()));
        let session = Self {
            term: Arc::new(Mutex::new(Terminal::new(cols, rows))),
            writer: Arc::new(Mutex::new(Box::new(Capture(sent.clone())))),
            master: None,
            killer: None,
            exited: Arc::new(AtomicBool::new(false)),
            shell: PathBuf::from("/bin/sh"),
        };
        (session, sent)
    }

    /// Feeds program output directly (tests and replies).
    #[cfg(test)]
    pub fn feed(&self, bytes: &[u8]) {
        lock(&self.term).feed(bytes);
    }

    pub fn write(&self, bytes: &[u8]) {
        let mut writer = lock(&self.writer);
        if writer
            .write_all(bytes)
            .and_then(|_| writer.flush())
            .is_err()
        {
            self.exited.store(true, Ordering::Relaxed);
        }
    }

    /// Encodes `input` against the program's modes and sends it. Typing
    /// scrolls back to the bottom.
    pub fn input(&self, input: &Input) {
        let bytes = {
            let mut term = lock(&self.term);
            if term.scrolled() {
                term.scroll(None);
            }
            term.encode(input)
        };
        if let Some(bytes) = bytes {
            self.write(&bytes);
        }
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
        if let Some(master) = &self.master {
            let _ = lock(master).resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    pub fn frame(&self) -> Frame {
        lock(&self.term).frame()
    }

    /// Scrolls into history (`Some(+n)`), down (`Some(-n)`) or to the end.
    pub fn scroll(&self, lines: Option<i32>) {
        lock(&self.term).scroll(lines);
    }

    /// Clears the screen and scrollback (like Cmd+K in Terminal.app) and
    /// asks the shell to redraw its prompt.
    pub fn clear(&self) {
        lock(&self.term).feed(b"\x1b[H\x1b[2J\x1b[3J");
        self.write(b"\x0c");
    }

    pub fn title(&self) -> String {
        lock(&self.term).title().to_string()
    }

    pub fn has_exited(&self) -> bool {
        self.exited.load(Ordering::Relaxed)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(killer) = &self.killer {
            let _ = lock(killer).kill();
        }
    }
}

fn read_loop(
    mut reader: Box<dyn Read + Send>,
    term: &Mutex<Terminal>,
    writer: &Mutex<Box<dyn Write + Send>>,
    exited: &AtomicBool,
    tx: &UnboundedSender<()>,
) {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let feed = lock(term).feed(&buf[..n]);
        if !feed.reply.is_empty() {
            let _ = lock(writer).write_all(&feed.reply);
        }
        if tx.unbounded_send(()).is_err() {
            return;
        }
    }
    exited.store(true, Ordering::Relaxed);
    let _ = tx.unbounded_send(());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// A real PTY: `sh -c` output reaches the emulator, then EOF is seen.
    #[test]
    fn real_pty_runs_a_command() {
        let dir = tempfile::tempdir().expect("tempdir");
        let script = dir.path().join("hello.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf 'ferrum-%s\\n' \"$(pwd | wc -c | tr -d ' ')\"\n",
        )
        .expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
        }
        let (session, mut rx) =
            Session::spawn(script.to_str(), Some(dir.path()), 40, 5).expect("spawn");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !session.has_exited() && Instant::now() < deadline {
            while rx.try_recv().is_ok() {}
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(session.has_exited(), "the script ends");
        assert!(session.frame().line_text(0).starts_with("ferrum-"));
    }

    #[test]
    fn detached_session_encodes_input() {
        let (session, sent) = Session::detached(20, 4);
        session.input(&Input::Key("C-c".into()));
        session.input(&Input::Text("ls".into()));
        session.input(&Input::Key("Enter".into()));
        assert_eq!(lock(&sent).as_slice(), b"\x03ls\r");
        session.feed(b"hello");
        assert_eq!(session.frame().line_text(0), "hello");
    }
}
