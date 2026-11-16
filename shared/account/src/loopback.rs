//! The loopback redirect target: a one-shot HTTP listener on
//! `127.0.0.1:<random port>` that waits for the browser's
//! `GET /callback?code=…&state=…`, answers with a small page and hands the
//! code back.

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::{AccountError, AuthCode, Result};

pub const CALLBACK_PATH: &str = "/callback";
/// A request head larger than this is not a browser redirect.
const MAX_HEAD: usize = 16 * 1024;
/// A connection that sends nothing for this long is dropped.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

const PAGE_STYLE: &str = "body{font:15px -apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;\
background:#111113;color:#ededef;display:flex;align-items:center;justify-content:center;\
height:100vh;margin:0}main{text-align:center}h1{font-size:20px;font-weight:600;margin:0 0 8px}\
p{color:#a0a0a8;margin:0}";

fn page(title: &str, detail: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>{title}</title>\
         <style>{PAGE_STYLE}</style></head><body><main><h1>{title}</h1><p>{detail}</p></main>\
         </body></html>"
    )
}

/// The listener of one sign-in.
pub struct Loopback {
    listener: TcpListener,
    port: u16,
}

impl Loopback {
    /// Binds `127.0.0.1:0` (a free port chosen by the OS).
    pub async fn bind() -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|e| AccountError::Network(format!("could not listen on 127.0.0.1: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| AccountError::Network(e.to_string()))?
            .port();
        Ok(Self { listener, port })
    }

    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// `http://127.0.0.1:<port>/callback`.
    #[must_use]
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}{CALLBACK_PATH}", self.port)
    }

    /// Serves requests until one reaches `/callback`, and returns its code.
    /// Other paths (`/favicon.ico`) get a 404 and the wait goes on.
    pub async fn wait(self, expected_state: &str) -> Result<AuthCode> {
        loop {
            let (stream, _) = self
                .listener
                .accept()
                .await
                .map_err(|e| AccountError::Network(e.to_string()))?;
            if let Some(result) = handle(stream, expected_state).await {
                return result;
            }
        }
    }
}

/// Answers one connection; `None` when it was not the callback.
async fn handle(mut stream: TcpStream, expected_state: &str) -> Option<Result<AuthCode>> {
    let target = match tokio::time::timeout(READ_TIMEOUT, read_target(&mut stream)).await {
        Ok(Some(target)) => target,
        _ => return None,
    };
    let Some(result) = parse_callback(&target, expected_state) else {
        respond(&mut stream, "404 Not Found", &page("Not found", "")).await;
        return None;
    };
    let (status, body) = match &result {
        Ok(_) => (
            "200 OK",
            page(
                "Signed in \u{2014} you can close this tab",
                "Return to Ferrum.",
            ),
        ),
        Err(AccountError::Denied) => (
            "200 OK",
            page("Sign-in cancelled", "You can close this tab."),
        ),
        Err(e) => (
            "400 Bad Request",
            page(
                "Sign-in failed",
                &format!("{e}. Try signing in again from the app."),
            ),
        ),
    };
    respond(&mut stream, status, &body).await;
    Some(result)
}

/// The request target of a `GET` (e.g. `/callback?code=…`), from the head.
async fn read_target(stream: &mut TcpStream) -> Option<String> {
    let mut head = Vec::with_capacity(1024);
    let mut buf = [0_u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut buf).await.ok()?;
        if n == 0 || head.len() + n > MAX_HEAD {
            return None;
        }
        head.extend_from_slice(&buf[..n]);
    }
    let line = std::str::from_utf8(&head).ok()?.lines().next()?;
    let mut parts = line.split(' ');
    match (parts.next(), parts.next()) {
        (Some("GET"), Some(target)) => Some(target.to_owned()),
        _ => None,
    }
}

async fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// Reads a request target. `None` unless the path is `/callback`; then the
/// code, or why there is none. `state` is checked before anything else.
#[must_use]
pub fn parse_callback(target: &str, expected_state: &str) -> Option<Result<AuthCode>> {
    let url = url::Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
    if url.path() != CALLBACK_PATH {
        return None;
    }
    let mut code = None;
    let mut state = None;
    let mut error = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            _ => {}
        }
    }
    if state.as_deref() != Some(expected_state) {
        return Some(Err(AccountError::StateMismatch));
    }
    Some(match (error, code) {
        (Some(e), _) if e == "access_denied" => Err(AccountError::Denied),
        (Some(e), _) => Err(AccountError::Callback(format!("the server said `{e}`"))),
        (None, Some(code)) if !code.is_empty() => Ok(AuthCode::new(code)),
        (None, _) => Err(AccountError::Callback("no code".into())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(target: &str) -> Option<Result<AuthCode>> {
        parse_callback(target, "st4te")
    }

    #[test]
    fn parses_the_code() {
        let code = parse("/callback?code=fac_abc&state=st4te").expect("callback");
        assert_eq!(code.expect("code").expose(), "fac_abc");
    }

    #[test]
    fn rejects_a_state_mismatch_even_with_a_code() {
        for target in [
            "/callback?code=fac_abc&state=other",
            "/callback?code=fac_abc",
            "/callback?error=access_denied&state=other",
        ] {
            assert!(
                matches!(parse(target), Some(Err(AccountError::StateMismatch))),
                "{target}"
            );
        }
    }

    #[test]
    fn denied_and_other_errors() {
        assert!(matches!(
            parse("/callback?error=access_denied&state=st4te"),
            Some(Err(AccountError::Denied))
        ));
        assert!(matches!(
            parse("/callback?error=server_error&state=st4te"),
            Some(Err(AccountError::Callback(_)))
        ));
        assert!(matches!(
            parse("/callback?state=st4te"),
            Some(Err(AccountError::Callback(_)))
        ));
    }

    #[test]
    fn other_paths_are_not_the_callback() {
        assert!(parse("/favicon.ico").is_none());
        assert!(parse("/callbackx?code=a&state=st4te").is_none());
    }

    #[tokio::test]
    async fn serves_the_page_and_ignores_other_requests() {
        let loopback = Loopback::bind().await.expect("bind");
        let uri = loopback.redirect_uri();
        assert!(uri.starts_with("http://127.0.0.1:") && uri.ends_with("/callback"));
        let port = loopback.port();
        let waiter = tokio::spawn(async move { loopback.wait("s").await });

        let favicon = raw_get(port, "/favicon.ico").await;
        assert!(favicon.starts_with("HTTP/1.1 404"), "{favicon}");
        let page = raw_get(port, "/callback?code=fac_1&state=s").await;
        assert!(page.starts_with("HTTP/1.1 200"), "{page}");
        assert!(page.contains("you can close this tab"));

        let code = waiter.await.expect("join").expect("code");
        assert_eq!(code.expose(), "fac_1");
    }

    async fn raw_get(port: u16, target: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect");
        stream
            .write_all(format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
            .await
            .expect("write");
        let mut out = String::new();
        stream.read_to_string(&mut out).await.expect("read");
        out
    }
}
