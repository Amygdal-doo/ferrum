//! Static assets embedded in the binary via rust-embed.
//!
//! URLs produced by [`url`] carry a content hash (`?v=<hash>`), so a request
//! whose `v` matches the current file is served `immutable` for a year. Any
//! other request (no `v`, or a stale one) gets a short revalidating cache.
//! Every asset response carries a strong ETag and honours `If-None-Match`.

use axum::{
    extract::{Path, RawQuery},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

pub use super::middleware::with_middleware;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Assets;

/// Cache policy for URLs whose `v` matches the file's content hash.
pub const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// Cache policy for unversioned or stale-versioned asset URLs.
pub const REVALIDATE: &str = "public, max-age=300, must-revalidate";

/// Number of hex chars of the sha256 used as the cache-busting version.
const VERSION_LEN: usize = 10;

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Short content hash of an embedded asset, if it exists.
pub fn version(path: &str) -> Option<String> {
    Assets::get(path).map(|f| {
        let mut h = hex(&f.metadata.sha256_hash());
        h.truncate(VERSION_LEN);
        h
    })
}

/// Public URL for an embedded asset path such as `"css/base.css"`,
/// cache-busted with a hash of the file's content.
pub fn url(path: &str) -> String {
    match version(path) {
        Some(v) => format!("/assets/{path}?v={v}"),
        None => {
            tracing::warn!(path, "assets::url called for a missing asset");
            format!("/assets/{path}")
        }
    }
}

/// Value of the `v` query parameter, if any.
fn query_version(query: Option<&str>) -> Option<&str> {
    query?.split('&').find_map(|pair| pair.strip_prefix("v="))
}

/// True when an `If-None-Match` header value matches `etag`.
fn etag_matches(if_none_match: &str, etag: &str) -> bool {
    if_none_match.split(',').map(str::trim).any(|candidate| {
        candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == etag
    })
}

pub async fn serve(
    Path(path): Path<String>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Response {
    let Some(file) = Assets::get(&path) else {
        return (
            StatusCode::NOT_FOUND,
            [(header::CACHE_CONTROL, "no-cache")],
            "not found",
        )
            .into_response();
    };
    let hash = hex(&file.metadata.sha256_hash());
    let etag = format!("\"{hash}\"");
    let cache = match query_version(query.as_deref()) {
        Some(v) if hash.get(..VERSION_LEN) == Some(v) => IMMUTABLE,
        _ => REVALIDATE,
    };
    let etag_value = HeaderValue::from_str(&etag).unwrap_or(HeaderValue::from_static("\"\""));

    let not_modified = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|inm| etag_matches(inm, &etag));
    if not_modified {
        return (
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag_value),
                (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
            ],
        )
            .into_response();
    }

    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    let content_type = HeaderValue::from_str(mime.as_ref())
        .unwrap_or(HeaderValue::from_static("application/octet-stream"));
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::ETAG, etag_value),
            (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
        ],
        file.data,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_is_content_hashed() {
        let u = url("css/tokens.css");
        let v = u.rsplit("?v=").next().unwrap_or_default();
        assert!(u.starts_with("/assets/css/tokens.css?v="));
        assert_eq!(v.len(), VERSION_LEN);
        assert!(v.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn missing_asset_url_has_no_version() {
        assert_eq!(url("nope.css"), "/assets/nope.css");
    }

    #[test]
    fn parses_query_version() {
        assert_eq!(query_version(Some("v=abc")), Some("abc"));
        assert_eq!(query_version(Some("x=1&v=abc")), Some("abc"));
        assert_eq!(query_version(Some("x=1")), None);
        assert_eq!(query_version(None), None);
    }

    #[test]
    fn matches_etags() {
        assert!(etag_matches("\"a\"", "\"a\""));
        assert!(etag_matches("\"b\", W/\"a\"", "\"a\""));
        assert!(etag_matches("*", "\"a\""));
        assert!(!etag_matches("\"b\"", "\"a\""));
    }
}
