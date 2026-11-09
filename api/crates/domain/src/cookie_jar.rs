//! Cookie jar: cookies received in `Set-Cookie` headers, kept for the
//! workspace and sent back on later requests to matching hosts and paths.
//! Storage and matching follow RFC 6265 §5.3–5.4, minus the public-suffix
//! list (a local API client talks to hosts the user chose).

use crate::response::ResponseCookie;
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StoredCookie {
    pub name: String,
    pub value: String,
    /// Lowercase host or domain, without a leading dot.
    pub domain: String,
    /// No `Domain` attribute was given: sent to exactly `domain` only.
    pub host_only: bool,
    pub path: String,
    /// `None` for session cookies, which are kept until cleared.
    pub expires: Option<DateTime<Utc>>,
    pub secure: bool,
    pub http_only: bool,
    pub created: DateTime<Utc>,
}

impl StoredCookie {
    #[must_use]
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires.is_some_and(|at| at <= now)
    }

    fn same_slot(&self, other: &Self) -> bool {
        self.name == other.name && self.domain == other.domain && self.path == other.path
    }
}

/// All stored cookies. Order is insertion order; [`CookieJar::matching`]
/// returns them in `Cookie` header order.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CookieJar {
    cookies: Vec<StoredCookie>,
}

/// `Expires` attribute value (RFC 1123, or the RFC 850 / dashed variants
/// servers still send). `None` if unparseable.
#[must_use]
pub fn parse_cookie_date(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if let Ok(at) = DateTime::parse_from_rfc2822(raw) {
        return Some(at.with_timezone(&Utc));
    }
    let spaced = raw.replace('-', " ");
    if let Ok(at) = DateTime::parse_from_rfc2822(&spaced) {
        return Some(at.with_timezone(&Utc));
    }
    ["%A, %d %b %y %H:%M:%S GMT", "%a, %d %b %y %H:%M:%S GMT"]
        .into_iter()
        .find_map(|fmt| NaiveDateTime::parse_from_str(&spaced, fmt).ok())
        .map(|at| at.and_utc())
}

/// RFC 6265 §5.1.3: `host` equals `domain` or is a subdomain of it.
#[must_use]
pub fn domain_matches(host: &str, domain: &str) -> bool {
    if host == domain {
        return true;
    }
    let is_ip = host.starts_with('[') || host.parse::<IpAddr>().is_ok();
    !is_ip
        && host.len() > domain.len()
        && host.ends_with(domain)
        && host.as_bytes()[host.len() - domain.len() - 1] == b'.'
}

/// RFC 6265 §5.1.4 default path: the request path up to its last `/`.
#[must_use]
pub fn default_path(request_path: &str) -> String {
    match request_path.rfind('/') {
        Some(i) if i > 0 && request_path.starts_with('/') => request_path[..i].to_owned(),
        _ => "/".to_owned(),
    }
}

/// RFC 6265 §5.1.4 path-match.
#[must_use]
pub fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    request_path == cookie_path
        || (request_path.starts_with(cookie_path)
            && (cookie_path.ends_with('/') || request_path[cookie_path.len()..].starts_with('/')))
}

/// Absolute expiry of a received cookie. `ResponseCookie::expires` holds
/// either the raw `Expires` text or `Max-Age=N`.
fn expiry(cookie: &ResponseCookie, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let raw = cookie.expires.as_deref()?;
    match raw.strip_prefix("Max-Age=") {
        Some(secs) => {
            let secs: i64 = secs.trim().parse().ok()?;
            Some(if secs <= 0 {
                DateTime::<Utc>::MIN_UTC
            } else {
                Duration::try_seconds(secs)
                    .and_then(|d| now.checked_add_signed(d))
                    .unwrap_or(DateTime::<Utc>::MAX_UTC)
            })
        }
        None => parse_cookie_date(raw),
    }
}

impl CookieJar {
    #[must_use]
    pub fn from_cookies(cookies: Vec<StoredCookie>) -> Self {
        Self { cookies }
    }

    #[must_use]
    pub fn cookies(&self) -> &[StoredCookie] {
        &self.cookies
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }

    /// Stores `cookie`, received from `host` for `request_path`, replacing
    /// one with the same name, domain and path. An already-expired cookie
    /// deletes that slot instead. Returns `false` (and stores nothing) when
    /// the `Domain` attribute does not cover `host`.
    pub fn store(
        &mut self,
        cookie: &ResponseCookie,
        host: &str,
        request_path: &str,
        now: DateTime<Utc>,
    ) -> bool {
        let host = host.to_ascii_lowercase();
        let (domain, host_only) = match cookie.domain.as_deref().map(str::trim) {
            Some(d) if !d.is_empty() => (d.trim_start_matches('.').to_ascii_lowercase(), false),
            _ => (host.clone(), true),
        };
        if !domain_matches(&host, &domain) {
            return false;
        }
        let path = match cookie.path.as_deref() {
            Some(p) if p.starts_with('/') => p.to_owned(),
            _ => default_path(request_path),
        };
        let mut stored = StoredCookie {
            name: cookie.name.clone(),
            value: cookie.value.clone(),
            domain,
            host_only,
            path,
            expires: expiry(cookie, now),
            secure: cookie.secure,
            http_only: cookie.http_only,
            created: now,
        };
        if let Some(i) = self.cookies.iter().position(|c| c.same_slot(&stored)) {
            stored.created = self.cookies.remove(i).created;
        }
        if !stored.is_expired(now) {
            self.cookies.push(stored);
        }
        true
    }

    /// Unexpired cookies to send to `host` + `path`, longest path first, then
    /// oldest first (RFC 6265 §5.4). `Secure` cookies only over https.
    #[must_use]
    pub fn matching(
        &self,
        host: &str,
        path: &str,
        https: bool,
        now: DateTime<Utc>,
    ) -> Vec<&StoredCookie> {
        let host = host.to_ascii_lowercase();
        let mut out: Vec<&StoredCookie> = self
            .cookies
            .iter()
            .filter(|c| !c.is_expired(now))
            .filter(|c| {
                if c.host_only {
                    host == c.domain
                } else {
                    domain_matches(&host, &c.domain)
                }
            })
            .filter(|c| path_matches(path, &c.path))
            .filter(|c| https || !c.secure)
            .collect();
        out.sort_by(|a, b| {
            b.path
                .len()
                .cmp(&a.path.len())
                .then(a.created.cmp(&b.created))
        });
        out
    }

    /// `Cookie` header value for a request, `None` when nothing matches.
    #[must_use]
    pub fn header_for(
        &self,
        host: &str,
        path: &str,
        https: bool,
        now: DateTime<Utc>,
    ) -> Option<String> {
        let pairs: Vec<String> = self
            .matching(host, path, https, now)
            .into_iter()
            .map(|c| format!("{}={}", c.name, c.value))
            .collect();
        (!pairs.is_empty()).then(|| pairs.join("; "))
    }

    /// Drops expired cookies; returns how many were removed.
    pub fn remove_expired(&mut self, now: DateTime<Utc>) -> usize {
        let before = self.cookies.len();
        self.cookies.retain(|c| !c.is_expired(now));
        before - self.cookies.len()
    }

    /// Removes one cookie by its identity. Returns whether it existed.
    pub fn remove(&mut self, domain: &str, path: &str, name: &str) -> bool {
        let before = self.cookies.len();
        self.cookies
            .retain(|c| !(c.domain == domain && c.path == path && c.name == name));
        before != self.cookies.len()
    }

    /// Removes every cookie stored for `domain`.
    pub fn clear_domain(&mut self, domain: &str) {
        self.cookies.retain(|c| c.domain != domain);
    }

    /// Distinct domains, sorted.
    #[must_use]
    pub fn domains(&self) -> Vec<String> {
        let mut out: Vec<String> = self.cookies.iter().map(|c| c.domain.clone()).collect();
        out.sort();
        out.dedup();
        out
    }
}

#[cfg(test)]
#[path = "cookie_jar_tests.rs"]
mod tests;
