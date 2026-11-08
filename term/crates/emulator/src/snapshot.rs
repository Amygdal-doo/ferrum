//! Grid → [`Frame`] and plain-text capture.

use crate::palette::{self, DEFAULT_BG, DEFAULT_FG};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::CursorShape as VtCursor;
use ferrumterm_protocol::{Cursor, CursorShape, Frame, Span, flags};

fn style(
    cell: &Cell,
    colors: &alacritty_terminal::term::color::Colors,
) -> (Option<u32>, Option<u32>, u8) {
    let mut fg = palette::resolve(cell.fg, colors);
    let mut bg = palette::resolve(cell.bg, colors);
    if cell.flags.contains(Flags::INVERSE) {
        let (f, b) = (bg.unwrap_or(DEFAULT_BG), fg.unwrap_or(DEFAULT_FG));
        fg = Some(f);
        bg = Some(b);
    }
    if cell.flags.contains(Flags::HIDDEN) {
        fg = Some(bg.unwrap_or(DEFAULT_BG));
    }
    let mut f = 0;
    for (flag, bit) in [
        (Flags::BOLD, flags::BOLD),
        (Flags::ITALIC, flags::ITALIC),
        (Flags::DIM, flags::DIM),
        (Flags::STRIKEOUT, flags::STRIKE),
    ] {
        if cell.flags.contains(flag) {
            f |= bit;
        }
    }
    if cell.flags.intersects(Flags::ALL_UNDERLINES) {
        f |= flags::UNDERLINE;
    }
    (fg, bg, f)
}

fn push_cell(text: &mut String, cell: &Cell) {
    text.push(if cell.c == '\0' { ' ' } else { cell.c });
    if let Some(zw) = cell.zerowidth() {
        text.extend(zw);
    }
}

pub fn frame<T: alacritty_terminal::event::EventListener>(term: &Term<T>, title: &str) -> Frame {
    let grid = term.grid();
    let (cols, rows) = (grid.columns(), grid.screen_lines());
    let offset = grid.display_offset();
    let colors = term.colors();
    let mut lines = Vec::with_capacity(rows);
    for r in 0..rows {
        let row = &grid[Line(r as i32 - offset as i32)];
        let mut spans: Vec<Span> = Vec::new();
        for c in 0..cols {
            let cell = &row[Column(c)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let (fg, bg, f) = style(cell, colors);
            match spans.last_mut() {
                Some(s) if s.fg == fg && s.bg == bg && s.f == f => push_cell(&mut s.t, cell),
                _ => {
                    let mut t = String::new();
                    push_cell(&mut t, cell);
                    spans.push(Span { t, fg, bg, f });
                }
            }
        }
        // Drop a trailing default-styled run of blanks (the client paints the background).
        if spans
            .last()
            .is_some_and(|s| s.bg.is_none() && s.f == 0 && s.t.trim_end().is_empty())
        {
            spans.pop();
        } else if let Some(s) = spans
            .last_mut()
            .filter(|s| s.bg.is_none() && s.f & flags::UNDERLINE == 0)
        {
            let trimmed = s.t.trim_end().len();
            s.t.truncate(trimmed);
        }
        lines.push(spans);
    }
    let content = term.renderable_content();
    let mode = *term.mode();
    let cursor_line = content.cursor.point.line.0 + offset as i32;
    let cursor = (mode.contains(TermMode::SHOW_CURSOR)
        && content.cursor.shape != VtCursor::Hidden
        && (0..rows as i32).contains(&cursor_line))
    .then_some(Cursor {
        row: cursor_line as u16,
        col: content.cursor.point.column.0 as u16,
        shape: match content.cursor.shape {
            VtCursor::Beam => CursorShape::Beam,
            VtCursor::Underline => CursorShape::Underline,
            _ => CursorShape::Block,
        },
    });
    Frame {
        cols: cols as u16,
        rows: rows as u16,
        lines,
        cursor,
        title: title.to_owned(),
        display_offset: offset as u32,
        history: grid.history_size() as u32,
        bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
        alt_screen: mode.contains(TermMode::ALT_SCREEN),
        mouse: mode.intersects(TermMode::MOUSE_MODE),
    }
}

pub fn capture<T>(term: &Term<T>, scrollback: bool) -> String {
    let grid = term.grid();
    let start = if scrollback {
        -(grid.history_size() as i32)
    } else {
        0
    };
    let mut out: Vec<String> = Vec::new();
    for l in start..grid.screen_lines() as i32 {
        let row = &grid[Line(l)];
        let mut s = String::new();
        for c in 0..grid.columns() {
            let cell = &row[Column(c)];
            if !cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                push_cell(&mut s, cell);
            }
        }
        out.push(s.trim_end().to_owned());
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out.join("\n")
}
