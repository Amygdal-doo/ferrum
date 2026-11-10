//! Line-based text diff.

use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Same,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    /// 1-based line number in the left text (`None` for added lines).
    pub old_line: Option<usize>,
    /// 1-based line number in the right text (`None` for removed lines).
    pub new_line: Option<usize>,
    /// Line content without its line ending.
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffStats {
    pub added: usize,
    pub removed: usize,
    pub same: usize,
}

impl DiffStats {
    pub fn identical(&self) -> bool {
        self.added == 0 && self.removed == 0
    }
}

pub fn diff_lines(old: &str, new: &str) -> (Vec<DiffLine>, DiffStats) {
    let diff = TextDiff::from_lines(old, new);
    let mut stats = DiffStats::default();
    let lines = diff
        .iter_all_changes()
        .map(|change| {
            let kind = match change.tag() {
                ChangeTag::Equal => {
                    stats.same += 1;
                    LineKind::Same
                }
                ChangeTag::Insert => {
                    stats.added += 1;
                    LineKind::Added
                }
                ChangeTag::Delete => {
                    stats.removed += 1;
                    LineKind::Removed
                }
            };
            DiffLine {
                kind,
                old_line: change.old_index().map(|i| i + 1),
                new_line: change.new_index().map(|i| i + 1),
                text: change
                    .value()
                    .trim_end_matches('\n')
                    .trim_end_matches('\r')
                    .to_string(),
            }
        })
        .collect();
    (lines, stats)
}

/// The diff as unified-style text (`+`/`-`/space prefixes, no hunks).
pub fn to_text(lines: &[DiffLine]) -> String {
    lines
        .iter()
        .map(|line| {
            let prefix = match line.kind {
                LineKind::Same => ' ',
                LineKind::Added => '+',
                LineKind::Removed => '-',
            };
            format!("{prefix} {}", line.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_texts() {
        let (lines, stats) = diff_lines("a\nb\n", "a\nb\n");
        assert!(stats.identical());
        assert_eq!(stats.same, 2);
        assert!(lines.iter().all(|l| l.kind == LineKind::Same));
    }

    #[test]
    fn changed_line_is_removed_then_added() {
        let (lines, stats) = diff_lines("a\nb\nc", "a\nB\nc");
        assert_eq!((stats.added, stats.removed, stats.same), (1, 1, 2));
        assert_eq!(lines[1].kind, LineKind::Removed);
        assert_eq!(lines[1].text, "b");
        assert_eq!(lines[1].old_line, Some(2));
        assert_eq!(lines[1].new_line, None);
        assert_eq!(lines[2].kind, LineKind::Added);
        assert_eq!(lines[2].new_line, Some(2));
        assert_eq!(to_text(&lines), "  a\n- b\n+ B\n  c");
    }

    #[test]
    fn empty_sides() {
        let (lines, stats) = diff_lines("", "x\ny");
        assert_eq!(stats.added, 2);
        assert_eq!(lines.len(), 2);
        let (_, stats) = diff_lines("x", "");
        assert_eq!(stats.removed, 1);
        let (lines, stats) = diff_lines("", "");
        assert!(lines.is_empty());
        assert!(stats.identical());
    }

    #[test]
    fn crlf_is_stripped_from_text() {
        let (lines, _) = diff_lines("a\r\nb\r\n", "a\r\nc\r\n");
        assert_eq!(lines[0].text, "a");
        assert!(lines.iter().all(|l| !l.text.ends_with('\r')));
    }

    #[test]
    fn insertion_keeps_numbers_aligned() {
        let (lines, _) = diff_lines("a\nc\n", "a\nb\nc\n");
        let c = lines.iter().find(|l| l.text == "c").unwrap();
        assert_eq!((c.old_line, c.new_line), (Some(2), Some(3)));
    }
}
