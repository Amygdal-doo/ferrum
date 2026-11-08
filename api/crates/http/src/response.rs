//! Converts a `reqwest::Response` into the domain [`HttpResponse`].

use crate::cookies::parse_set_cookies;
use ferrumapi_domain::HttpResponse;
use reqwest::header::SET_COOKIE;
use std::time::Instant;

/// Reads the full body; `time_ms` is measured from `started` to body end.
pub(crate) async fn read_response(
    resp: reqwest::Response,
    started: Instant,
) -> Result<HttpResponse, reqwest::Error> {
    let status = resp.status();
    let http_version = format!("{:?}", resp.version());
    let final_url = resp.url().to_string();
    let headers: Vec<(String, String)> = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_owned(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let cookies = parse_set_cookies(
        headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(SET_COOKIE.as_str()))
            .map(|(_, v)| v.as_str()),
    );
    let body = resp.bytes().await?.to_vec();
    let time_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    Ok(HttpResponse {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or_default().to_owned(),
        http_version,
        final_url,
        headers,
        cookies,
        size_bytes: body.len() as u64,
        body,
        time_ms,
    })
}
