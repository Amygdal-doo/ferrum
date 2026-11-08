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
