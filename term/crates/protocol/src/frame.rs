//! A rendered screen: rows of styled spans plus cursor. Colors are resolved
//! RGB (`0xRRGGBB`); `None` means the client's default foreground/background.

use serde::{Deserialize, Serialize};

/// Span attribute bits.
pub mod flags {
    pub const BOLD: u8 = 1;
    pub const ITALIC: u8 = 1 << 1;
    pub const UNDERLINE: u8 = 1 << 2;
    pub const STRIKE: u8 = 1 << 3;
    pub const DIM: u8 = 1 << 4;
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Span {
    pub t: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<u32>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub f: u8,
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorShape {
    #[default]
    Block,
    Beam,
    Underline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    pub row: u16,
    pub col: u16,
    pub shape: CursorShape,
}

/// A shell command and its output on a [`Frame`]'s screen, from shell
/// integration (OSC 133 marks; see the emulator's `marks`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CommandMark {
    /// Numbers the pane's commands, oldest first.
    pub id: u64,
    /// The screen rows it covers, `top` to `bottom` inclusive (prompt,
    /// command line and output).
    pub top: u16,
    pub bottom: u16,
    /// Its first line is `top` (else it began above the screen).
    pub starts: bool,
    /// The first row of its output on the screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<u16>,
    /// Exit code once it finished (and the shell said).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<i32>,
    #[serde(default)]
    pub running: bool,
    /// The command line, when the shell sent it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    /// Where the typed command line starts on the screen, `(row, col)`,
    /// once something was typed (server revision 4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<(u16, u16)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Frame {
    pub cols: u16,
    pub rows: u16,
    /// Exactly `rows` lines; each line's spans cover up to `cols` cells.
    pub lines: Vec<Vec<Span>>,
    /// `None` when hidden or scrolled out of view.
    pub cursor: Option<Cursor>,
    pub title: String,
    /// Lines scrolled back from the bottom.
    pub display_offset: u32,
    /// Lines of scrollback available.
    pub history: u32,
    pub bracketed_paste: bool,
    pub alt_screen: bool,
    /// Mouse reporting (SGR) requested by the program.
    pub mouse: bool,
    /// Shell commands on the screen, top to bottom (none without shell
    /// integration, or from servers before revision 3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<CommandMark>,
}

impl Frame {
    /// Plain text of one line (spans concatenated, trailing spaces trimmed).
    #[must_use]
    pub fn line_text(&self, row: usize) -> String {
        self.lines
            .get(row)
            .map(|l| {
                l.iter()
                    .map(|s| s.t.as_str())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .unwrap_or_default()
    }
}
