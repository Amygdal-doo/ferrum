//! Click to move the cursor in the command line. At the shell's prompt
//! (shell integration), a click on the line being typed moves the cursor
//! there by pressing Left or Right once per character in between, so it
//! works with any line editor (zsh, bash, fish). Clicks before the typed
//! command go to its start, clicks past it to its end.

use ferrumterm_protocol::Frame;
use unicode_width::UnicodeWidthChar;

/// Most arrow presses one click sends.
const MAX_PRESSES: usize = 1_000;

/// The arrow key and how many presses move the cursor of `frame` to screen
/// cell `(row, col)`; `None` when the shell isn't waiting for a command
/// there or the cursor is already in place.
#[must_use]
pub fn keys(frame: &Frame, row: u16, col: u16) -> Option<(&'static str, usize)> {
    if frame.alt_screen || frame.mouse || frame.display_offset != 0 {
        return None;
    }
    let cursor = frame.cursor?;
    let command = frame.commands.last()?;
    let at_prompt = command.output.is_none()
        && !command.running
        && (command.top..=command.bottom).contains(&cursor.row);
    if !at_prompt || row != cursor.row {
        return None;
    }
    let start = match command.input {
        Some((r, c)) if r == row => c,
        Some((r, _)) if r < row => 0,
        Some(_) => return None,
        // Nothing typed yet: the cursor is where typing starts.
        None => cursor.col,
    };
    let starts = char_columns(frame, row);
    let end = starts
        .iter()
        .rev()
        .find(|(_, ch)| !ch.is_whitespace())
        .map_or(0, |(c, ch)| c + ch.width().unwrap_or(1) as u16)
        .max(cursor.col);
    let target = col.clamp(start.min(end), end);
    let between = |from: u16, to: u16| {
        starts
            .iter()
            .filter(|(c, _)| (from..to).contains(c))
            .count()
            .min(MAX_PRESSES)
    };
    match target.cmp(&cursor.col) {
        std::cmp::Ordering::Greater => Some(("Right", between(cursor.col, target))),
        std::cmp::Ordering::Less => Some(("Left", between(target, cursor.col))),
        std::cmp::Ordering::Equal => None,
    }
    .filter(|(_, n)| *n > 0)
}

/// Each character of screen row `row` with the column it starts at
/// (wide characters take two columns; combining marks none).
fn char_columns(frame: &Frame, row: u16) -> Vec<(u16, char)> {
    let mut out = Vec::new();
    let mut col: u16 = 0;
    for span in frame.lines.get(usize::from(row)).into_iter().flatten() {
        for ch in span.t.chars() {
            let w = ch.width().unwrap_or(0) as u16;
            if w > 0 {
                out.push((col, ch));
                col += w;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumterm_protocol::{CommandMark, Cursor, CursorShape, Span};

    /// A one-row prompt `~ $ ` followed by `typed`, the cursor at `cursor`.
    fn prompt(typed: &str, cursor: u16) -> Frame {
        Frame {
            cols: 40,
            rows: 2,
            lines: vec![
                vec![Span {
                    t: format!("~ $ {typed}"),
                    ..Span::default()
                }],
                Vec::new(),
            ],
            cursor: Some(Cursor {
                row: 0,
                col: cursor,
                shape: CursorShape::Beam,
            }),
            commands: vec![CommandMark {
                id: 1,
                starts: true,
                input: (!typed.is_empty()).then_some((0, 4)),
                ..CommandMark::default()
            }],
            ..Frame::default()
        }
    }

    #[test]
    fn moves_left_and_right_within_the_command() {
        let f = prompt("git status", 14);
        assert_eq!(keys(&f, 0, 8), Some(("Left", 6)));
        let f = prompt("git status", 4);
        assert_eq!(keys(&f, 0, 7), Some(("Right", 3)));
        assert_eq!(keys(&f, 0, 4), None, "already there");
    }

    #[test]
    fn clamps_to_the_typed_command() {
        let f = prompt("ls -la", 7);
        // On the prompt itself: the start of the command.
        assert_eq!(keys(&f, 0, 1), Some(("Left", 3)));
        // Past the end: the end.
        assert_eq!(keys(&f, 0, 30), Some(("Right", 3)));
        // Nothing typed: nowhere to go.
        assert_eq!(keys(&prompt("", 4), 0, 20), None);
    }

    #[test]
    fn counts_wide_characters_once() {
        // "ab日本c": 日 and 本 take two columns each.
        let f = prompt("ab日本c", 11);
        assert_eq!(keys(&f, 0, 6), Some(("Left", 3)));
    }

    #[test]
    fn only_at_a_prompt_on_the_cursor_row() {
        let f = prompt("ls", 6);
        assert_eq!(keys(&f, 1, 2), None, "another row");
        let mut running = f.clone();
        running.commands[0].running = true;
        assert_eq!(keys(&running, 0, 4), None);
        let mut full_screen = f.clone();
        full_screen.alt_screen = true;
        assert_eq!(keys(&full_screen, 0, 4), None);
        let mut plain = f;
        plain.commands.clear();
        assert_eq!(keys(&plain, 0, 4), None, "no shell integration");
    }
}
