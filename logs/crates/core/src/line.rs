//! One parsed log line: ANSI colours as ranges, level, timestamp and, for
//! JSON lines (pino, bunyan, tracing-json, zap, …), message plus fields.

use crate::ansi::{self, AnsiStyle};
use crate::level::{self, Level};
use serde_json::{Map, Value};
use std::ops::Range;

/// Where a line came from within its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stream {
    Stdout,
    Stderr,
    /// Lines written by the Logs page itself (started, exited, file rotated…).
    System,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogLine {
    /// Position in the project's history; set by [`crate::LogBuffer::push`].
    pub seq: u64,
    /// The source slot within its session (stable while the app runs).
    pub source: u32,
    pub stream: Stream,
    pub level: Option<Level>,
    /// Display form (`12:00:01.123`, local time), when the line had one.
    pub timestamp: Option<String>,
    /// Sub-source: the compose service or the file name for multi-file globs.
    pub tag: Option<String>,
    /// What is shown and searched: escapes removed, the message for JSON.
    pub text: String,
    /// ANSI styles as byte ranges into `text`.
    pub spans: Vec<(Range<usize>, AnsiStyle)>,
    /// Other JSON fields, in order, values rendered compactly.
    pub fields: Vec<(String, String)>,
    /// The original line for JSON (what "copy" puts on the clipboard).
    pub original: Option<String>,
}

impl LogLine {
    /// A line written by the Logs page (status changes, errors).
    #[must_use]
    pub fn system(source: u32, text: impl Into<String>, level: Option<Level>) -> Self {
        Self {
            seq: 0,
            source,
            stream: Stream::System,
            level,
            timestamp: Some(chrono::Local::now().format("%H:%M:%S%.3f").to_string()),
            tag: None,
            text: text.into(),
            spans: Vec::new(),
            fields: Vec::new(),
            original: None,
        }
    }

    /// The line as it was produced (JSON source, or the text with its tag).
    #[must_use]
    pub fn copy_text(&self) -> String {
        if let Some(original) = &self.original {
            return original.clone();
        }
        let mut out = String::new();
        if let Some(ts) = &self.timestamp {
            out.push_str(ts);
            out.push(' ');
        }
        out.push_str(&self.text);
        out
    }

    /// `key=value key2=value2`, for the dim suffix after a JSON message.
    #[must_use]
    pub fn fields_inline(&self) -> String {
        let mut out = String::new();
        for (key, value) in &self.fields {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(key);
            out.push('=');
            out.push_str(value);
        }
        out
    }
}

/// How the raw text of a source is shaped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParseOptions {
    /// Lines start with an RFC 3339 timestamp (`docker logs --timestamps`).
    pub timestamp_prefix: bool,
    /// Lines start with `service-1  | ` (`docker compose logs`).
    pub compose_prefix: bool,
}

/// Parses one raw line (without its newline).
#[must_use]
pub fn parse(source: u32, stream: Stream, raw: &str, options: ParseOptions) -> LogLine {
    let mut rest = raw.strip_suffix('\r').unwrap_or(raw);
    let mut tag = None;
    if options.compose_prefix
        && let Some((service, after)) = split_compose(rest)
    {
        tag = Some(service.to_owned());
        rest = after;
    }
    let mut timestamp = None;
    if options.timestamp_prefix {
        let (ts, after) = split_timestamp(rest);
        timestamp = ts.map(display_rfc3339);
        rest = after;
    }
    let styled = ansi::parse(rest);
    let mut line = LogLine {
        seq: 0,
        source,
        stream,
        level: None,
        timestamp,
        tag,
        text: styled.text,
        spans: styled.spans,
        fields: Vec::new(),
        original: None,
    };
    let trimmed = line.text.trim();
    if trimmed.starts_with('{')
        && trimmed.ends_with('}')
        && let Ok(map) = serde_json::from_str::<Map<String, Value>>(trimmed)
    {
        apply_json(&mut line, map);
    } else {
        line.level = level::detect(&line.text);
    }
    line
}

const LEVEL_KEYS: [&str; 5] = ["level", "lvl", "severity", "log.level", "levelname"];
const TIME_KEYS: [&str; 6] = ["time", "timestamp", "ts", "@timestamp", "t", "datetime"];
const MESSAGE_KEYS: [&str; 4] = ["msg", "message", "@message", "event"];

fn apply_json(line: &mut LogLine, mut map: Map<String, Value>) {
    line.original = Some(line.text.clone());
    for key in LEVEL_KEYS {
        if let Some(value) = map.shift_remove(key) {
            line.level = match &value {
                Value::String(s) => Level::from_word(s.trim()),
                Value::Number(n) => n.as_f64().and_then(Level::from_number),
                _ => None,
            };
            break;
        }
    }
    for key in TIME_KEYS {
        if let Some(value) = map.shift_remove(key) {
            line.timestamp = display_json_time(&value).or(line.timestamp.take());
            break;
        }
    }
    // tracing-subscriber's JSON layer nests the message in `fields`.
    if let Some(Value::Object(fields)) = map.shift_remove("fields") {
        for (key, value) in fields {
            map.entry(key).or_insert(value);
        }
    }
    let mut message = None;
    for key in MESSAGE_KEYS {
        if let Some(Value::String(text)) = map.get(key) {
            message = Some(text.clone());
            map.shift_remove(key);
            break;
        }
    }
    // bunyan/pino bookkeeping that only adds noise.
    for key in ["v", "pid", "hostname"] {
        map.shift_remove(key);
    }
    line.fields = map
        .into_iter()
        .map(|(key, value)| {
            let value = match value {
                Value::String(s) => s,
                other => other.to_string(),
            };
            (key, value)
        })
        .collect();
    line.text = message.map_or_else(|| line.fields_inline(), |m| ansi::strip(&m));
    if line.level.is_none() {
        line.level = level::detect(&line.text);
    }
    line.spans.clear();
}

/// `web-1  | text` → (`web-1`, `text`).
fn split_compose(line: &str) -> Option<(&str, &str)> {
    let (head, rest) = line.split_once(" | ").or_else(|| {
        // An empty line from a service: `web-1  |`.
        line.strip_suffix(" |").map(|head| (head, ""))
    })?;
    let service = head.trim();
    let valid = !service.is_empty()
        && service
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    valid.then_some((service, rest))
}

/// Splits a leading `2024-05-01T10:00:00.123456789Z ` off a line.
#[must_use]
pub fn split_timestamp(line: &str) -> (Option<&str>, &str) {
    let (head, rest) = line.split_once(' ').unwrap_or((line, ""));
    if looks_like_rfc3339(head) {
        (Some(head), rest)
    } else {
        (None, line)
    }
}

fn looks_like_rfc3339(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 20
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && (s.ends_with('Z') || s.get(19..).is_some_and(|t| t.contains(['+', '-'])))
}

fn display_rfc3339(ts: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(ts).map_or_else(
        |_| ts.to_owned(),
        |t| {
            t.with_timezone(&chrono::Local)
                .format("%H:%M:%S%.3f")
                .to_string()
        },
    )
}

/// JSON times: RFC 3339 strings or Unix seconds / milliseconds.
fn display_json_time(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(display_rfc3339(s)),
        Value::Number(n) => {
            let n = n.as_f64()?;
            let millis = if n > 1e11 { n } else { n * 1000. } as i64;
            let time = chrono::DateTime::from_timestamp_millis(millis)?;
            Some(
                time.with_timezone(&chrono::Local)
                    .format("%H:%M:%S%.3f")
                    .to_string(),
            )
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(raw: &str) -> LogLine {
        parse(0, Stream::Stdout, raw, ParseOptions::default())
    }

    #[test]
    fn plain_lines_keep_colours_and_get_a_level() {
        let line = plain("\u{1b}[31mERROR\u{1b}[0m db down\r");
        assert_eq!(line.text, "ERROR db down");
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.level, Some(Level::Error));
        assert_eq!(line.copy_text(), "ERROR db down");
        assert!(line.original.is_none());
    }

    #[test]
    fn pino_and_bunyan_lines() {
        let line = plain(
            r#"{"level":50,"time":1714557600000,"pid":1,"hostname":"h","msg":"boom","req":{"id":7},"v":1}"#,
        );
        assert_eq!(line.level, Some(Level::Error));
        assert_eq!(line.text, "boom");
        assert!(line.timestamp.is_some());
        assert_eq!(line.fields, [("req".to_owned(), r#"{"id":7}"#.to_owned())]);
        assert!(line.copy_text().starts_with("{\"level\":50"));
    }

    #[test]
    fn tracing_and_zap_lines() {
        let line = plain(
            r#"{"timestamp":"2024-05-01T10:00:00.5Z","level":"WARN","fields":{"message":"slow","ms":812},"target":"app::db"}"#,
        );
        assert_eq!(line.level, Some(Level::Warn));
        assert_eq!(line.text, "slow");
        assert_eq!(line.fields.len(), 2);
        assert_eq!(line.fields_inline(), "target=app::db ms=812");
        let zap =
            plain(r#"{"level":"info","ts":1714557600.25,"caller":"main.go:12","msg":"ready"}"#);
        assert_eq!(zap.level, Some(Level::Info));
        assert_eq!(zap.text, "ready");
        // No message key: the fields are the text.
        let bare = plain(r#"{"a":1,"b":"x"}"#);
        assert_eq!(bare.text, "a=1 b=x");
        assert_eq!(bare.level, None);
        // Not JSON after all.
        assert_eq!(plain("{not json}").text, "{not json}");
    }

    #[test]
    fn docker_and_compose_prefixes() {
        let options = ParseOptions {
            timestamp_prefix: true,
            compose_prefix: true,
        };
        let line = parse(
            1,
            Stream::Stdout,
            "web-1  | 2024-05-01T10:00:00.123456789Z [WARN] hi",
            options,
        );
        assert_eq!(line.tag.as_deref(), Some("web-1"));
        assert!(
            line.timestamp
                .as_deref()
                .is_some_and(|t| t.ends_with(".123"))
        );
        assert_eq!(line.text, "[WARN] hi");
        assert_eq!(line.level, Some(Level::Warn));
        let empty = parse(1, Stream::Stdout, "db-1  |", options);
        assert_eq!(empty.tag.as_deref(), Some("db-1"));
        assert_eq!(empty.text, "");
        assert_eq!(split_timestamp("plain text"), (None, "plain text"));
        assert_eq!(
            split_timestamp("2024-05-01T10:00:00+02:00"),
            (Some("2024-05-01T10:00:00+02:00"), "")
        );
    }

    #[test]
    fn system_lines() {
        let line = LogLine::system(3, "exited", Some(Level::Error));
        assert_eq!(line.stream, Stream::System);
        assert_eq!(line.source, 3);
        assert!(line.copy_text().ends_with(" exited"));
    }
}
