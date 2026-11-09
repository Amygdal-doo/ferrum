//! The workspace cookie jar: filled from `Set-Cookie` responses, sent back
//! on later requests to matching hosts and paths, persisted in the store.
//! Jar failures are logged and never block a request.

use crate::AppServices;
use chrono::Utc;
use ferrumapi_domain::{CookieJar, HttpRequest, HttpResponse, KeyValue, VariableSet};
use ferrumapi_http::parse_url;
use ferrumapi_persistence::StoreResult;
use std::collections::HashSet;
use std::sync::Arc;

/// Loaded on first use. A Tokio mutex held across the save, so concurrent
/// responses are persisted in the order they were applied.
pub(crate) type SharedJar = Arc<tokio::sync::Mutex<Option<CookieJar>>>;

/// Adds the jar's `name=value` pairs to the request's `Cookie` header. A
/// cookie the user already set explicitly in that header wins.
fn merge_cookie_header(headers: &mut Vec<KeyValue>, jar_header: &str) {
    let user = headers
        .iter_mut()
        .find(|kv| kv.is_active() && kv.key.trim().eq_ignore_ascii_case("cookie"));
    let Some(user) = user else {
        headers.push(KeyValue::new("Cookie", jar_header));
        return;
    };
    let taken: HashSet<String> = user
        .value
        .split(';')
        .map(|pair| pair.split('=').next().unwrap_or_default().trim().to_owned())
        .collect();
    let extra: Vec<&str> = jar_header
        .split("; ")
        .filter(|pair| !taken.contains(pair.split('=').next().unwrap_or_default()))
        .collect();
    if !extra.is_empty() {
        let own = user.value.trim().trim_end_matches(';').trim_end();
        user.value = if own.is_empty() {
            extra.join("; ")
        } else {
            format!("{own}; {}", extra.join("; "))
        };
    }
}

impl AppServices {
    /// Runs `f` on the loaded jar and saves it when `f` reports a change.
    async fn with_jar<T>(&self, f: impl FnOnce(&mut CookieJar) -> (T, bool)) -> StoreResult<T> {
        let mut guard = self.cookies.lock().await;
        let jar = match guard.take() {
            Some(jar) => jar,
            None => self.store.load_cookies().await?,
        };
        let jar = guard.insert(jar);
        let (out, changed) = f(jar);
        if changed {
            self.store.save_cookies(jar).await?;
        }
        Ok(out)
    }

    /// Current unexpired cookies (expired ones are dropped from the store).
    pub async fn cookie_jar(&self) -> StoreResult<CookieJar> {
        self.with_jar(|jar| {
            let removed = jar.remove_expired(Utc::now());
            (jar.clone(), removed > 0)
        })
        .await
    }

    /// Deletes one cookie and returns the remaining jar.
    pub async fn delete_cookie(
        &self,
        domain: String,
        path: String,
        name: String,
    ) -> StoreResult<CookieJar> {
        self.with_jar(|jar| {
            let changed = jar.remove(&domain, &path, &name);
            (jar.clone(), changed)
        })
        .await
    }

    /// Clears every cookie, or only those of `domain`.
    pub async fn clear_cookies(&self, domain: Option<String>) -> StoreResult<CookieJar> {
        self.with_jar(|jar| {
            match domain {
                Some(d) => jar.clear_domain(&d),
                None => *jar = CookieJar::default(),
            }
            (jar.clone(), true)
        })
        .await
    }

    /// Adds matching jar cookies to `request` (URL resolved with `vars`).
    pub(crate) async fn apply_cookies(&self, request: &mut HttpRequest, vars: &VariableSet) {
        let Ok(url) = parse_url(&vars.resolve(&request.url)) else {
            return;
        };
        let host = url.host_str().unwrap_or_default().to_owned();
        let https = url.scheme() == "https";
        let header = self
            .with_jar(|jar| (jar.header_for(&host, url.path(), https, Utc::now()), false))
            .await;
        match header {
            Ok(Some(header)) => merge_cookie_header(&mut request.headers, &header),
            Ok(None) => {}
            Err(e) => tracing::warn!(error = %e, "cookie jar unavailable"),
        }
    }

    /// Stores the response's cookies against its final URL.
    pub(crate) async fn remember_cookies(&self, response: &HttpResponse) {
        if response.cookies.is_empty() {
            return;
        }
        let Ok(url) = parse_url(&response.final_url) else {
            return;
        };
        let host = url.host_str().unwrap_or_default().to_owned();
        let now = Utc::now();
        let result = self
            .with_jar(|jar| {
                let mut changed = false;
                for cookie in &response.cookies {
                    changed |= jar.store(cookie, &host, url.path(), now);
                }
                ((), changed)
            })
            .await;
        if let Err(e) = result {
            tracing::warn!(error = %e, "could not save cookies");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie_values(headers: &[KeyValue]) -> Vec<(&str, &str)> {
        headers
            .iter()
            .map(|kv| (kv.key.as_str(), kv.value.as_str()))
            .collect()
    }

    #[test]
    fn adds_header_when_user_has_none() {
        let mut headers = vec![KeyValue::new("Accept", "*/*")];
        merge_cookie_header(&mut headers, "a=1; b=2");
        assert_eq!(
            cookie_values(&headers),
            [("Accept", "*/*"), ("Cookie", "a=1; b=2")]
        );
    }

    #[test]
    fn user_cookie_wins_and_jar_fills_the_rest() {
        let mut headers = vec![KeyValue::new("cookie", "a=mine;")];
        merge_cookie_header(&mut headers, "a=jar; b=2");
        assert_eq!(cookie_values(&headers), [("cookie", "a=mine; b=2")]);

        let mut headers = vec![KeyValue::new("Cookie", "a=mine; b=mine")];
        merge_cookie_header(&mut headers, "a=jar; b=jar");
        assert_eq!(cookie_values(&headers), [("Cookie", "a=mine; b=mine")]);
    }

    #[test]
    fn disabled_user_header_is_ignored() {
        let mut off = KeyValue::new("Cookie", "a=off");
        off.enabled = false;
        let mut headers = vec![off];
        merge_cookie_header(&mut headers, "a=jar");
        assert_eq!(
            cookie_values(&headers),
            [("Cookie", "a=off"), ("Cookie", "a=jar")]
        );
    }
}
