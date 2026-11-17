//! VT emulation for one block, on `alacritty_terminal` (the emulator inside
//! Alacritty). Adds what a multiplexer needs on top: screen frames for
//! clients, text capture, OSC 7501 program status, replies the program asks
//! for (cursor reports etc.), and key-spec → byte encoding.

pub mod keys;
mod palette;
mod search;
mod snapshot;

/// Default palette, so embedders can map emulator colours onto their own theme.
pub use palette::{ANSI, DEFAULT_BG, DEFAULT_FG};

use alacritty_terminal::event::{Event as TermEvent, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use ferrumterm_protocol::status::{OscScanner, StatusUpdate, parse_osc_body};
use ferrumterm_protocol::{FindMatch, Frame, Input};
use std::sync::{Arc, Mutex, PoisonError};

/// Scrollback lines kept per block.
pub const SCROLLBACK: usize = 10_000;

#[derive(Clone, Default)]
struct Listener(Arc<Mutex<Vec<TermEvent>>>);

impl EventListener for Listener {
    fn send_event(&self, event: TermEvent) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event);
    }
}

/// Side effects of feeding PTY output.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Feed {
    /// Bytes to write back to the PTY (device status replies…).
    pub reply: Vec<u8>,
    pub title_changed: bool,
    pub bell: bool,
    pub status: Vec<StatusUpdate>,
}

pub struct Terminal {
    term: Term<Listener>,
    parser: Processor,
    events: Listener,
    scanner: OscScanner,
    title: String,
}

impl Terminal {
    #[must_use]
    pub fn new(cols: u16, rows: u16) -> Self {
        let events = Listener::default();
        let config = Config {
            scrolling_history: SCROLLBACK,
            ..Config::default()
        };
        let size = TermSize::new(usize::from(cols.max(2)), usize::from(rows.max(1)));
        Self {
            term: Term::new(config, &size, events.clone()),
            parser: Processor::new(),
            events,
            scanner: OscScanner::default(),
            title: String::new(),
        }
    }

    /// Processes program output.
    pub fn feed(&mut self, bytes: &[u8]) -> Feed {
        let mut feed = Feed::default();
        for body in self.scanner.feed(bytes) {
            if let Some(u) = parse_osc_body(&body) {
                feed.status.push(u);
            }
        }
        self.parser.advance(&mut self.term, bytes);
        let events =
            std::mem::take(&mut *self.events.0.lock().unwrap_or_else(PoisonError::into_inner));
        for e in events {
            match e {
                TermEvent::PtyWrite(s) => feed.reply.extend_from_slice(s.as_bytes()),
                TermEvent::Title(t) => {
                    self.title = t.chars().take(200).collect();
                    feed.title_changed = true;
                }
                TermEvent::ResetTitle => {
                    self.title.clear();
                    feed.title_changed = true;
                }
                TermEvent::Bell => feed.bell = true,
                TermEvent::ColorRequest(i, fmt) => {
                    let rgb = palette::request_color(&self.term, i);
                    feed.reply.extend_from_slice(fmt(rgb).as_bytes());
                }
                TermEvent::TextAreaSizeRequest(fmt) => {
                    let ws = alacritty_terminal::event::WindowSize {
                        num_lines: self.rows(),
                        num_cols: self.cols(),
                        cell_width: 8,
                        cell_height: 16,
                    };
                    feed.reply.extend_from_slice(fmt(ws).as_bytes());
                }
                _ => {}
            }
        }
        feed
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.term.resize(TermSize::new(
            usize::from(cols.max(2)),
            usize::from(rows.max(1)),
        ));
    }

    #[must_use]
    pub fn cols(&self) -> u16 {
        u16::try_from(self.term.columns()).unwrap_or(u16::MAX)
    }

    #[must_use]
    pub fn rows(&self) -> u16 {
        u16::try_from(self.term.screen_lines()).unwrap_or(u16::MAX)
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Scrolls the viewport into history (`Some(+n)`), towards the bottom
    /// (`Some(-n)`), or back to the bottom (`None`).
    pub fn scroll(&mut self, lines: Option<i32>) {
        match lines {
            Some(n) => self.term.scroll_display(Scroll::Delta(n)),
            None => self.term.scroll_display(Scroll::Bottom),
        }
    }

    #[must_use]
    pub fn frame(&self) -> Frame {
        snapshot::frame(&self.term, &self.title)
    }

    /// Screen (and optionally scrollback) as plain text, trailing blanks trimmed.
    #[must_use]
    pub fn capture(&self, scrollback: bool) -> String {
        snapshot::capture(&self.term, scrollback)
    }

    /// Searches the screen and scrollback: the matches, oldest first, and
    /// whether there were more than [`search::MAX_MATCHES`]. `Err` explains
    /// an invalid regex.
    pub fn find(
        &self,
        query: &str,
        case_sensitive: bool,
        regex: bool,
    ) -> Result<(Vec<FindMatch>, bool), String> {
        search::find(&self.term, query, case_sensitive, regex)
    }

    /// Bytes to send to the PTY for `input`, honouring the program's modes
    /// (application cursor keys, bracketed paste). Unknown key specs yield `None`.
    #[must_use]
    pub fn encode(&self, input: &Input) -> Option<Vec<u8>> {
        let mode = *self.term.mode();
        match input {
            Input::Text(t) | Input::Raw(t) => Some(t.as_bytes().to_vec()),
            Input::Paste(t) => {
                // Strip ESC so pasted text cannot end the paste early.
                let clean = t.replace('\x1b', "");
                Some(if mode.contains(TermMode::BRACKETED_PASTE) {
                    format!("\x1b[200~{clean}\x1b[201~").into_bytes()
                } else {
                    clean.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
                })
            }
            Input::Key(spec) => keys::encode(spec, mode.contains(TermMode::APP_CURSOR)),
        }
    }

    /// Whether the viewport is scrolled back.
    #[must_use]
    pub fn scrolled(&self) -> bool {
        self.term.grid().display_offset() > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumterm_protocol::{ProgramState, flags};

    #[test]
    fn renders_text_colors_and_cursor() {
        let mut t = Terminal::new(20, 4);
        t.feed(b"hi \x1b[1;31mred\x1b[0m\r\nnext");
        let f = t.frame();
        assert_eq!((f.cols, f.rows, f.lines.len()), (20, 4, 4));
        assert_eq!(f.line_text(0), "hi red");
        let red = f.lines[0].iter().find(|s| s.t == "red").expect("red span");
        assert_eq!(red.fg, Some(palette::ANSI[1]));
        assert_ne!(red.f & flags::BOLD, 0);
        assert_eq!(f.line_text(1), "next");
        let c = f.cursor.expect("cursor");
        assert_eq!((c.row, c.col), (1, 4));
    }

    #[test]
    fn title_status_and_device_reply() {
        let mut t = Terminal::new(20, 4);
        let s = ferrumterm_protocol::ProgramStatus::new(ProgramState::Working).to_osc();
        let feed = t.feed(format!("\x1b]2;my agent\x07{s}\x1b[6n").as_bytes());
        assert!(feed.title_changed);
        assert_eq!(t.title(), "my agent");
        assert!(
            matches!(feed.status.as_slice(), [StatusUpdate::Set(p)] if p.state == ProgramState::Working)
        );
        assert_eq!(feed.reply, b"\x1b[1;1R", "cursor position report");
    }

    #[test]
    fn capture_includes_scrollback_and_resize() {
        let mut t = Terminal::new(10, 3);
        for i in 0..6 {
            t.feed(format!("line{i}\r\n").as_bytes());
        }
        assert_eq!(t.capture(false), "line4\nline5");
        assert!(t.capture(true).starts_with("line0\nline1"));
        t.scroll(Some(2));
        assert!(t.scrolled());
        assert_eq!(t.frame().line_text(0), "line2");
        t.scroll(None);
        t.resize(30, 5);
        assert_eq!((t.cols(), t.rows()), (30, 5));
    }

    #[test]
    fn finds_in_scrollback_across_wraps_and_wide_characters() {
        let mut t = Terminal::new(10, 3);
        t.feed(b"Error one\r\nok\r\n");
        // Wraps after ten cells: "abcdefERRO" / "R here".
        t.feed(b"abcdefERROR here\r\n\xe4\xb8\xad error\r\nlast");
        let (found, truncated) = t.find("error", false, false).expect("find");
        assert!(!truncated);
        let at = |m: &FindMatch| (m.line, m.col, m.end_line, m.end_col);
        let places: Vec<_> = found.iter().map(at).collect();
        // "中" is two cells wide, so "error" after it starts at column 3.
        assert_eq!(places, [(0, 0, 0, 5), (2, 6, 3, 1), (4, 3, 4, 8)]);
        // The screen's top row is line `history - display_offset`.
        let f = t.frame();
        assert_eq!(f.history, 3);
        assert_eq!(f.line_text(1), "\u{4e2d} error");
        let (found, _) = t.find("error", true, false).expect("find");
        assert_eq!(found.len(), 1, "case sensitive: only the last one");
        let (found, _) = t.find("o[nk]", true, true).expect("regex");
        assert_eq!(
            found.iter().map(at).collect::<Vec<_>>(),
            [(0, 6, 0, 8), (1, 0, 1, 2)]
        );
        assert!(t.find("(", false, true).is_err(), "invalid regex");
        assert_eq!(t.find("(", false, false).expect("plain").0.len(), 0);
        assert!(t.find("", false, false).expect("empty").0.is_empty());
    }

    #[test]
    fn encodes_keys_and_bracketed_paste() {
        let mut t = Terminal::new(10, 3);
        assert_eq!(t.encode(&Input::Key("Up".into())), Some(b"\x1b[A".to_vec()));
        t.feed(b"\x1b[?1h\x1b[?2004h");
        assert_eq!(t.encode(&Input::Key("Up".into())), Some(b"\x1bOA".to_vec()));
        assert_eq!(
            t.encode(&Input::Paste("a\x1bb".into())),
            Some(b"\x1b[200~ab\x1b[201~".to_vec())
        );
        assert_eq!(t.encode(&Input::Key("nonsense-key".into())), None);
    }
}
