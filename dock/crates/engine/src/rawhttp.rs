//! Minimal HTTP/1.0 GET over the engine socket, for the one call bollard
//! cannot make: `GET /system/df?type=volume` (its query serializer rejects the
//! `type` array, and an untyped df fails on some engines while computing image usage).

use crate::endpoint::Endpoint;
use crate::error::{EngineError, EngineResult};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_BODY: usize = 32 << 20;

/// `GET path` → (status, body). HTTP/1.0 so the engine closes the connection
/// and does not use chunked encoding.
pub async fn get(
    endpoint: &Endpoint,
    path: &str,
    timeout: Duration,
) -> EngineResult<(u16, Vec<u8>)> {
    let request =
        format!("GET {path} HTTP/1.0\r\nHost: localhost\r\nUser-Agent: FerrumDock\r\n\r\n");
    let exchange = async {
        let response = match endpoint {
            #[cfg(unix)]
            Endpoint::Unix(socket) => {
                exchange(tokio::net::UnixStream::connect(socket).await?, &request).await?
            }
            #[cfg(not(unix))]
            Endpoint::Unix(_) => {
                return Err(std::io::Error::other("unix sockets are not supported here"));
            }
            Endpoint::Tcp(addr) => {
                exchange(tokio::net::TcpStream::connect(addr).await?, &request).await?
            }
        };
        Ok::<_, std::io::Error>(response)
    };
    let response = tokio::time::timeout(timeout, exchange)
        .await
        .map_err(|_| EngineError::Transport(format!("{path}: timed out")))?
        .map_err(|e| EngineError::Transport(format!("{path}: {e}")))?;
    parse_response(&response)
}

async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    request: &str,
) -> std::io::Result<Vec<u8>> {
    stream.write_all(request.as_bytes()).await?;
    let mut response = Vec::new();
    (&mut stream)
        .take(MAX_BODY as u64)
        .read_to_end(&mut response)
        .await?;
    Ok(response)
}

/// Splits an HTTP/1.x response into status and body.
pub fn parse_response(response: &[u8]) -> EngineResult<(u16, Vec<u8>)> {
    let invalid = || EngineError::Transport("invalid HTTP response from the engine".into());
    let split = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(invalid)?;
    let head = std::str::from_utf8(&response[..split]).map_err(|_| invalid())?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or_else(invalid)?;
    if head.lines().any(|l| {
        let l = l.to_ascii_lowercase();
        l.starts_with("transfer-encoding:") && l.contains("chunked")
    }) {
        return Err(EngineError::Transport("unexpected chunked response".into()));
    }
    Ok((status, response[split + 4..].to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_and_body() {
        let raw = b"HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n{\"a\":1}";
        let (status, body) = parse_response(raw).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(status, 200);
        assert_eq!(body, b"{\"a\":1}");
        assert!(parse_response(b"garbage").is_err());
        assert!(
            parse_response(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n").is_err()
        );
    }

    #[tokio::test]
    async fn talks_http_over_a_unix_socket() {
        let dir = std::env::temp_dir().join(format!("ferrumdock-raw-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let socket = dir.join("engine.sock");
        let _ = std::fs::remove_file(&socket);
        let listener =
            tokio::net::UnixListener::bind(&socket).unwrap_or_else(|e| panic!("bind: {e}"));
        let server = tokio::spawn(async move {
            let (mut conn, _) = listener
                .accept()
                .await
                .unwrap_or_else(|e| panic!("accept: {e}"));
            let mut buf = vec![0; 1024];
            let n = conn.read(&mut buf).await.unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]).into_owned();
            let _ = conn.write_all(b"HTTP/1.0 200 OK\r\n\r\nhello").await;
            request
        });
        let result = get(
            &Endpoint::Unix(socket.clone()),
            "/system/df?type=volume",
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(result.ok(), Some((200, b"hello".to_vec())));
        let request = server.await.unwrap_or_default();
        assert!(request.starts_with("GET /system/df?type=volume HTTP/1.0\r\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
