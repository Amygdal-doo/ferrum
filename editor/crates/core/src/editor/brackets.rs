//! Matching-bracket lookup for the bracket next to the caret.

use std::ops::Range;

use crate::buffer::Buffer;

const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];
/// Chars scanned at most when looking for a match.
const SCAN_LIMIT: usize = 100_000;

/// The bracket at `offset` (or just before it) and its partner, as byte
/// ranges `(bracket, partner)`.
pub fn matching_bracket(buffer: &Buffer, offset: usize) -> Option<(Range<usize>, Range<usize>)> {
    let candidates = [
        Some(offset),
        offset
            .checked_sub(1)
            .map(|_| buffer.prev_char_offset(offset)),
    ];
    for at in candidates.into_iter().flatten() {
        let Some(c) = buffer.char_at(at) else {
            continue;
        };
        if let Some(&(open, close)) = PAIRS.iter().find(|(o, cl)| *o == c || *cl == c) {
            let forward = c == open;
            if let Some(partner) = scan(buffer, at, open, close, forward) {
                return Some((at..at + 1, partner..partner + 1));
            }
        }
    }
    None
}

fn scan(buffer: &Buffer, at: usize, open: char, close: char, forward: bool) -> Option<usize> {
    let rope = buffer.rope();
    let start_char = rope.byte_to_char(at);
    let mut depth = 0usize;
    if forward {
        for (i, c) in rope.chars_at(start_char).enumerate().take(SCAN_LIMIT) {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some(rope.char_to_byte(start_char + i));
                }
            }
        }
    } else {
        let mut chars = rope.chars_at(start_char + 1);
        let mut index = start_char + 1;
        for _ in 0..SCAN_LIMIT {
            let c = chars.prev()?;
            index -= 1;
            if c == close {
                depth += 1;
            } else if c == open {
                depth -= 1;
                if depth == 0 {
                    return Some(rope.char_to_byte(index));
                }
            }
        }
    }
    None
}
