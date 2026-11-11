//! What the log view shows: level / source / text filters, the filtered
//! list of line numbers (updated incrementally) and match highlighting.

use crate::ansi::AnsiStyle;
use crate::buffer::LogBuffer;
use crate::level::Level;
use crate::line::LogLine;
use regex::{Regex, RegexBuilder};
use std::collections::{HashSet, VecDeque};
use std::ops::Range;

/// Level toggles as the toolbar shows them. Trace counts as debug; lines
/// without a level follow the info toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Levels {
    pub error: bool,
    pub warn: bool,
    pub info: bool,
    pub debug: bool,
}

impl Default for Levels {
    fn default() -> Self {
        Self {
            error: true,
            warn: true,
            info: true,
            debug: true,
        }
    }
}

impl Levels {
    pub const ERRORS_ONLY: Levels = Levels {
        error: true,
        warn: false,
        info: false,
        debug: false,
    };

    #[must_use]
    pub fn allows(&self, level: Option<Level>) -> bool {
        match level {
            Some(Level::Error) => self.error,
            Some(Level::Warn) => self.warn,
            Some(Level::Info) | None => self.info,
            Some(Level::Debug | Level::Trace) => self.debug,
        }
    }

    /// The toggle for `level` (trace shares debug's).
    pub fn toggle(&mut self, level: Level) {
        let flag = match level {
            Level::Error => &mut self.error,
            Level::Warn => &mut self.warn,
            Level::Info => &mut self.info,
            Level::Debug | Level::Trace => &mut self.debug,
        };
        *flag = !*flag;
    }

    #[must_use]
    pub fn get(&self, level: Level) -> bool {
        self.allows(Some(level))
    }
}

/// The user's filter settings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    pub levels: Levels,
    pub query: String,
    pub regex: bool,
    pub case_sensitive: bool,
    /// Source slots switched off in the sidebar.
    pub hidden: HashSet<u32>,
}

/// A compiled [`Filter`].
#[derive(Debug, Clone)]
pub struct Matcher {
    levels: Levels,
    hidden: HashSet<u32>,
    pattern: Option<Regex>,
}

impl Filter {
    /// Compiles the filter; an invalid regex is an error message.
    pub fn compile(&self) -> Result<Matcher, String> {
        let query = self.query.trim();
        let pattern = if query.is_empty() {
            None
        } else {
            let source = if self.regex {
                query.to_owned()
            } else {
                regex::escape(query)
            };
            let regex = RegexBuilder::new(&source)
                .case_insensitive(!self.case_sensitive)
                .size_limit(1 << 20)
                .build()
                .map_err(|e| match e {
                    regex::Error::Syntax(s) => s.lines().last().unwrap_or("").trim().to_owned(),
                    other => other.to_string(),
                })?;
            Some(regex)
        };
        Ok(Matcher {
            levels: self.levels,
            hidden: self.hidden.clone(),
            pattern,
        })
    }
}

impl Matcher {
    /// Shows everything.
    #[must_use]
    pub fn all() -> Self {
        Self {
            levels: Levels::default(),
            hidden: HashSet::new(),
            pattern: None,
        }
    }

    #[must_use]
    pub fn matches(&self, line: &LogLine) -> bool {
        if self.hidden.contains(&line.source) || !self.levels.allows(line.level) {
            return false;
        }
        let Some(pattern) = &self.pattern else {
            return true;
        };
        pattern.is_match(&line.text)
            || line.tag.as_deref().is_some_and(|t| pattern.is_match(t))
            || line
                .fields
                .iter()
                .any(|(k, v)| pattern.is_match(k) || pattern.is_match(v))
    }

    /// Byte ranges of `text` the query matches (for highlighting).
    #[must_use]
    pub fn highlights(&self, text: &str) -> Vec<Range<usize>> {
        let Some(pattern) = &self.pattern else {
            return Vec::new();
        };
        pattern
            .find_iter(text)
            .filter(|m| !m.is_empty())
            .take(64)
            .map(|m| m.range())
            .collect()
    }

    #[must_use]
    pub fn has_query(&self) -> bool {
        self.pattern.is_some()
    }
}

/// The `seq`s of the lines that pass the filter, in order.
#[derive(Debug, Clone, Default)]
pub struct View {
    seqs: VecDeque<u64>,
    /// Lines up to (not including) this `seq` have been considered.
    scanned: u64,
}

impl View {
    /// Rebuilds from the whole buffer.
    pub fn rebuild(&mut self, buffer: &LogBuffer, matcher: &Matcher) {
        self.seqs = buffer
            .iter()
            .filter(|line| matcher.matches(line))
            .map(|line| line.seq)
            .collect();
        self.scanned = buffer.end_seq();
    }

    /// Takes in lines added since the last call and forgets dropped ones;
    /// returns how many visible lines were added.
    pub fn update(&mut self, buffer: &LogBuffer, matcher: &Matcher) -> usize {
        let first = buffer.first_seq();
        while self.seqs.front().is_some_and(|&seq| seq < first) {
            self.seqs.pop_front();
        }
        let before = self.seqs.len();
        for line in buffer.since(self.scanned) {
            if matcher.matches(line) {
                self.seqs.push_back(line.seq);
            }
        }
        self.scanned = buffer.end_seq();
        self.seqs.len() - before
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.seqs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.seqs.is_empty()
    }

    /// The `seq` of visible row `row`.
    #[must_use]
    pub fn seq(&self, row: usize) -> Option<u64> {
        self.seqs.get(row).copied()
    }

    /// The row showing `seq`, if visible.
    #[must_use]
    pub fn row_of(&self, seq: u64) -> Option<usize> {
        self.seqs.binary_search(&seq).ok()
    }

    pub fn seqs(&self) -> impl Iterator<Item = u64> + '_ {
        self.seqs.iter().copied()
    }

    pub fn clear(&mut self, buffer: &LogBuffer) {
        self.seqs.clear();
        self.scanned = buffer.end_seq();
    }
}

/// A run of text with its ANSI style and whether the search matched it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub range: Range<usize>,
    pub style: Option<AnsiStyle>,
    pub matched: bool,
}

/// Splits `0..len` at every style and match boundary so the UI can give
/// each run one combined style. Both inputs must be sorted, non-overlapping.
#[must_use]
pub fn segments(
    len: usize,
    styles: &[(Range<usize>, AnsiStyle)],
    matches: &[Range<usize>],
) -> Vec<Segment> {
    let mut cuts: Vec<usize> = vec![0, len];
    for (range, _) in styles {
        cuts.extend([range.start.min(len), range.end.min(len)]);
    }
    for range in matches {
        cuts.extend([range.start.min(len), range.end.min(len)]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .map(|w| {
            let range = w[0]..w[1];
            let style = styles
                .iter()
                .find(|(r, _)| r.start <= range.start && range.end <= r.end)
                .map(|(_, s)| *s);
            let matched = matches
                .iter()
                .any(|r| r.start <= range.start && range.end <= r.end);
            Segment {
                range,
                style,
                matched,
            }
        })
        .filter(|s| s.style.is_some() || s.matched)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::line::{ParseOptions, Stream, parse};

    fn buffer(lines: &[(u32, &str)]) -> LogBuffer {
        let mut buffer = LogBuffer::new(100);
        for (source, text) in lines {
            buffer.push(parse(
                *source,
                Stream::Stdout,
                text,
                ParseOptions::default(),
            ));
        }
        buffer
    }

    #[test]
    fn levels_sources_and_text() {
        let buffer = buffer(&[
            (0, "INFO started"),
            (0, "ERROR db down"),
            (1, "WARN slow DB"),
            (1, "plain line"),
            (0, "DEBUG x"),
        ]);
        let count = |filter: &Filter| {
            let mut view = View::default();
            view.rebuild(&buffer, &filter.compile().expect("valid"));
            view.len()
        };
        let mut filter = Filter::default();
        assert_eq!(count(&filter), 5);
        filter.levels = Levels::ERRORS_ONLY;
        assert_eq!(count(&filter), 1);
        filter.levels = Levels::default();
        filter.levels.toggle(Level::Info);
        assert_eq!(count(&filter), 3, "unlevelled lines follow info");
        filter.levels = Levels::default();
        filter.hidden.insert(0);
        assert_eq!(count(&filter), 2);
        filter.hidden.clear();
        filter.query = "db".into();
        assert_eq!(count(&filter), 2, "case-insensitive");
        filter.case_sensitive = true;
        assert_eq!(count(&filter), 1);
        filter.case_sensitive = false;
        filter.query = "d.*n".into();
        assert_eq!(count(&filter), 0, "plain text by default");
        filter.regex = true;
        assert_eq!(count(&filter), 1);
        filter.query = "(".into();
        assert!(filter.compile().is_err());
    }

    #[test]
    fn json_fields_are_searchable() {
        let buffer = buffer(&[(0, r#"{"msg":"hi","user":"ada"}"#)]);
        let filter = Filter {
            query: "ada".into(),
            ..Filter::default()
        };
        let mut view = View::default();
        view.rebuild(&buffer, &filter.compile().expect("valid"));
        assert_eq!(view.len(), 1);
    }

    #[test]
    fn incremental_updates_follow_the_ring_buffer() {
        let mut buffer = LogBuffer::new(3);
        let matcher = Filter {
            query: "x".into(),
            ..Filter::default()
        }
        .compile()
        .expect("valid");
        let mut view = View::default();
        view.rebuild(&buffer, &matcher);
        for text in ["x1", "y", "x2"] {
            buffer.push(parse(0, Stream::Stdout, text, ParseOptions::default()));
        }
        assert_eq!(view.update(&buffer, &matcher), 2);
        assert_eq!(view.seqs().collect::<Vec<_>>(), [0, 2]);
        for text in ["x3", "y"] {
            buffer.push(parse(0, Stream::Stdout, text, ParseOptions::default()));
        }
        assert_eq!(view.update(&buffer, &matcher), 1);
        assert_eq!(view.seqs().collect::<Vec<_>>(), [2, 3], "seq 0 fell off");
        assert_eq!(view.row_of(3), Some(1));
        assert_eq!(view.seq(0), Some(2));
        view.clear(&buffer);
        assert!(view.is_empty());
        assert_eq!(view.update(&buffer, &matcher), 0);
    }

    #[test]
    fn stays_bounded_with_a_fast_producer() {
        use crate::buffer::DEFAULT_CAPACITY;
        let mut buffer = LogBuffer::new(DEFAULT_CAPACITY);
        let line = parse(
            0,
            Stream::Stdout,
            "INFO served in 3ms",
            ParseOptions::default(),
        );
        let matcher = Filter {
            query: "served".into(),
            ..Filter::default()
        }
        .compile()
        .expect("valid");
        let mut view = View::default();
        for _ in 0..25 {
            for _ in 0..10_000 {
                buffer.push(line.clone());
            }
            view.update(&buffer, &matcher);
        }
        assert_eq!(buffer.len(), DEFAULT_CAPACITY);
        assert_eq!(buffer.dropped, 50_000);
        assert_eq!(view.len(), DEFAULT_CAPACITY);
        assert_eq!(view.seq(0), Some(50_000));
    }

    #[test]
    fn highlights_and_segments() {
        let matcher = Filter {
            query: "ab".into(),
            ..Filter::default()
        }
        .compile()
        .expect("valid");
        assert_eq!(matcher.highlights("xAByab"), [1..3, 4..6]);
        assert!(Matcher::all().highlights("ab").is_empty());
        let red = AnsiStyle {
            bold: true,
            ..AnsiStyle::default()
        };
        let segs = segments(6, &[(0..2, red)], std::slice::from_ref(&(1..3)));
        assert_eq!(
            segs,
            [
                Segment {
                    range: 0..1,
                    style: Some(red),
                    matched: false
                },
                Segment {
                    range: 1..2,
                    style: Some(red),
                    matched: true
                },
                Segment {
                    range: 2..3,
                    style: None,
                    matched: true
                },
            ]
        );
    }
}
