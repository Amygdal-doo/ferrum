//! A minimal Server-Sent Events decoder for streamed HTTP bodies.

use futures::{Stream, StreamExt};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

/// Incremental decoder: feed bytes, take complete events.
#[derive(Debug, Default)]
pub struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buffer.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some((end, skip)) = find_boundary(&self.buffer) {
            let block: Vec<u8> = self.buffer.drain(..end + skip).take(end).collect();
            if let Some(event) = parse_block(&String::from_utf8_lossy(&block)) {
                out.push(event);
            }
        }
        out
    }

    /// Whatever is left when the body ends.
    pub fn finish(&mut self) -> Option<SseEvent> {
        let rest = std::mem::take(&mut self.buffer);
        parse_block(&String::from_utf8_lossy(&rest))
    }
}

fn find_boundary(buf: &[u8]) -> Option<(usize, usize)> {
    let lf = buf.windows(2).position(|w| w == b"\n\n").map(|i| (i, 2));
    let crlf = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| (i, 4));
    match (lf, crlf) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}

fn parse_block(block: &str) -> Option<SseEvent> {
    let mut event = SseEvent::default();
    let mut has_data = false;
    for line in block.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => event.event = Some(value.to_string()),
            "data" => {
                if has_data {
                    event.data.push('\n');
                }
                event.data.push_str(value);
                has_data = true;
            }
            _ => {}
        }
    }
    (has_data || event.event.is_some()).then_some(event)
}

/// The SSE events of a byte stream.
pub fn events<S, B, E>(body: S) -> impl Stream<Item = Result<SseEvent, E>>
where
    S: Stream<Item = Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
{
    futures::stream::unfold(
        (
            body,
            SseDecoder::default(),
            std::collections::VecDeque::new(),
            false,
        ),
        |(mut body, mut decoder, mut queue, mut done)| async move {
            loop {
                if let Some(event) = queue.pop_front() {
                    return Some((Ok(event), (body, decoder, queue, done)));
                }
                if done {
                    return None;
                }
                match body.next().await {
                    Some(Ok(bytes)) => queue.extend(decoder.feed(bytes.as_ref())),
                    Some(Err(e)) => return Some((Err(e), (body, decoder, queue, true))),
                    None => {
                        done = true;
                        queue.extend(decoder.finish());
                    }
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_split_chunks_comments_and_multiline_data() {
        let mut d = SseDecoder::default();
        assert!(d.feed(b"event: a\ndata: {\"x\"").is_empty());
        let got = d.feed(b":1}\n\n: comment\n\ndata: one\ndata: two\r\n\r\ndata: tail");
        assert_eq!(
            got,
            [
                SseEvent {
                    event: Some("a".into()),
                    data: "{\"x\":1}".into()
                },
                SseEvent {
                    event: None,
                    data: "one\ntwo".into()
                },
            ]
        );
        assert_eq!(d.finish().map(|e| e.data), Some("tail".into()));
    }
}
