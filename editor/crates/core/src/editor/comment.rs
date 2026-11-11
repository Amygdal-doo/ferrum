//! Toggle-comment change sets for line and block comment languages.

use std::ops::Range;

use super::Change;
use crate::buffer::Buffer;

fn indent_len(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

/// Comments every non-blank row at the block's smallest indent, or removes
/// the comment token (and one space) when all non-blank rows have one.
pub(crate) fn line_comment_changes(
    buffer: &Buffer,
    rows: Range<usize>,
    token: &str,
) -> Vec<Change> {
    let mut targets: Vec<usize> = rows.clone().filter(|r| !buffer.is_blank_line(*r)).collect();
    if targets.is_empty() {
        targets = rows.collect();
    }
    let commented = targets
        .iter()
        .all(|r| buffer.line(*r).trim_start().starts_with(token));
    if commented {
        return targets
            .into_iter()
            .map(|row| {
                let line = buffer.line(row);
                let at = buffer.line_start(row) + indent_len(&line);
                let after = &line[indent_len(&line) + token.len()..];
                let space = usize::from(after.starts_with(' '));
                Change::new(at..at + token.len() + space, "")
            })
            .collect();
    }
    let indent = targets
        .iter()
        .map(|r| indent_len(&buffer.line(*r)))
        .min()
        .unwrap_or(0);
    targets
        .into_iter()
        .map(|row| {
            let at = buffer.line_start(row) + indent.min(buffer.line_len(row));
            Change::insert(at, format!("{token} "))
        })
        .collect()
}

/// Wraps the block's non-blank rows in `open … close`, or unwraps them when
/// the first starts with `open` and the last ends with `close`.
pub(crate) fn block_comment_changes(
    buffer: &Buffer,
    rows: Range<usize>,
    open: &str,
    close: &str,
) -> Vec<Change> {
    let non_blank: Vec<usize> = rows.filter(|r| !buffer.is_blank_line(*r)).collect();
    let (Some(&first), Some(&last)) = (non_blank.first(), non_blank.last()) else {
        return Vec::new();
    };
    let first_line = buffer.line(first);
    let last_line = buffer.line(last);
    let first_at = buffer.line_start(first) + indent_len(&first_line);
    let trimmed_end = last_line.trim_end().len();
    let last_end = buffer.line_start(last) + trimmed_end;
    if first_line.trim_start().starts_with(open) && last_line.trim_end().ends_with(close) {
        let after_open = &first_line[indent_len(&first_line) + open.len()..];
        let open_len = open.len() + usize::from(after_open.starts_with(' '));
        let before_close = &last_line[..trimmed_end - close.len()];
        let close_len = close.len() + usize::from(before_close.ends_with(' '));
        return vec![
            Change::new(first_at..first_at + open_len, ""),
            Change::new(last_end - close_len..last_end, ""),
        ];
    }
    vec![
        Change::insert(first_at, format!("{open} ")),
        Change::insert(last_end, format!(" {close}")),
    ]
}
