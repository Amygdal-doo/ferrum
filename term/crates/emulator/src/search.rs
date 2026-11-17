//! Searching the screen and scrollback (`Find`). Rows joined by a soft wrap
//! are searched as one line, so a match can continue on the next row.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;
use ferrumterm_protocol::FindMatch;

/// Matches returned at most (`truncated` says there were more).
pub const MAX_MATCHES: usize = 5_000;

/// Where a character of a joined line is: line (from the oldest
/// scrollback line), cell column and width in cells.
#[derive(Clone, Copy)]
struct Place {
    byte: usize,
    line: u32,
    col: u16,
    width: u16,
}

/// The matches of `query` (a regex with `regex`, else plain text), oldest
/// first. An invalid regex is an error to show the user.
pub fn find<T>(
    term: &Term<T>,
    query: &str,
    case_sensitive: bool,
    regex: bool,
) -> Result<(Vec<FindMatch>, bool), String> {
    if query.is_empty() {
        return Ok((Vec::new(), false));
    }
    let pattern = if regex {
        query.to_owned()
    } else {
        regex::escape(query)
    };
    let re = regex::RegexBuilder::new(&pattern)
        .case_insensitive(!case_sensitive)
        .size_limit(1 << 20)
        .build()
        .map_err(|e| match e {
            regex::Error::Syntax(s) => s.lines().last().unwrap_or("invalid regex").to_owned(),
            other => other.to_string(),
        })?;
    let grid = term.grid();
    let history = grid.history_size();
    let mut out = Vec::new();
    let mut text = String::new();
    let mut places: Vec<Place> = Vec::new();
    let top = -(history as i32);
    let bottom = grid.screen_lines() as i32;
    for l in top..bottom {
        let row = &grid[Line(l)];
        let line = (l - top) as u32;
        for c in 0..grid.columns() {
            let cell = &row[Column(c)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let width = if cell.flags.contains(Flags::WIDE_CHAR) {
                2
            } else {
                1
            };
            places.push(Place {
                byte: text.len(),
                line,
                col: c as u16,
                width,
            });
            text.push(if cell.c == '\0' { ' ' } else { cell.c });
            for z in cell.zerowidth().unwrap_or_default() {
                text.push(*z);
            }
        }
        let wraps = row[Column(grid.columns() - 1)]
            .flags
            .contains(Flags::WRAPLINE);
        if wraps && l + 1 < bottom {
            continue;
        }
        // End of a line: its trailing blanks are not text.
        let end = text.trim_end_matches(' ').len();
        text.truncate(end);
        while places.last().is_some_and(|p| p.byte >= end) {
            places.pop();
        }
        if collect(&re, &text, &places, &mut out) {
            return Ok((out, true));
        }
        text.clear();
        places.clear();
    }
    Ok((out, false))
}

/// Adds the matches in one joined line; `true` once the limit is reached.
fn collect(re: &regex::Regex, text: &str, places: &[Place], out: &mut Vec<FindMatch>) -> bool {
    let at = |byte: usize| match places.binary_search_by_key(&byte, |p| p.byte) {
        Ok(i) => i,
        // Inside a character (a zero-width addition): the cell it is on.
        Err(i) => i.saturating_sub(1),
    };
    for m in re.find_iter(text) {
        if m.is_empty() {
            continue;
        }
        let first = places[at(m.start())];
        let last = places[at(m.end() - 1)];
        out.push(FindMatch {
            line: first.line,
            col: first.col,
            end_line: last.line,
            end_col: last.col + last.width,
        });
        if out.len() >= MAX_MATCHES {
            return true;
        }
    }
    false
}
