//! Auth that depends on the final request: OAuth 1.0a and AWS SigV4 sign
//! the built `reqwest::Request` (URL, form body, payload hash); Digest
//! answers the server's `401` challenge (see `HttpClient`). An explicit
//! user `Authorization` header still wins.

pub mod aws;
pub mod digest;
pub mod oauth1;

use ferrumapi_domain::{Auth, ExecError};
use hmac::{Hmac, Mac as _};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue};
use sha1::Sha1;
use sha2::{Digest as _, Sha256};

/// RFC 3986 unreserved characters stay; everything else is encoded.
const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

pub(crate) fn enc(s: &str) -> String {
    utf8_percent_encode(s, UNRESERVED).to_string()
}

/// AWS uses the same unreserved set.
pub(crate) fn aws_enc(s: &str) -> String {
    enc(s)
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

pub(crate) fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

pub(crate) fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    // HMAC accepts keys of any length.
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(key) else {
        return Vec::new();
    };
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

pub(crate) fn hmac_sha1(key: &[u8], data: &[u8]) -> Vec<u8> {
    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(key) else {
        return Vec::new();
    };
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// A random lowercase hex nonce (128 bits).
pub(crate) fn nonce() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn header_value(v: &str) -> Result<HeaderValue, ExecError> {
    let mut value = HeaderValue::from_str(v)
        .map_err(|_| ExecError::InvalidRequest("auth value contains invalid characters".into()))?;
    value.set_sensitive(true);
    Ok(value)
}

/// Signs `req` for OAuth 1.0a and AWS SigV4; other auth types are left
/// alone. Does nothing when the user set `Authorization` themselves.
pub(crate) fn sign(auth: &Auth, req: &mut reqwest::Request) -> Result<(), ExecError> {
    if req.headers().contains_key(AUTHORIZATION) {
        return Ok(());
    }
    let now = chrono::Utc::now();
    let method = req.method().as_str().to_owned();
    match auth {
        Auth::OAuth1(cfg) => {
            let form = form_pairs(req);
            let value =
                oauth1::authorization(&method, req.url(), &form, cfg, &nonce(), now.timestamp());
            req.headers_mut()
                .insert(AUTHORIZATION, header_value(&value)?);
        }
        Auth::AwsSigV4(cfg) => {
            let payload = match req.body() {
                None => sha256_hex(b""),
                Some(b) => b
                    .as_bytes()
                    .map_or_else(|| aws::UNSIGNED_PAYLOAD.to_owned(), sha256_hex),
            };
            let extra: Vec<(String, String)> = req
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|v| vec![("content-type".to_owned(), v.to_owned())])
                .unwrap_or_default();
            let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
            let signed = aws::sign(&method, req.url(), &extra, &payload, cfg, &amz_date);
            for (k, v) in signed.headers {
                let name = HeaderName::from_bytes(k.as_bytes())
                    .map_err(|e| ExecError::Other(e.to_string()))?;
                req.headers_mut().insert(name, header_value(&v)?);
            }
            req.headers_mut()
                .insert(AUTHORIZATION, header_value(&signed.authorization)?);
        }
        _ => {}
    }
    Ok(())
}

/// Decoded pairs of a buffered `application/x-www-form-urlencoded` body.
fn form_pairs(req: &reqwest::Request) -> Vec<(String, String)> {
    let is_form = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/x-www-form-urlencoded"));
    match req.body().and_then(reqwest::Body::as_bytes) {
        Some(bytes) if is_form => url::form_urlencoded::parse(bytes)
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect(),
        _ => Vec::new(),
    }
}

/// The `Authorization` header answering a Digest challenge in `resp`, if
/// it has one.
pub(crate) fn digest_answer(
    username: &str,
    password: &str,
    method: &str,
    url: &url::Url,
    resp: &reqwest::Response,
) -> Option<String> {
    let challenge = digest::parse_challenge(
        resp.headers()
            .get_all(reqwest::header::WWW_AUTHENTICATE)
            .iter()
            .filter_map(|v| v.to_str().ok()),
    )?;
    let uri = match url.query() {
        Some(q) => format!("{}?{q}", url.path()),
        None => url.path().to_owned(),
    };
    Some(digest::authorization(
        &challenge,
        username,
        password,
        method,
        &uri,
        &nonce(),
        1,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_and_hex() {
        assert_eq!(enc("a b+c/~"), "a%20b%2Bc%2F~");
        assert_eq!(enc("ü"), "%C3%BC");
        assert_eq!(hex(&[0, 15, 255]), "000fff");
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(nonce().len(), 32);
    }
}
