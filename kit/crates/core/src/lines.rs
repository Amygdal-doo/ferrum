//! Line operations: sort, dedupe, trim, and counts.

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineOp {
    SortAsc,
    SortDesc,
    /// Case-insensitive ascending sort.
    SortNatural,
    Reverse,
    Dedupe,
    Trim,
    RemoveEmpty,
    Shuffle,
}

impl LineOp {
    pub const ALL: [LineOp; 8] = [
        LineOp::SortAsc,
        LineOp::SortDesc,
        LineOp::SortNatural,
        LineOp::Reverse,
        LineOp::Dedupe,
        LineOp::Trim,
        LineOp::RemoveEmpty,
        LineOp::Shuffle,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LineOp::SortAsc => "Sort A→Z",
            LineOp::SortDesc => "Sort Z→A",
            LineOp::SortNatural => "Sort (ignore case)",
            LineOp::Reverse => "Reverse",
            LineOp::Dedupe => "Remove duplicates",
            LineOp::Trim => "Trim whitespace",
            LineOp::RemoveEmpty => "Remove empty lines",
            LineOp::Shuffle => "Shuffle",
        }
    }
}

/// Applies `op` to the lines of `text` (`\n` or `\r\n`), joining with `\n`.
/// `Shuffle` uses `seed` so results are reproducible in tests.
pub fn apply(text: &str, op: LineOp, seed: u64) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    match op {
        LineOp::SortAsc => lines.sort_unstable(),
        LineOp::SortDesc => lines.sort_unstable_by(|a, b| b.cmp(a)),
        LineOp::SortNatural => lines.sort_by_cached_key(|l| l.to_lowercase()),
        LineOp::Reverse => lines.reverse(),
        LineOp::Dedupe => {
            let mut seen = HashSet::new();
            lines.retain(|line| seen.insert(*line));
        }
        LineOp::Trim => {
            return lines
                .iter()
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join("\n");
        }
        LineOp::RemoveEmpty => lines.retain(|line| !line.trim().is_empty()),
        LineOp::Shuffle => shuffle(&mut lines, seed),
    }
    lines.join("\n")
}

/// Fisher–Yates with a small xorshift generator (no `rand` dependency).
fn shuffle<T>(items: &mut [T], seed: u64) {
    let mut state = seed | 1;
    for i in (1..items.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextStats {
    pub lines: usize,
    pub non_empty_lines: usize,
    pub unique_lines: usize,
    pub words: usize,
    pub chars: usize,
    pub bytes: usize,
}

pub fn stats(text: &str) -> TextStats {
    let lines: Vec<&str> = text.lines().collect();
    TextStats {
        lines: lines.len(),
        non_empty_lines: lines.iter().filter(|l| !l.trim().is_empty()).count(),
        unique_lines: lines.iter().collect::<HashSet<_>>().len(),
        words: text.split_whitespace().count(),
        chars: text.chars().count(),
        bytes: text.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts() {
        assert_eq!(apply("b\nA\na", LineOp::SortAsc, 0), "A\na\nb");
        assert_eq!(apply("b\nA\na", LineOp::SortDesc, 0), "b\na\nA");
        assert_eq!(apply("b\nC\na", LineOp::SortNatural, 0), "a\nb\nC");
        assert_eq!(apply("1\n2\n3", LineOp::Reverse, 0), "3\n2\n1");
    }

    #[test]
    fn dedupes_keeping_first_occurrence() {
        assert_eq!(apply("b\na\nb\na\nc", LineOp::Dedupe, 0), "b\na\nc");
        // Case and whitespace matter.
        assert_eq!(apply("a\nA\na ", LineOp::Dedupe, 0), "a\nA\na ");
    }

    #[test]
    fn trims_and_removes_empty() {
        assert_eq!(apply("  a \n\tb\n", LineOp::Trim, 0), "a\nb");
        assert_eq!(apply("a\n\n  \nb", LineOp::RemoveEmpty, 0), "a\nb");
    }

    #[test]
    fn handles_crlf_and_empty_text() {
        assert_eq!(apply("b\r\na\r\n", LineOp::SortAsc, 0), "a\nb");
        assert_eq!(apply("", LineOp::SortAsc, 0), "");
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let input = "1\n2\n3\n4\n5\n6";
        let out = apply(input, LineOp::Shuffle, 42);
        let mut sorted: Vec<&str> = out.lines().collect();
        sorted.sort_unstable();
        assert_eq!(sorted.join("\n"), input);
        assert_eq!(out, apply(input, LineOp::Shuffle, 42));
    }

    #[test]
    fn counts() {
        let s = stats("hello world\n\nhello world\nné");
        assert_eq!(s.lines, 4);
        assert_eq!(s.non_empty_lines, 3);
        assert_eq!(s.unique_lines, 3);
        assert_eq!(s.words, 5);
        assert_eq!(s.chars, 27);
        assert_eq!(s.bytes, 28);
        assert_eq!(stats(""), TextStats::default());
    }
}
