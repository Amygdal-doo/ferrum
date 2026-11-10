//! Log buffering: turns raw chunks into lines, splits engine timestamps off,
//! keeps a bounded history and searches it.

use crate::ansi;
use crate::model::{LogChunk, LogStream};
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub stream: LogStream,
    /// RFC 3339 timestamp from the engine, when requested.
    pub timestamp: Option<String>,
    /// The line as the program wrote it (may contain ANSI escapes).
    pub raw: String,
    /// `raw` without escapes; what search matches against.
    pub plain: String,
}

/// Bounded line buffer fed with [`LogChunk`]s.
#[derive(Debug, Clone)]
pub struct LogBuffer {
    lines: VecDeque<LogLine>,
    /// Unterminated tail per stream (stdout, stderr).
    partial: [String; 2],
    capacity: usize,
    has_timestamps: bool,
    /// Lines dropped from the front because of `capacity`.
    pub dropped: usize,
}

impl LogBuffer {
    /// `has_timestamps`: the engine was asked for timestamps (`timestamps=true`).
    #[must_use]
    pub fn new(capacity: usize, has_timestamps: bool) -> Self {
        Self {
            lines: VecDeque::new(),
            partial: [String::new(), String::new()],
            capacity: capacity.max(1),
            has_timestamps,
            dropped: 0,
        }
    }

    pub fn push(&mut self, chunk: &LogChunk) {
        let slot = match chunk.stream {
            LogStream::Stdout => 0,
            LogStream::Stderr => 1,
        };
        let mut pending = std::mem::take(&mut self.partial[slot]);
        pending.push_str(&chunk.text);
        let mut rest = pending.as_str();
        while let Some(newline) = rest.find('\n') {
            let line = &rest[..newline];
            self.push_line(chunk.stream, line);
            rest = &rest[newline + 1..];
        }
        self.partial[slot] = rest.to_owned();
    }

    /// Flushes unterminated lines (the stream ended).
    pub fn finish(&mut self) {
        for (slot, stream) in [(0, LogStream::Stdout), (1, LogStream::Stderr)] {
            let line = std::mem::take(&mut self.partial[slot]);
            if !line.is_empty() {
                self.push_line(stream, &line);
            }
        }
    }

    fn push_line(&mut self, stream: LogStream, line: &str) {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let (timestamp, raw) = if self.has_timestamps {
            split_timestamp(line)
        } else {
            (None, line)
        };
        let plain = ansi::strip(raw);
        self.lines.push_back(LogLine {
            stream,
            timestamp: timestamp.map(str::to_owned),
            raw: raw.to_owned(),
            plain,
        });
        while self.lines.len() > self.capacity {
            self.lines.pop_front();
            self.dropped += 1;
        }
    }

    #[must_use]
    pub fn lines(&self) -> &VecDeque<LogLine> {
        &self.lines
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Indexes of lines containing `query` (case-insensitive); all lines when empty.
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<usize> {
        let query = query.trim();
        if query.is_empty() {
            return (0..self.lines.len()).collect();
        }
        let needle = query.to_lowercase();
        self.lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.plain.to_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect()
    }
}

/// Splits Docker's `2024-05-01T10:00:00.123456789Z ` prefix off a line.
#[must_use]
pub fn split_timestamp(line: &str) -> (Option<&str>, &str) {
    let Some((head, rest)) = line.split_once(' ') else {
        return if looks_like_timestamp(line) {
            (Some(line), "")
        } else {
            (None, line)
        };
    };
    if looks_like_timestamp(head) {
        (Some(head), rest)
    } else {
        (None, line)
    }
}

fn looks_like_timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 20
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && (s.ends_with('Z') || s.get(19..).is_some_and(|t| t.contains(['+', '-'])))
}

/// `2024-05-01T10:00:00.123456789Z` → `2024-05-01 12:00:00.123` in local time.
#[must_use]
pub fn display_timestamp(ts: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(ts).map_or_else(
        |_| ts.to_owned(),
        |t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S%.3f")
                .to_string()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(stream: LogStream, text: &str) -> LogChunk {
        LogChunk {
            stream,
            text: text.to_owned(),
        }
    }

    #[test]
    fn joins_partial_chunks_per_stream() {
        let mut buffer = LogBuffer::new(100, false);
        buffer.push(&chunk(LogStream::Stdout, "hel"));
        buffer.push(&chunk(LogStream::Stderr, "oops\n"));
        buffer.push(&chunk(LogStream::Stdout, "lo\nwor"));
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.lines()[0].raw, "oops");
        assert_eq!(buffer.lines()[0].stream, LogStream::Stderr);
        assert_eq!(buffer.lines()[1].raw, "hello");
        buffer.finish();
        assert_eq!(buffer.lines()[2].raw, "wor");
        buffer.finish();
        assert_eq!(buffer.len(), 3);
    }

    #[test]
    fn splits_timestamps_and_strips_ansi_for_search() {
        let mut buffer = LogBuffer::new(100, true);
        buffer.push(&chunk(
            LogStream::Stdout,
            "2024-05-01T10:00:00.123456789Z \u{1b}[32mGET /health\u{1b}[0m 200\r\n2024-05-01T10:00:01Z ERROR db down\n",
        ));
        let lines = buffer.lines();
        assert_eq!(
            lines[0].timestamp.as_deref(),
            Some("2024-05-01T10:00:00.123456789Z")
        );
        assert_eq!(lines[0].plain, "GET /health 200");
        assert!(lines[0].raw.contains('\u{1b}'));
        assert_eq!(buffer.search("error"), [1]);
        assert_eq!(buffer.search("  "), [0, 1]);
        assert_eq!(
            buffer.search("32m"),
            Vec::<usize>::new(),
            "escapes are not searchable"
        );
    }

    #[test]
    fn drops_oldest_lines_beyond_capacity() {
        let mut buffer = LogBuffer::new(2, false);
        buffer.push(&chunk(LogStream::Stdout, "a\nb\nc\n"));
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.dropped, 1);
        assert_eq!(buffer.lines()[0].raw, "b");
    }

    #[test]
    fn recognises_only_real_timestamps() {
        assert_eq!(
            split_timestamp("plain text line"),
            (None, "plain text line")
        );
        assert_eq!(
            split_timestamp("2024-05-01T10:00:00+02:00 hi"),
            (Some("2024-05-01T10:00:00+02:00"), "hi")
        );
        assert_eq!(
            split_timestamp("2024-05-01T10:00:00.5Z"),
            (Some("2024-05-01T10:00:00.5Z"), "")
        );
        assert_eq!(display_timestamp("not a time"), "not a time");
        assert!(display_timestamp("2024-05-01T10:00:00.123456789Z").ends_with(":00.123"));
    }
}
