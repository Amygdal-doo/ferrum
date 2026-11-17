//! Command blocks on screen (shell integration: each command with its
//! output, see `Frame::commands`): which rows a pane shows once collapsed
//! outputs are folded away, which command a row belongs to, and where ⌘↑ /
//! ⌘↓ go. The toolbar and actions are in `view::blocks`; `paint` draws the
//! separators and marks.

use ferrumterm_protocol::{CommandInfo, CommandMark, Frame};
use std::collections::HashSet;

/// The frame rows a pane shows, top to bottom: every row except the output
/// of `collapsed` commands (finished ones only; a running command's output
/// keeps showing).
#[must_use]
pub(crate) fn visible_rows(frame: &Frame, collapsed: &HashSet<u64>) -> Vec<u16> {
    let folded: Vec<(u16, u16)> = frame
        .commands
        .iter()
        .filter(|c| !c.running && collapsed.contains(&c.id))
        .filter_map(|c| c.output.map(|o| (o, c.bottom)))
        .collect();
    (0..frame.rows)
        .filter(|r| !folded.iter().any(|(from, to)| (from..=to).contains(&r)))
        .collect()
}

/// The command frame row `row` belongs to.
#[must_use]
pub(crate) fn at_row(frame: &Frame, row: u16) -> Option<&CommandMark> {
    frame
        .commands
        .iter()
        .find(|c| (c.top..=c.bottom).contains(&row))
}

/// Whether a command failed (a known non-zero exit code).
#[must_use]
pub(crate) fn failed(c: &CommandMark) -> bool {
    c.exit.is_some_and(|code| code != 0)
}

/// Which way ⌘↑ / ⌘↓ go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Jump {
    Previous,
    Next,
}

/// Where ⌘↑ / ⌘↓ go from `frame`'s view of `commands`: the command to
/// show at the top of the screen, and the lines to scroll there
/// (`Request::Scroll`). ⌘↑ goes to the nearest command starting above the
/// top row; ⌘↓ to the next one below it, or back to the bottom after the
/// last.
#[must_use]
pub(crate) fn jump(
    frame: &Frame,
    commands: &[CommandInfo],
    to: Jump,
) -> Option<(Option<u64>, i32)> {
    let history = i64::from(frame.history);
    let offset = i64::from(frame.display_offset);
    let top = history - offset;
    let target = match to {
        Jump::Previous => commands.iter().rev().find(|c| i64::from(c.line) < top),
        Jump::Next => commands.iter().find(|c| i64::from(c.line) > top),
    };
    let (id, want) = match (target, to) {
        (Some(c), _) => (Some(c.id), (history - i64::from(c.line)).clamp(0, history)),
        (None, Jump::Next) => (None, 0),
        (None, Jump::Previous) => return None,
    };
    let lines = i32::try_from(want - offset).ok()?;
    (lines != 0 || id.is_some()).then_some((id, lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(id: u64, top: u16, bottom: u16, output: Option<u16>) -> CommandMark {
        CommandMark {
            id,
            top,
            bottom,
            starts: true,
            output,
            exit: Some(0),
            ..CommandMark::default()
        }
    }

    fn frame(history: u32, offset: u32, commands: Vec<CommandMark>) -> Frame {
        Frame {
            rows: 8,
            cols: 20,
            lines: vec![Vec::new(); 8],
            history,
            display_offset: offset,
            commands,
            ..Frame::default()
        }
    }

    fn info(id: u64, line: u32) -> CommandInfo {
        CommandInfo {
            id,
            line,
            output_line: Some(line + 1),
            end_line: line + 3,
            command: String::new(),
            exit: Some(0),
            running: false,
        }
    }

    #[test]
    fn collapsing_folds_a_finished_commands_output() {
        let mut running = mark(2, 4, 7, Some(5));
        running.running = true;
        let f = frame(0, 0, vec![mark(1, 0, 3, Some(1)), running]);
        let all: Vec<u16> = (0..8).collect();
        assert_eq!(visible_rows(&f, &HashSet::new()), all);
        assert_eq!(visible_rows(&f, &HashSet::from([1])), [0, 4, 5, 6, 7]);
        assert_eq!(visible_rows(&f, &HashSet::from([2])), all, "still running");
        assert_eq!(at_row(&f, 2).map(|c| c.id), Some(1));
        assert_eq!(at_row(&f, 6).map(|c| c.id), Some(2));
    }

    #[test]
    fn jumps_between_command_starts() {
        // Commands at lines 0, 10, 20 and 30; 30 lines of history, at the
        // bottom (the screen's top row is line 30).
        let commands = [info(1, 0), info(2, 10), info(3, 20), info(4, 30)];
        let bottom = frame(30, 0, Vec::new());
        assert_eq!(
            jump(&bottom, &commands, Jump::Previous),
            Some((Some(3), 10))
        );
        assert_eq!(
            jump(&bottom, &commands, Jump::Next),
            None,
            "already at the bottom"
        );
        // Showing line 10 at the top.
        let at_two = frame(30, 20, Vec::new());
        assert_eq!(
            jump(&at_two, &commands, Jump::Previous),
            Some((Some(1), 10))
        );
        assert_eq!(jump(&at_two, &commands, Jump::Next), Some((Some(3), -10)));
        // Past the last one: back to the bottom.
        let at_three = frame(30, 10, Vec::new());
        assert_eq!(jump(&at_three, &commands, Jump::Next), Some((Some(4), -10)));
        let at_top = frame(30, 30, Vec::new());
        assert_eq!(jump(&at_top, &commands, Jump::Previous), None);
    }
}
