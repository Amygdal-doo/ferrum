//! Reading the master password: from the terminal with echo off (raw
//! mode, so nothing is shown and nothing lands in the shell history), or
//! one line from stdin with `--password-stdin` for scripts.

use std::io::{self, BufRead, IsTerminal, Write};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal;
use zeroize::Zeroizing;

/// Leaves raw mode however the prompt ends.
struct RawMode;

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

/// Prompts on the terminal (stderr), echoing nothing.
pub fn prompt(label: &str) -> Result<Zeroizing<String>, String> {
    if !io::stdin().is_terminal() && !io::stderr().is_terminal() {
        return Err("no terminal to ask for the master password: use --password-stdin".into());
    }
    eprint!("{label}");
    io::stderr().flush().ok();
    let mut password = Zeroizing::new(String::new());
    let raw = terminal::enable_raw_mode()
        .map(|()| RawMode)
        .map_err(|e| format!("could not read from the terminal: {e}"))?;
    let result = loop {
        let event = match event::read() {
            Ok(event) => event,
            Err(e) => break Err(format!("could not read from the terminal: {e}")),
        };
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Enter => break Ok(()),
                    KeyCode::Char('c' | 'd') if ctrl => break Err("cancelled".into()),
                    KeyCode::Esc => break Err("cancelled".into()),
                    KeyCode::Backspace => {
                        password.pop();
                    }
                    KeyCode::Char(c) if !ctrl => password.push(c),
                    _ => {}
                }
            }
            Event::Paste(text) => password.push_str(text.trim_end_matches(['\r', '\n'])),
            _ => {}
        }
    };
    drop(raw);
    eprintln!();
    result.map(|()| password)
}

/// The first line of stdin, without its line ending.
pub fn from_stdin() -> Result<Zeroizing<String>, String> {
    let mut line = Zeroizing::new(String::new());
    io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| format!("could not read the password from stdin: {e}"))?;
    let trimmed = Zeroizing::new(line.trim_end_matches(['\r', '\n']).to_string());
    if trimmed.is_empty() {
        return Err("no password on stdin".into());
    }
    Ok(trimmed)
}
