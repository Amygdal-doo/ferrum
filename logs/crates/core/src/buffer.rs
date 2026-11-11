//! The bounded per-project line history and the byte → line splitter the
//! producers use.

use crate::line::LogLine;
use std::collections::VecDeque;

/// Lines kept per project before the oldest are dropped.
pub const DEFAULT_CAPACITY: usize = 200_000;

/// Ring buffer of lines with ever-increasing sequence numbers, so views
/// can refer to lines by `seq` while old ones fall off the front.
#[derive(Debug, Clone)]
pub struct LogBuffer {
    lines: VecDeque<LogLine>,
    capacity: usize,
    next_seq: u64,
    /// Lines dropped from the front because of `capacity`.
    pub dropped: u64,
}

impl LogBuffer {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            capacity: capacity.max(1),
            next_seq: 0,
            dropped: 0,
        }
    }

    /// Appends `line`, numbering it, and returns its `seq`.
    pub fn push(&mut self, mut line: LogLine) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        line.seq = seq;
        self.lines.push_back(line);
        while self.lines.len() > self.capacity {
            self.lines.pop_front();
            self.dropped += 1;
        }
        seq
    }

    /// Sequence number of the oldest line kept (== `end_seq` when empty).
    #[must_use]
    pub fn first_seq(&self) -> u64 {
        self.lines.front().map_or(self.next_seq, |l| l.seq)
    }

    /// The `seq` the next line will get.
    #[must_use]
    pub fn end_seq(&self) -> u64 {
        self.next_seq
    }

    #[must_use]
    pub fn get(&self, seq: u64) -> Option<&LogLine> {
        let index = seq.checked_sub(self.first_seq())?;
        self.lines.get(usize::try_from(index).ok()?)
    }

    /// Lines with `seq >= from`, oldest first.
    pub fn since(&self, from: u64) -> impl Iterator<Item = &LogLine> {
        let skip = usize::try_from(from.saturating_sub(self.first_seq())).unwrap_or(usize::MAX);
        self.lines.iter().skip(skip)
    }

    pub fn iter(&self) -> impl Iterator<Item = &LogLine> {
        self.lines.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Forgets every line; numbering continues.
    pub fn clear(&mut self) {
        self.lines.clear();
    }
}

/// Lines longer than this are cut so one runaway line can't eat memory.
pub const MAX_LINE_BYTES: usize = 64 * 1024;

/// Splits a byte stream into lines. Bytes are kept until a newline so
/// multi-byte characters split across reads decode correctly.
#[derive(Debug, Default, Clone)]
pub struct Splitter {
    partial: Vec<u8>,
}

impl Splitter {
    /// Feeds `bytes`, calling `emit` for each complete line (lossy UTF-8).
    pub fn feed(&mut self, bytes: &[u8], mut emit: impl FnMut(&str)) {
        let mut rest = bytes;
        while let Some(newline) = rest.iter().position(|&b| b == b'\n') {
            self.partial.extend_from_slice(&rest[..newline]);
            emit(&String::from_utf8_lossy(&self.partial));
            self.partial.clear();
            rest = &rest[newline + 1..];
        }
        self.partial.extend_from_slice(rest);
        if self.partial.len() > MAX_LINE_BYTES {
            emit(&String::from_utf8_lossy(&self.partial));
            self.partial.clear();
        }
    }

    /// Emits the unterminated tail, if any (the stream ended).
    pub fn finish(&mut self, mut emit: impl FnMut(&str)) {
        if !self.partial.is_empty() {
            emit(&String::from_utf8_lossy(&self.partial));
            self.partial.clear();
        }
    }

    /// Drops the unterminated tail (the file was truncated or replaced).
    pub fn reset(&mut self) {
        self.partial.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> LogLine {
        LogLine::system(0, text, None)
    }

    #[test]
    fn numbers_lines_and_drops_the_oldest() {
        let mut buffer = LogBuffer::new(2);
        assert_eq!(buffer.first_seq(), 0);
        for text in ["a", "b", "c"] {
            buffer.push(line(text));
        }
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.dropped, 1);
        assert_eq!(buffer.first_seq(), 1);
        assert_eq!(buffer.end_seq(), 3);
        assert!(buffer.get(0).is_none());
        assert_eq!(buffer.get(2).map(|l| l.text.as_str()), Some("c"));
        assert_eq!(buffer.since(2).count(), 1);
        assert_eq!(buffer.since(0).count(), 2);
        buffer.clear();
        assert!(buffer.is_empty());
        assert_eq!(buffer.push(line("d")), 3);
        assert_eq!(buffer.first_seq(), 3);
    }

    #[test]
    fn splits_bytes_into_lines() {
        let mut splitter = Splitter::default();
        let mut out = Vec::new();
        let euro = "€".as_bytes();
        splitter.feed(b"one\ntw", |l| out.push(l.to_owned()));
        splitter.feed(&euro[..1], |l| out.push(l.to_owned()));
        splitter.feed(&[&euro[1..], b"o\n\nthree"].concat(), |l| {
            out.push(l.to_owned())
        });
        assert_eq!(out, ["one", "tw€o", ""]);
        splitter.finish(|l| out.push(l.to_owned()));
        assert_eq!(out.last().map(String::as_str), Some("three"));
        splitter.finish(|l| out.push(l.to_owned()));
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn cuts_runaway_lines() {
        let mut splitter = Splitter::default();
        let mut count = 0;
        splitter.feed(&vec![b'x'; MAX_LINE_BYTES + 1], |_| count += 1);
        assert_eq!(count, 1);
    }
}
