//! Assembles the outgoing URL, headers and body from a substituted request.

use crate::auth::apply_auth;
use crate::body::{PreparedBody, prepare_body};
use ferrumapi_domain::{ExecError, HttpRequest, KeyValue};
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use url::Url;

pub(crate) struct Prepared {
    pub url: Url,
    pub headers: HeaderMap,
    pub body: PreparedBody,
}

/// Parses a user-typed URL. Without a scheme, `http://` is assumed (as
/// Postman does). Only `http` and `https` are accepted. Error messages never
/// include the URL itself.
pub fn parse_url(raw: &str) -> Result<Url, ExecError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(ExecError::InvalidUrl("URL is empty".into()));
    }
    let url = if has_scheme(raw) {
        Url::parse(raw)
    } else {
        Url::parse(&format!("http://{raw}"))
    }
    .map_err(|e| ExecError::InvalidUrl(e.to_string()))?;
    match url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(ExecError::InvalidUrl(format!(
                "unsupported scheme {other:?}"
            )));
        }
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(ExecError::InvalidUrl("missing host".into()));
    }
    Ok(url)
}

/// `scheme://` prefix where scheme is `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`.
fn has_scheme(s: &str) -> bool {
    s.split_once("://").is_some_and(|(scheme, _)| {
        let mut chars = scheme.chars();
        chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    })
}

/// Active header rows only, in order; repeated names are kept.
pub(crate) fn user_headers(rows: &[KeyValue]) -> Result<HeaderMap, ExecError> {
    let mut map = HeaderMap::new();
    for kv in rows.iter().filter(|kv| kv.is_active()) {
        let key = kv.key.trim();
        let name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|_| ExecError::InvalidRequest(format!("invalid header name {key:?}")))?;
        let value = HeaderValue::from_str(kv.value.trim()).map_err(|_| {
            ExecError::InvalidRequest(format!("header {key:?} has an invalid value"))
        })?;
        map.append(name, value);
    }
    Ok(map)
}

/// `request` must already be substituted. Enabled params are part of
/// `request.url`, so `params` is not appended again.
pub(crate) async fn prepare(request: &HttpRequest) -> Result<Prepared, ExecError> {
    let mut url = parse_url(&request.url)?;
    let mut headers = user_headers(&request.headers)?;
    apply_auth(&request.auth, &mut headers, &mut url)?;
    let body = prepare_body(&request.body).await?;
    match &body {
        PreparedBody::Bytes { content_type, .. } if !headers.contains_key(CONTENT_TYPE) => {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
        }
        PreparedBody::Multipart(_) => {
            // A user Content-Type without a boundary would make the body
            // unparseable; let reqwest generate the correct one instead.
            let has_boundary = headers
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.to_ascii_lowercase().contains("boundary="));
            if !has_boundary {
                headers.remove(CONTENT_TYPE);
            }
        }
        _ => {}
    }
    Ok(Prepared { url, headers, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_scheme_defaulting() {
        let u = parse_url("localhost:8080/a?b=1").expect("parse");
        assert_eq!(u.as_str(), "http://localhost:8080/a?b=1");
        let u = parse_url("  https://example.test/x ").expect("parse");
        assert_eq!(u.scheme(), "https");
        assert!(matches!(parse_url(""), Err(ExecError::InvalidUrl(_))));
        assert!(matches!(
            parse_url("ftp://x"),
            Err(ExecError::InvalidUrl(_))
        ));
        assert!(matches!(
            parse_url("http://"),
            Err(ExecError::InvalidUrl(_))
        ));
        let Err(ExecError::InvalidUrl(msg)) = parse_url("http://sec ret@[::1") else {
            panic!("expected invalid url");
        };
        assert!(!msg.contains("sec ret"));
    }

    #[test]
    fn headers_only_active_and_validated() {
        let mut off = KeyValue::new("X-Off", "1");
        off.enabled = false;
        let rows = [
            KeyValue::new("X-A", "1"),
            KeyValue::new("x-a", "2"),
            off,
            KeyValue::new(" ", "ignored"),
        ];
        let map = user_headers(&rows).expect("headers");
        assert_eq!(map.len(), 2);
        assert!(map.get("x-off").is_none());
        let bad = [KeyValue::new("Bad Name", "v")];
        assert!(matches!(
            user_headers(&bad),
            Err(ExecError::InvalidRequest(_))
        ));
        let bad_val = [KeyValue::new("X", "a\nb")];
        assert!(matches!(
            user_headers(&bad_val),
            Err(ExecError::InvalidRequest(_))
        ));
    }
}
