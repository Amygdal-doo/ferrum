//! Job logs: timestamps split off, ANSI parsed, `##[group]` sections
//! folded, `##[error]` / `##[warning]` lines marked, and search.

use crate::ansi::{self, Span};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Plain,
    /// The title line of a `##[group]`.
    Group,
    Error,
    Warning,
    Notice,
    /// `##[command]` (the shell command a step runs).
    Command,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// 1-based, counting the lines shown (markers like `##[endgroup]` excluded).
    pub number: usize,
    /// `2024-05-01T10:00:00.1234567Z`, when the line had one.
    pub timestamp: Option<String>,
    pub kind: LineKind,
    /// Text without escapes or the `##[…]` marker.
    pub text: String,
    pub spans: Vec<Span>,
    /// Index into [`JobLog::groups`] of the group this line is inside
    /// (not set on the group's own title line).
    pub group: Option<usize>,
}

/// A foldable section: its title line and the lines inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Group {
    pub header: usize,
    /// Lines `header + 1 .. end` are inside.
    pub end: usize,
    pub has_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JobLog {
    pub lines: Vec<LogLine>,
    pub groups: Vec<Group>,
}

/// Splits a leading `2024-05-01T10:00:00.1234567Z ` timestamp off.
fn split_timestamp(line: &str) -> (Option<&str>, &str) {
    if let Some((head, rest)) = line.split_once(' ') {
        let b = head.as_bytes();
        if (20..=34).contains(&b.len())
            && b[4] == b'-'
            && b[10] == b'T'
            && b.ends_with(b"Z")
            && b[0].is_ascii_digit()
        {
            return (Some(head), rest);
        }
    }
    (None, line)
}

const MARKERS: [(&str, LineKind); 6] = [
    ("##[group]", LineKind::Group),
    ("##[error]", LineKind::Error),
    ("##[warning]", LineKind::Warning),
    ("##[notice]", LineKind::Notice),
    ("##[command]", LineKind::Command),
    ("##[debug]", LineKind::Debug),
];

/// Compiler-style errors that tools print without a `##[error]` marker.
fn looks_like_error(text: &str) -> bool {
    let t = text.trim_start();
    let lower: String = t.chars().take(8).collect::<String>().to_lowercase();
    lower.starts_with("error:")
        || lower.starts_with("error[")
        || t.starts_with("Error: ")
        || t.starts_with("FAILED")
        || t.contains("Process completed with exit code")
}

impl JobLog {
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        let mut log = Self::default();
        let mut open: Option<usize> = None;
        let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
        for raw_line in raw.lines() {
            let (timestamp, rest) = split_timestamp(raw_line.trim_end_matches('\r'));
            let rest = rest.strip_prefix('\u{feff}').unwrap_or(rest);
            if rest.starts_with("##[endgroup]") {
                if let Some(g) = open.take() {
                    log.groups[g].end = log.lines.len();
                }
                continue;
            }
            let (kind, body) = MARKERS
                .iter()
                .find_map(|(m, k)| rest.strip_prefix(m).map(|b| (*k, b)))
                .unwrap_or((LineKind::Plain, rest));
            let spans = ansi::parse(body);
            let text: String = spans.iter().map(|s| s.text.as_str()).collect();
            let kind = if kind == LineKind::Plain && looks_like_error(&text) {
                LineKind::Error
            } else {
                kind
            };
            let index = log.lines.len();
            let mut group = open;
            if kind == LineKind::Group {
                if let Some(g) = open.take() {
                    log.groups[g].end = index;
                }
                log.groups.push(Group {
                    header: index,
                    end: index + 1,
                    has_error: false,
                });
                open = Some(log.groups.len() - 1);
                group = None;
            } else if let Some(g) = open {
                log.groups[g].end = index + 1;
            }
            if kind == LineKind::Error
                && let Some(g) = group
            {
                log.groups[g].has_error = true;
            }
            log.lines.push(LogLine {
                number: index + 1,
                timestamp: timestamp.map(str::to_owned),
                kind,
                text,
                spans,
                group,
            });
        }
        log
    }

    /// Groups folded by default: all but those with errors.
    #[must_use]
    pub fn default_collapsed(&self) -> HashSet<usize> {
        (0..self.groups.len())
            .filter(|g| !self.groups[*g].has_error)
            .collect()
    }

    /// Indices of the lines to draw with the groups in `collapsed` folded.
    #[must_use]
    pub fn visible(&self, collapsed: &HashSet<usize>) -> Vec<usize> {
        self.lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.group.is_none_or(|g| !collapsed.contains(&g)))
            .map(|(i, _)| i)
            .collect()
    }

    /// The group whose title is line `index`.
    #[must_use]
    pub fn group_at(&self, index: usize) -> Option<usize> {
        self.groups.iter().position(|g| g.header == index)
    }

    /// Lines containing `query` (case-insensitive).
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<usize> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        self.lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.text.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }

    /// Unfolds the group holding line `index`, if any.
    pub fn reveal(&self, index: usize, collapsed: &mut HashSet<usize>) {
        if let Some(g) = self.lines.get(index).and_then(|l| l.group) {
            collapsed.remove(&g);
        }
    }

    #[must_use]
    pub fn errors(&self) -> Vec<usize> {
        self.lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.kind == LineKind::Error)
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "\u{feff}2024-05-01T10:00:00.1000000Z ##[group]Run actions/checkout@v4\n\
2024-05-01T10:00:00.2000000Z with:\n\
2024-05-01T10:00:00.3000000Z   fetch-depth: 1\n\
2024-05-01T10:00:00.4000000Z ##[endgroup]\n\
2024-05-01T10:00:01.0000000Z ##[group]Run cargo test\n\
2024-05-01T10:00:01.1000000Z ##[command]cargo test\n\
2024-05-01T10:00:02.0000000Z \u{1b}[1m\u{1b}[91merror[E0425]\u{1b}[0m: cannot find value `x`\n\
2024-05-01T10:00:03.0000000Z ##[endgroup]\n\
2024-05-01T10:00:04.0000000Z ##[error]Process completed with exit code 101.\n\
no timestamp here\n";

    #[test]
    fn parses_groups_markers_and_timestamps() {
        let log = JobLog::parse(LOG);
        assert_eq!(log.lines.len(), 8);
        assert_eq!(log.lines[0].kind, LineKind::Group);
        assert_eq!(log.lines[0].text, "Run actions/checkout@v4");
        assert_eq!(
            log.lines[0].timestamp.as_deref(),
            Some("2024-05-01T10:00:00.1000000Z")
        );
        assert_eq!(log.lines[1].group, Some(0));
        assert_eq!(log.lines[4].kind, LineKind::Command);
        assert_eq!(log.lines[5].kind, LineKind::Error);
        assert_eq!(log.lines[5].text, "error[E0425]: cannot find value `x`");
        assert_eq!(log.lines[6].kind, LineKind::Error);
        assert_eq!(log.lines[6].group, None);
        assert_eq!(log.lines[7].number, 8);
        assert_eq!(log.lines[7].kind, LineKind::Plain);
        assert_eq!(
            log.groups,
            vec![
                Group {
                    header: 0,
                    end: 3,
                    has_error: false
                },
                Group {
                    header: 3,
                    end: 6,
                    has_error: true
                },
            ]
        );
        let last = JobLog::parse("no timestamp here");
        assert_eq!(last.lines[0].timestamp, None);
    }

    #[test]
    fn folding_and_search() {
        let log = JobLog::parse(LOG);
        let mut collapsed = log.default_collapsed();
        assert_eq!(collapsed, HashSet::from([0]));
        assert_eq!(log.visible(&collapsed), vec![0, 3, 4, 5, 6, 7]);
        let hits = log.search("FETCH-depth");
        assert_eq!(hits, vec![2]);
        log.reveal(hits[0], &mut collapsed);
        assert!(collapsed.is_empty());
        assert_eq!(log.visible(&collapsed).len(), 8);
        assert!(log.search("  ").is_empty());
        assert_eq!(log.group_at(3), Some(1));
        assert_eq!(log.errors(), vec![5, 6]);
    }

    #[test]
    fn unclosed_group_runs_to_the_next() {
        let log = JobLog::parse("##[group]A\na1\n##[group]B\nb1\nb2");
        assert_eq!(log.groups[0].end, 2);
        assert_eq!(log.groups[1].end, 5);
        assert_eq!(log.lines[4].group, Some(1));
    }
}
