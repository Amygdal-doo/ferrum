//! Conditional GETs: a [`Transport`] wrapper that remembers each response's
//! `ETag`, sends `If-None-Match` next time and turns a `304 Not Modified`
//! back into the remembered body. GitHub does not count 304s against the
//! rate limit, so polling an unchanged run list is free.

use async_trait::async_trait;
use ferrumgit_forge::{ForgeResult, HttpRequest, HttpResponse, Method, Transport};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// Entries kept before the cache starts over.
const MAX_ENTRIES: usize = 256;
/// Larger bodies (logs) are not kept.
const MAX_BODY: usize = 512 * 1024;

pub struct EtagTransport {
    inner: Arc<dyn Transport>,
    entries: Mutex<HashMap<u64, (String, HttpResponse)>>,
    hits: AtomicU64,
}

impl EtagTransport {
    #[must_use]
    pub fn new(inner: Arc<dyn Transport>) -> Self {
        Self {
            inner,
            entries: Mutex::default(),
            hits: AtomicU64::new(0),
        }
    }

    /// Responses answered from the cache so far.
    #[must_use]
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, HashMap<u64, (String, HttpResponse)>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// URL + credentials, so one token never sees another's cached answer.
fn key(r: &HttpRequest) -> u64 {
    let mut h = DefaultHasher::new();
    r.url.hash(&mut h);
    for (k, v) in &r.headers {
        if k.eq_ignore_ascii_case("authorization") {
            v.hash(&mut h);
        }
    }
    h.finish()
}

#[async_trait]
impl Transport for EtagTransport {
    async fn send(&self, mut request: HttpRequest) -> ForgeResult<HttpResponse> {
        if request.method != Method::Get || request.url.contains("/logs") {
            return self.inner.send(request).await;
        }
        let key = key(&request);
        let cached = self.entries().get(&key).cloned();
        if let Some((etag, _)) = &cached {
            request
                .headers
                .push(("if-none-match".to_owned(), etag.clone()));
        }
        let response = self.inner.send(request).await?;
        if response.status == 304
            && let Some((_, mut old)) = cached
        {
            self.hits.fetch_add(1, Ordering::Relaxed);
            // Fresh rate-limit headers, old body.
            old.headers = response.headers;
            return Ok(old);
        }
        if (200..300).contains(&response.status)
            && response.body.len() <= MAX_BODY
            && let Some(etag) = response.header("etag")
        {
            let mut entries = self.entries();
            if entries.len() >= MAX_ENTRIES {
                entries.clear();
            }
            entries.insert(key, (etag.to_owned(), response.clone()));
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Answers 304 when the right `If-None-Match` arrives.
    struct Server {
        seen: Mutex<Vec<HttpRequest>>,
    }

    #[async_trait]
    impl Transport for Server {
        async fn send(&self, request: HttpRequest) -> ForgeResult<HttpResponse> {
            let conditional = request
                .headers
                .iter()
                .any(|(k, v)| k == "if-none-match" && v == "\"v1\"");
            self.seen.lock().expect("lock").push(request);
            Ok(if conditional {
                HttpResponse {
                    status: 304,
                    headers: vec![("x-ratelimit-remaining".into(), "4999".into())],
                    body: String::new(),
                }
            } else {
                HttpResponse {
                    status: 200,
                    headers: vec![
                        ("etag".into(), "\"v1\"".into()),
                        ("x-ratelimit-remaining".into(), "5000".into()),
                    ],
                    body: "{\"ok\":true}".into(),
                }
            })
        }
    }

    fn get(url: &str, token: &str) -> HttpRequest {
        HttpRequest {
            method: Method::Get,
            url: url.into(),
            headers: vec![("authorization".into(), format!("Bearer {token}"))],
            body: None,
        }
    }

    #[tokio::test]
    async fn replays_body_on_not_modified() {
        let server = Arc::new(Server {
            seen: Mutex::default(),
        });
        let t = EtagTransport::new(server.clone());
        let first = t.send(get("https://x/runs", "a")).await.expect("first");
        assert_eq!(first.status, 200);
        let second = t.send(get("https://x/runs", "a")).await.expect("second");
        assert_eq!(second.status, 200);
        assert_eq!(second.body, "{\"ok\":true}");
        assert_eq!(second.header("x-ratelimit-remaining"), Some("4999"));
        assert_eq!(t.hits(), 1);
        // Another token is not served from this token's cache.
        t.send(get("https://x/runs", "b")).await.expect("other");
        assert_eq!(t.hits(), 1);
        // Logs are never conditional.
        t.send(get("https://x/jobs/1/logs", "a"))
            .await
            .expect("logs");
        t.send(get("https://x/jobs/1/logs", "a"))
            .await
            .expect("logs");
        let seen = server.seen.lock().expect("lock");
        assert!(!seen[4].headers.iter().any(|(k, _)| k == "if-none-match"));
        assert_eq!(t.hits(), 1);
    }
}
