//! JSON-RPC framing used by language servers: a `Content-Length` header,
//! a blank line, then the JSON body.

use std::io::{self, BufRead, Write};

use serde_json::Value;

/// Writes one framed message.
pub fn write_message(out: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message).map_err(io::Error::other)?;
    write!(out, "Content-Length: {}\r\n\r\n", body.len())?;
    out.write_all(&body)?;
    out.flush()
}

/// Reads one framed message; `Ok(None)` at end of stream.
pub fn read_message(input: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut length: Option<usize> = None;
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            if length.is_some() {
                break;
            }
            // Stray blank line before the headers.
            continue;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length =
                Some(value.trim().parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "bad Content-Length")
                })?);
        }
    }
    let mut body = vec![0; length.unwrap_or(0)];
    input.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trips_messages() {
        let mut bytes = Vec::new();
        write_message(
            &mut bytes,
            &json!({"jsonrpc": "2.0", "id": 1, "result": "é"}),
        )
        .expect("test value");
        write_message(&mut bytes, &json!({"method": "x"})).expect("test value");
        assert!(bytes.starts_with(b"Content-Length: 38\r\n\r\n{"));
        let mut input = io::Cursor::new(bytes);
        assert_eq!(
            read_message(&mut input)
                .expect("test value")
                .expect("test value")["result"],
            "é"
        );
        assert_eq!(
            read_message(&mut input)
                .expect("test value")
                .expect("test value")["method"],
            "x"
        );
        assert!(read_message(&mut input).expect("test value").is_none());
    }

    #[test]
    fn tolerates_extra_headers_and_case() {
        let raw = b"content-length: 2\r\nContent-Type: application/vscode-jsonrpc\r\n\r\n{}";
        let mut input = io::Cursor::new(&raw[..]);
        assert_eq!(
            read_message(&mut input).expect("test value"),
            Some(json!({}))
        );
    }

    #[test]
    fn bad_body_is_an_error() {
        let mut input = io::Cursor::new(&b"Content-Length: 3\r\n\r\nnop"[..]);
        assert!(read_message(&mut input).is_err());
    }
}
