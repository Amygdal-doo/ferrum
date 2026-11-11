//! Combined logs of a compose project (`docker compose logs -f`): every
//! service's stream, line by line, prefixed with the service name and
//! interleaved as lines arrive.

use crate::engine::{EngineStream, SharedEngine};
use crate::logs::split_timestamp;
use crate::model::{LogChunk, LogOptions, LogStream};
use futures::StreamExt;

/// Turns one container's raw chunks into whole lines prefixed with
/// `label | ` (after the engine timestamp, so timestamps still parse).
#[derive(Debug, Clone)]
pub struct LinePrefixer {
    prefix: String,
    timestamps: bool,
    /// Unterminated tail per stream (stdout, stderr).
    partial: [String; 2],
}

impl LinePrefixer {
    /// `width` pads labels so the `|` columns line up.
    #[must_use]
    pub fn new(label: &str, width: usize, timestamps: bool) -> Self {
        Self {
            prefix: format!("{label:<width$} | "),
            timestamps,
            partial: [String::new(), String::new()],
        }
    }

    fn line(&self, out: &mut String, line: &str) {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let (ts, rest) = if self.timestamps {
            split_timestamp(line)
        } else {
            (None, line)
        };
        if let Some(ts) = ts {
            out.push_str(ts);
            out.push(' ');
        }
        out.push_str(&self.prefix);
        out.push_str(rest);
        out.push('\n');
    }

    /// The complete lines of `chunk` (plus earlier partial text), prefixed;
    /// `None` when no line was completed.
    pub fn push(&mut self, chunk: &LogChunk) -> Option<LogChunk> {
        let slot = usize::from(chunk.stream == LogStream::Stderr);
        let mut pending = std::mem::take(&mut self.partial[slot]);
        pending.push_str(&chunk.text);
        let mut out = String::new();
        let mut rest = pending.as_str();
        while let Some(newline) = rest.find('\n') {
            self.line(&mut out, &rest[..newline]);
            rest = &rest[newline + 1..];
        }
        self.partial[slot] = rest.to_owned();
        (!out.is_empty()).then_some(LogChunk {
            stream: chunk.stream,
            text: out,
        })
    }

    /// Unterminated lines, flushed when the stream ends.
    pub fn finish(&mut self) -> Vec<LogChunk> {
        [LogStream::Stdout, LogStream::Stderr]
            .into_iter()
            .enumerate()
            .filter_map(|(slot, stream)| {
                let text = std::mem::take(&mut self.partial[slot]);
                (!text.is_empty()).then(|| {
                    let mut out = String::new();
                    self.line(&mut out, &text);
                    LogChunk { stream, text: out }
                })
            })
            .collect()
    }
}

/// One container's prefixed lines. A stream error becomes a stderr line of
/// that service, so one failing container doesn't end the others.
fn prefixed(
    engine: &SharedEngine,
    id: &str,
    prefixer: LinePrefixer,
    options: LogOptions,
) -> EngineStream<LogChunk> {
    let inner = engine.logs(id, options);
    futures::stream::unfold(Some((inner, prefixer)), |state| async move {
        let (mut inner, mut prefixer) = state?;
        let mut items = Vec::new();
        let more = loop {
            match inner.next().await {
                Some(Ok(chunk)) => {
                    if let Some(out) = prefixer.push(&chunk) {
                        items.push(Ok(out));
                        break true;
                    }
                }
                Some(Err(error)) => {
                    items.extend(prefixer.finish().into_iter().map(Ok));
                    let mut text = String::new();
                    prefixer.line(&mut text, &format!("[logs ended: {error}]"));
                    items.push(Ok(LogChunk {
                        stream: LogStream::Stderr,
                        text,
                    }));
                    break false;
                }
                None => {
                    items.extend(prefixer.finish().into_iter().map(Ok));
                    break false;
                }
            }
        };
        if items.is_empty() {
            return None;
        }
        let next = more.then_some((inner, prefixer));
        Some((futures::stream::iter(items), next))
    })
    .flatten()
    .boxed()
}

/// Follows (per `options`) the logs of every `(container id, service)` of a
/// project as one stream. Lines keep the engine timestamp first.
pub fn project_logs(
    engine: &SharedEngine,
    sources: &[(String, String)],
    options: LogOptions,
) -> EngineStream<LogChunk> {
    let width = sources
        .iter()
        .map(|(_, label)| label.chars().count())
        .max()
        .unwrap_or(0);
    let streams = sources.iter().map(|(id, label)| {
        prefixed(
            engine,
            id,
            LinePrefixer::new(label, width, options.timestamps),
            options,
        )
    });
    futures::stream::select_all(streams).boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeEngine;
    use std::sync::Arc;

    fn chunk(stream: LogStream, text: &str) -> LogChunk {
        LogChunk {
            stream,
            text: text.to_owned(),
        }
    }

    #[test]
    fn prefixes_whole_lines_after_the_timestamp() {
        let mut p = LinePrefixer::new("db", 4, true);
        assert_eq!(
            p.push(&chunk(LogStream::Stdout, "2024-05-01T10:00:00Z rea")),
            None
        );
        let out = p.push(&chunk(LogStream::Stdout, "dy\r\nplain line\npart"));
        assert_eq!(
            out.map(|c| c.text).as_deref(),
            Some("2024-05-01T10:00:00Z db   | ready\ndb   | plain line\n")
        );
        let rest = p.finish();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].text, "db   | part\n");
        assert!(p.finish().is_empty());
    }

    #[tokio::test]
    async fn merges_services_and_keeps_going_after_one_ends() {
        let fake = Arc::new(FakeEngine::default());
        {
            let mut s = fake.state();
            s.container_logs
                .insert("a".into(), vec![chunk(LogStream::Stdout, "one\ntwo")]);
            s.container_logs
                .insert("b".into(), vec![chunk(LogStream::Stderr, "oops\n")]);
        }
        let engine: crate::engine::SharedEngine = fake.clone();
        let sources = [
            ("a".to_owned(), "app".to_owned()),
            ("b".to_owned(), "worker".to_owned()),
        ];
        let options = LogOptions {
            timestamps: false,
            ..LogOptions::default()
        };
        let chunks: Vec<LogChunk> = project_logs(&engine, &sources, options)
            .filter_map(|c| async move { c.ok() })
            .collect()
            .await;
        let mut lines: Vec<String> = chunks
            .iter()
            .flat_map(|c| c.text.lines().map(str::to_owned).collect::<Vec<_>>())
            .collect();
        lines.sort();
        assert_eq!(lines, ["app    | one", "app    | two", "worker | oops"]);
        assert!(chunks.iter().any(|c| c.stream == LogStream::Stderr));
        let requests = fake.state().log_requests.clone();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|(_, o)| o.follow && !o.timestamps));
    }
}
