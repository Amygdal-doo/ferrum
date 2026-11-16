//! Reverse proxy to the accounts service, so sign-in and account pages live
//! under this site's own URLs.
//!
//! With `ACCOUNTS_URL` set, [`PATHS`] go to that service: the method, path and
//! query, body and headers (Cookie, Origin, Referer, Authorization, …) go up;
//! the status, headers (Set-Cookie, Location, …) and body come back. Redirects
//! are passed through, never followed. The request gets `X-Forwarded-For`,
//! `-Proto` and `-Host`. Without `ACCOUNTS_URL` these paths are the normal
//! 404, except `/login` and `/signup`, which say sign-in is coming soon.
//!
//! Proxied responses are never cached and never stored here.

use std::{net::SocketAddr, time::Duration};

use axum::{
    body::{Body, to_bytes},
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

use super::AppState;
use crate::pages;

/// Exact paths, then prefixes (with their trailing `/`), that are proxied.
pub const PATHS: [&str; 4] = ["/login", "/signup", "/logout", "/account"];
pub const PREFIXES: [&str; 4] = ["/account/", "/auth/", "/oauth/", "/api/v1/"];
/// Largest request body sent upstream (forms and token JSON are tiny).
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
/// Largest response body accepted from upstream (account pages).
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(20);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// True for the paths [`proxy`] handles.
pub fn is_accounts_path(path: &str) -> bool {
    PATHS.contains(&path) || PREFIXES.iter().any(|p| path.starts_with(p))
}

/// Headers that belong to one connection (RFC 9110 §7.6.1), plus `Host` and
/// `Content-Length`, which the client sets itself.
fn hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "proxy-connection"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
    )
}

/// The headers to send upstream: the client's, minus hop-by-hop ones and
/// `Accept-Encoding` (this site compresses its own responses), plus the
/// `X-Forwarded-*` set.
pub fn upstream_headers(
    incoming: &HeaderMap,
    peer: Option<SocketAddr>,
    site_url: &str,
) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (name, value) in incoming {
        let forwarded = name.as_str().starts_with("x-forwarded-");
        if !hop_by_hop(name) && *name != header::ACCEPT_ENCODING && !forwarded {
            out.append(name.clone(), value.clone());
        }
    }
    let prior = incoming
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok());
    let chain = match (prior, peer) {
        (Some(p), Some(peer)) => Some(format!("{p}, {}", peer.ip())),
        (Some(p), None) => Some(p.to_owned()),
        (None, Some(peer)) => Some(peer.ip().to_string()),
        (None, None) => None,
    };
    let proto = incoming
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or(if site_url.starts_with("https://") {
            "https"
        } else {
            "http"
        })
        .to_owned();
    let host = incoming
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    for (name, value) in [
        ("x-forwarded-for", chain),
        ("x-forwarded-proto", Some(proto)),
        ("x-forwarded-host", host),
    ] {
        if let Some(v) = value.and_then(|v| HeaderValue::from_str(&v).ok()) {
            out.insert(HeaderName::from_static(name), v);
        }
    }
    out
}

/// The HTTP client for the accounts service. Never follows redirects: they
/// are the browser's to follow.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .user_agent(concat!("web-ferrum/", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_default()
}

fn no_store(mut res: Response) -> Response {
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

fn unavailable() -> Response {
    no_store(
        (
            StatusCode::BAD_GATEWAY,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "Ferrum accounts are unavailable right now. Try again in a few minutes.",
        )
            .into_response(),
    )
}

/// Every [`PATHS`] / [`PREFIXES`] request.
pub async fn proxy(State(s): State<AppState>, req: Request) -> Response {
    let Some(base) = s.config.accounts_url.as_deref() else {
        return not_configured(&s, req.uri().path()).await;
    };
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| *addr);
    let (parts, body) = req.into_parts();
    let path_and_query = parts.uri.path_and_query().map_or("/", |pq| pq.as_str());
    let Ok(body) = to_bytes(body, MAX_REQUEST_BYTES).await else {
        return no_store(StatusCode::PAYLOAD_TOO_LARGE.into_response());
    };
    let upstream = s
        .accounts_client
        .request(parts.method, format!("{base}{path_and_query}"))
        .headers(upstream_headers(&parts.headers, peer, s.site_url()))
        .body(body)
        .send()
        .await;
    let upstream = match upstream {
        Ok(res) => res,
        Err(err) => {
            tracing::warn!(%err, "accounts service unreachable");
            return unavailable();
        }
    };
    let status = upstream.status();
    let mut headers = HeaderMap::new();
    for (name, value) in upstream.headers() {
        if !hop_by_hop(name) {
            headers.append(name.clone(), value.clone());
        }
    }
    let body = match read_limited(upstream).await {
        Some(body) => body,
        None => return unavailable(),
    };
    let mut res = Response::new(Body::from(body));
    *res.status_mut() = status;
    *res.headers_mut() = headers;
    no_store(res)
}

async fn read_limited(mut res: reqwest::Response) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.ok()? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            tracing::warn!("accounts response too large");
            return None;
        }
        body.extend_from_slice(&chunk);
    }
    Some(body)
}

/// Without `ACCOUNTS_URL`: a "coming soon" page for `/login` and `/signup`,
/// the site's 404 for everything else.
async fn not_configured(s: &AppState, path: &str) -> Response {
    let res = match path {
        "/login" | "/signup" => pages::sign_in_soon::render(s.site_url()).into_response(),
        _ => super::not_found(State(s.clone())).await.into_response(),
    };
    no_store(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_only_the_account_paths() {
        for p in [
            "/login",
            "/signup",
            "/logout",
            "/account",
            "/account/sessions",
            "/auth/github/callback",
            "/oauth/authorize",
            "/api/v1/me",
        ] {
            assert!(is_accounts_path(p), "{p}");
        }
        for p in [
            "/",
            "/accounts",
            "/loginx",
            "/api/ping",
            "/api/v1",
            "/authx",
            "/oauth",
            "/about",
        ] {
            assert!(!is_accounts_path(p), "{p}");
        }
    }

    #[test]
    fn forwards_headers_and_adds_x_forwarded() {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, HeaderValue::from_static("tryferrum.dev"));
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("ferrum_session=frt_x"),
        );
        h.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://tryferrum.dev"),
        );
        h.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
        h.insert(header::ACCEPT_ENCODING, HeaderValue::from_static("gzip"));
        h.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.9"));
        let peer: SocketAddr = "10.0.0.2:5000".parse().unwrap_or_else(|_| unreachable!());
        let out = upstream_headers(&h, Some(peer), "https://tryferrum.dev");
        assert_eq!(out[header::COOKIE], "ferrum_session=frt_x");
        assert_eq!(out[header::ORIGIN], "https://tryferrum.dev");
        assert_eq!(out["x-forwarded-for"], "203.0.113.9, 10.0.0.2");
        assert_eq!(out["x-forwarded-proto"], "https");
        assert_eq!(out["x-forwarded-host"], "tryferrum.dev");
        assert!(out.get(header::HOST).is_none());
        assert!(out.get(header::CONNECTION).is_none());
        assert!(out.get(header::ACCEPT_ENCODING).is_none());
        let local = upstream_headers(&HeaderMap::new(), None, "http://localhost:8080");
        assert_eq!(local["x-forwarded-proto"], "http");
        assert!(local.get("x-forwarded-for").is_none());
    }
}
