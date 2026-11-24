//! Command blocks from shell integration (OSC 133, "semantic prompts"):
//! the shell marks where its prompt starts (`A`), where the typed command
//! starts (`B`, optional), where the command's output starts (`C`, with the
//! command line as `cmdline_url=`), and when it finished (`D;<exit code>`).
//!
//! Each mark sets the cursor's template so that every cell written after
//! it carries its command and zone (as a hidden hyperlink id, which frames
//! and captures never show). The marks thus live in the grid itself: they
//! scroll into the scrollback, reflow on resize and leave with the oldest
//! lines like any text. A row belongs to the command of its first marked
//! cell; unmarked (blank) rows belong to the row above. The exit code and
//! command line are kept per command, for the most recent [`KEEP`].

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::term::{Term, TermMode};
use ferrumterm_protocol::{CommandInfo, CommandMark};
use std::collections::VecDeque;

/// Commands whose exit code and command line are remembered.
pub const KEEP: usize = 2_000;
/// Rows looked at above the screen for the command its first row belongs to.
const LOOK_UP: i32 = 2_000;
const ID_PREFIX: &str = "fterm-cmd:";

/// An OSC 133 mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mark {
    /// The prompt starts.
    Prompt,
    /// The command line starts (the prompt ended).
    Input,
    /// The command runs: its output starts. Carries the command line.
    Output(Option<String>),
    /// The command finished with this exit code.
    Done(Option<i32>),
    /// A program's own OSC 8 hyperlink ended (which clears the template).
    LinkEnd,
}

/// Part of a command block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Prompt,
    Input,
    Output,
}

impl Zone {
    fn letter(self) -> char {
        match self {
            Self::Prompt => 'p',
            Self::Input => 'i',
            Self::Output => 'o',
        }
    }
}

/// What the terminal knows of one command.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Record {
    id: u64,
    command: Option<String>,
    exit: Option<i32>,
    /// `C` seen and no `D` yet.
    running: bool,
    /// `C` seen.
    ran: bool,
}

/// Commands and the current zone.
#[derive(Debug, Default)]
pub struct Commands {
    records: VecDeque<Record>,
    next: u64,
    zone: Option<(u64, Zone)>,
}

impl Commands {
    /// Applies `mark` (just after the parser processed it).
    pub fn apply<T>(&mut self, mark: Mark, term: &mut Term<T>) {
        match mark {
            Mark::Prompt => {
                // Prompted again with nothing run (an empty line, a redraw,
                // a second integration): the same command.
                let reuse = self.records.back().filter(|r| !r.ran).map(|r| r.id);
                let id = reuse.unwrap_or_else(|| self.start());
                self.set_zone(Some((id, Zone::Prompt)), term);
            }
            Mark::Input => {
                let id = self.current_or_start();
                self.set_zone(Some((id, Zone::Input)), term);
            }
            Mark::Output(command) => {
                let id = match self.records.back() {
                    Some(r) if !r.ran => r.id,
                    _ => self.start(),
                };
                if let Some(r) = self.records.back_mut() {
                    r.ran = true;
                    r.running = true;
                    r.command = command
                        .map(|c| c.trim().to_owned())
                        .filter(|c| !c.is_empty());
                }
                self.set_zone(Some((id, Zone::Output)), term);
            }
            Mark::Done(exit) => {
                if let Some(r) = self.records.back_mut().filter(|r| r.running) {
                    r.running = false;
                    r.exit = exit;
                    self.set_zone(None, term);
                }
            }
            Mark::LinkEnd => {
                let zone = self.zone;
                self.set_zone(zone, term);
            }
        }
    }

    fn start(&mut self) -> u64 {
        self.next += 1;
        // A command that never got its `D` is over once the next starts.
        if let Some(r) = self.records.back_mut() {
            r.running = false;
        }
        self.records.push_back(Record {
            id: self.next,
            command: None,
            exit: None,
            running: false,
            ran: false,
        });
        if self.records.len() > KEEP {
            self.records.pop_front();
        }
        self.next
    }

    fn current_or_start(&mut self) -> u64 {
        match self.records.back() {
            Some(r) if !r.ran => r.id,
            _ => self.start(),
        }
    }

    fn set_zone<T>(&mut self, zone: Option<(u64, Zone)>, term: &mut Term<T>) {
        self.zone = zone;
        let link = zone.map(|(id, z)| {
            Hyperlink::new(
                Some(format!("{ID_PREFIX}{id}:{}", z.letter())),
                "fterm:".into(),
            )
        });
        term.grid_mut().cursor.template.set_hyperlink(link);
    }

    fn record(&self, id: u64) -> Option<&Record> {
        let i = self.records.binary_search_by_key(&id, |r| r.id).ok()?;
        self.records.get(i)
    }

    /// Whether there is nothing to look for: no mark seen yet (no shell
    /// integration), or a full-screen program (its screen is no command
    /// output to group).
    fn idle<T>(&self, term: &Term<T>) -> bool {
        self.records.is_empty() || term.mode().contains(TermMode::ALT_SCREEN)
    }

    /// The commands with rows on `term`'s screen.
    pub fn on_screen<T>(&self, term: &Term<T>) -> Vec<CommandMark> {
        if self.idle(term) {
            return Vec::new();
        }
        let grid = term.grid();
        let offset = grid.display_offset() as i32;
        let rows = grid.screen_lines() as i32;
        let last = last_line(term);
        let above = zone_above(term, -offset);
        let mut carry = above;
        let mut out: Vec<CommandMark> = Vec::new();
        for r in 0..rows {
            let line = r - offset;
            if line > last {
                break;
            }
            let zone = row_zone(&grid[Line(line)], grid.columns()).or_else(|| inherit(carry));
            carry = zone;
            let Some((id, zone)) = zone else { continue };
            let row = r as u16;
            let input = input_col(&grid[Line(line)], grid.columns(), id).map(|c| (row, c));
            match out.last_mut() {
                Some(m) if m.id == id => {
                    m.bottom = row;
                    if zone == Zone::Output && m.output.is_none() {
                        m.output = Some(row);
                    }
                    if m.input.is_none() {
                        m.input = input;
                    }
                }
                _ => {
                    let record = self.record(id);
                    out.push(CommandMark {
                        id,
                        top: row,
                        bottom: row,
                        starts: above.is_none_or(|(a, _)| a != id) || r > 0,
                        output: (zone == Zone::Output).then_some(row),
                        exit: record.and_then(|r| r.exit),
                        running: record.is_some_and(|r| r.running),
                        command: record.and_then(|r| r.command.clone()).unwrap_or_default(),
                        input,
                    });
                }
            }
        }
        out
    }

    /// Every command still in the scrollback or on the screen, oldest first.
    pub fn all<T>(&self, term: &Term<T>) -> Vec<CommandInfo> {
        if self.idle(term) {
            return Vec::new();
        }
        let grid = term.grid();
        let top = -(grid.history_size() as i32);
        let mut carry = None;
        let mut out: Vec<CommandInfo> = Vec::new();
        for l in top..=last_line(term) {
            let zone = row_zone(&grid[Line(l)], grid.columns()).or_else(|| inherit(carry));
            carry = zone;
            let Some((id, zone)) = zone else { continue };
            let line = (l - top) as u32;
            match out.last_mut() {
                Some(c) if c.id == id => {
                    c.end_line = line;
                    if zone == Zone::Output && c.output_line.is_none() {
                        c.output_line = Some(line);
                    }
                }
                _ => {
                    let record = self.record(id);
                    out.push(CommandInfo {
                        id,
                        line,
                        output_line: (zone == Zone::Output).then_some(line),
                        end_line: line,
                        exit: record.and_then(|r| r.exit),
                        running: record.is_some_and(|r| r.running),
                        command: record.and_then(|r| r.command.clone()).unwrap_or_default(),
                    });
                }
            }
        }
        out
    }

    /// Command `id` and the text of its output (soft-wrapped rows joined,
    /// trailing blanks trimmed).
    pub fn output<T>(&self, term: &Term<T>, id: u64) -> Option<(CommandInfo, String)> {
        let info = self.all(term).into_iter().find(|c| c.id == id)?;
        let grid = term.grid();
        let top = -(grid.history_size() as i32);
        let mut text = String::new();
        if let Some(first) = info.output_line {
            for line in first..=info.end_line {
                let row = &grid[Line(line as i32 + top)];
                let mut s = String::new();
                for c in 0..grid.columns() {
                    let cell = &row[Column(c)];
                    if !cell
                        .flags
                        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                    {
                        s.push(if cell.c == '\0' { ' ' } else { cell.c });
                    }
                }
                let wraps = row[Column(grid.columns() - 1)]
                    .flags
                    .contains(Flags::WRAPLINE);
                if wraps {
                    text.push_str(&s);
                } else {
                    text.push_str(s.trim_end());
                    text.push('\n');
                }
            }
        }
        let trimmed = text.trim_end().len();
        text.truncate(trimmed);
        Some((info, text))
    }
}

/// The last line that can belong to a command: the cursor's (below it the
/// main screen is empty).
fn last_line<T>(term: &Term<T>) -> i32 {
    term.grid().cursor.point.line.0
}

/// The command and zone of the first marked cell of `row`.
fn row_zone(row: &alacritty_terminal::grid::Row<Cell>, cols: usize) -> Option<(u64, Zone)> {
    (0..cols).find_map(|c| {
        let link = row[Column(c)].hyperlink()?;
        parse_id(link.id())
    })
}

/// The first column of `row` holding command `id`'s typed command line.
fn input_col(row: &alacritty_terminal::grid::Row<Cell>, cols: usize, id: u64) -> Option<u16> {
    (0..cols).find_map(|c| {
        let link = row[Column(c)].hyperlink()?;
        (parse_id(link.id())? == (id, Zone::Input)).then_some(c as u16)
    })
}

/// An unmarked row continues the row above; after the command line, it
/// is output.
fn inherit(above: Option<(u64, Zone)>) -> Option<(u64, Zone)> {
    above.map(|(id, zone)| match zone {
        Zone::Input => (id, Zone::Output),
        z => (id, z),
    })
}

/// The command and zone of the line above `line` (looking further up past
/// unmarked rows).
fn zone_above<T>(term: &Term<T>, line: i32) -> Option<(u64, Zone)> {
    let grid = term.grid();
    let top = -(grid.history_size() as i32);
    let mut l = line - 1;
    while l >= top && line - l <= LOOK_UP {
        if let Some(zone) = row_zone(&grid[Line(l)], grid.columns()) {
            return Some(zone);
        }
        l -= 1;
    }
    None
}

fn parse_id(id: &str) -> Option<(u64, Zone)> {
    let rest = id.strip_prefix(ID_PREFIX)?;
    let (n, zone) = rest.split_once(':')?;
    let zone = match zone {
        "p" => Zone::Prompt,
        "i" => Zone::Input,
        "o" => Zone::Output,
        _ => return None,
    };
    Some((n.parse().ok()?, zone))
}

/// Parses an OSC body (`133;…` or `8;…`); `None` for other OSCs.
fn parse_body(body: &[u8]) -> Option<Mark> {
    let body = std::str::from_utf8(body).ok()?;
    if let Some(rest) = body.strip_prefix("8;") {
        // `8;params;uri`: an empty URI ends the link.
        return rest
            .split_once(';')
            .filter(|(_, uri)| uri.is_empty())
            .map(|_| Mark::LinkEnd);
    }
    let rest = body.strip_prefix("133;")?;
    let mut parts = rest.split(';');
    Some(match parts.next()? {
        "A" => Mark::Prompt,
        "B" => Mark::Input,
        "C" => Mark::Output(parts.find_map(|p| {
            if let Some(url) = p.strip_prefix("cmdline_url=") {
                Some(
                    percent_encoding::percent_decode_str(url)
                        .decode_utf8_lossy()
                        .into_owned(),
                )
            } else {
                p.strip_prefix("cmdline=").map(str::to_owned)
            }
        })),
        "D" => Mark::Done(parts.next().and_then(|c| c.trim().parse().ok())),
        _ => return None,
    })
}

/// Finds OSC 133 marks (and OSC 8 link ends) in a PTY byte stream, across
/// chunk boundaries: each with the offset just after its terminator.
#[derive(Debug, Default)]
pub struct MarkScanner {
    state: State,
    buf: Vec<u8>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    Esc,
    Osc,
    OscEsc,
}

const MAX_BODY: usize = 16 * 1024;

impl MarkScanner {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<(usize, Mark)> {
        let mut out = Vec::new();
        for (i, &b) in bytes.iter().enumerate() {
            self.state = match (self.state, b) {
                (State::Ground, 0x1b) => State::Esc,
                (State::Ground, _) => State::Ground,
                (State::Esc, b']') => {
                    self.buf.clear();
                    State::Osc
                }
                (State::Esc, 0x1b) => State::Esc,
                (State::Esc, _) => State::Ground,
                (State::Osc, 0x07) => {
                    self.finish(i + 1, &mut out);
                    State::Ground
                }
                (State::Osc, 0x1b) => State::OscEsc,
                (State::Osc, _) => {
                    // Only `133;` and `8;` bodies are kept.
                    let ours = match self.buf.len() {
                        0 => b == b'1' || b == b'8',
                        1 if self.buf[0] == b'8' => b == b';',
                        n @ 1..=3 if self.buf[0] == b'1' => b"133;"[n] == b,
                        _ => true,
                    };
                    if !ours || self.buf.len() >= MAX_BODY {
                        State::Ground
                    } else {
                        self.buf.push(b);
                        State::Osc
                    }
                }
                (State::OscEsc, b'\\') => {
                    self.finish(i + 1, &mut out);
                    State::Ground
                }
                (State::OscEsc, b']') => {
                    self.buf.clear();
                    State::Osc
                }
                (State::OscEsc, _) => State::Ground,
            };
        }
        out
    }

    fn finish(&mut self, end: usize, out: &mut Vec<(usize, Mark)>) {
        if let Some(mark) = parse_body(&self.buf) {
            out.push((end, mark));
        }
        self.buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_marks_across_chunks_with_their_offsets() {
        let mut scan = MarkScanner::default();
        let stream: &[u8] =
            b"\x1b]133;A\x07$ \x1b]133;C;cmdline_url=echo%20hi%21\x1b\\hi\r\n\x1b]133;D;2\x07\x1b]0;t\x07\x1b]8;;\x1b\\";
        let mut found = Vec::new();
        let mut base = 0;
        for chunk in stream.chunks(5) {
            found.extend(scan.feed(chunk).into_iter().map(|(end, m)| (base + end, m)));
            base += chunk.len();
        }
        let marks: Vec<Mark> = found.iter().map(|(_, m)| m.clone()).collect();
        assert_eq!(
            marks,
            [
                Mark::Prompt,
                Mark::Output(Some("echo hi!".into())),
                Mark::Done(Some(2)),
                Mark::LinkEnd,
            ]
        );
        assert_eq!(&stream[found[0].0..found[0].0 + 2], b"$ ");
        assert_eq!(&stream[found[1].0..found[1].0 + 2], b"hi");
        assert_eq!(
            parse_body(b"133;C;cmdline=ls -l"),
            Some(Mark::Output(Some("ls -l".into())))
        );
        assert_eq!(parse_body(b"133;B"), Some(Mark::Input));
        assert_eq!(parse_body(b"8;id=x;https://a"), None, "a link starting");
        assert_eq!(parse_body(b"7501;state=done"), None);
    }
}
