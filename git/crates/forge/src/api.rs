//! Shared REST plumbing: auth headers, rate-limit tracking, error mapping.

use crate::error::{ForgeError, ForgeResult, scope_hint};
use crate::transport::{HttpRequest, HttpResponse, Method, Transport};
use ferrumgit_domain::{ForgeKind, Secret};
use serde::de::DeserializeOwned;
use std::sync::{Arc, Mutex, PoisonError};

/// Below this many remaining requests, lists skip per-item status lookups.
pub const LOW_RATE_LIMIT: u64 = 100;

/// Last rate-limit headers seen for one API. Share one per API base URL so
/// the state survives between clients.
#[derive(Debug, Default)]
pub struct RateLimits {
    state: Mutex<Option<RateState>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateState {
    pub remaining: u64,
    /// Unix seconds.
    pub reset_at: Option<i64>,
}

impl RateLimits {
    #[must_use]
    pub fn get(&self) -> Option<RateState> {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set(&self, s: RateState) {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) = Some(s);
    }

    /// Refuses early while the limit is exhausted and not yet reset.
    fn check(&self, now: i64) -> ForgeResult<()> {
        match self.get() {
            Some(RateState {
                remaining: 0,
                reset_at: Some(at),
            }) if at > now => Err(ForgeError::RateLimited { reset_at: Some(at) }),
            _ => Ok(()),
        }
    }
}

/// An authenticated connection to one forge API.
#[derive(Clone)]
pub struct Api {
    transport: Arc<dyn Transport>,
    pub(crate) kind: ForgeKind,
    pub(crate) host: String,
    base: String,
    token: Secret,
    limits: Arc<RateLimits>,
}

impl std::fmt::Debug for Api {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Api")
            .field("kind", &self.kind)
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl Api {
    #[must_use]
    pub fn new(
        transport: Arc<dyn Transport>,
        kind: ForgeKind,
        host: &str,
        base: &str,
        token: Secret,
        limits: Arc<RateLimits>,
    ) -> Self {
        Self {
            transport,
            kind,
            host: host.to_owned(),
            base: base.trim_end_matches('/').to_owned(),
            token,
            limits,
        }
    }

    #[must_use]
    pub fn rate(&self) -> Option<RateState> {
        self.limits.get()
    }

    /// Plenty of requests left for optional extra lookups.
    #[must_use]
    pub fn has_headroom(&self) -> bool {
        self.rate().is_none_or(|r| r.remaining >= LOW_RATE_LIMIT)
    }

    fn headers(&self) -> Vec<(String, String)> {
        match self.kind {
            ForgeKind::GitHub => vec![
                ("accept".into(), "application/vnd.github+json".into()),
                ("x-github-api-version".into(), "2022-11-28".into()),
                (
                    "authorization".into(),
                    format!("Bearer {}", self.token.expose()),
                ),
            ],
            ForgeKind::GitLab => vec![
                ("accept".into(), "application/json".into()),
                ("private-token".into(), self.token.expose().to_owned()),
            ],
        }
    }

    /// `path` is relative to the API base (or an absolute URL).
    pub async fn request(
        &self,
        method: Method,
        path: impl AsRef<str> + Send,
        body: Option<serde_json::Value>,
    ) -> ForgeResult<HttpResponse> {
        let path = path.as_ref();
        self.limits.check(chrono::Utc::now().timestamp())?;
        let url = if path.starts_with("http://") || path.starts_with("https://") {
            path.to_owned()
        } else {
            format!("{}{path}", self.base)
        };
        let request = HttpRequest {
            method,
            url,
            headers: self.headers(),
            body: body.map(|b| b.to_string()),
        };
        tracing::debug!(method = method.as_str(), path, "forge request");
        let response = self.transport.send(request).await?;
        if let Some(state) = rate_state(&response) {
            self.limits.set(state);
        }
        if (200..300).contains(&response.status) {
            Ok(response)
        } else {
            Err(map_error(self.kind, &self.host, &response))
        }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: impl AsRef<str> + Send) -> ForgeResult<T> {
        let response = self.request(Method::Get, path, None).await?;
        parse(&response.body)
    }

    pub async fn send<T: DeserializeOwned>(
        &self,
        method: Method,
        path: impl AsRef<str> + Send,
        body: serde_json::Value,
    ) -> ForgeResult<T> {
        let response = self.request(method, path, Some(body)).await?;
        parse(&response.body)
    }

    /// Sends and ignores the response body.
    pub async fn send_unit(
        &self,
        method: Method,
        path: impl AsRef<str> + Send,
        body: serde_json::Value,
    ) -> ForgeResult<()> {
        self.request(method, path, Some(body)).await.map(|_| ())
    }
}

pub(crate) fn parse<T: DeserializeOwned>(body: &str) -> ForgeResult<T> {
    serde_json::from_str(body).map_err(|e| ForgeError::Parse(e.to_string()))
}

/// `x-ratelimit-*` (GitHub) or `ratelimit-*` (GitLab).
fn rate_state(r: &HttpResponse) -> Option<RateState> {
    let remaining = r
        .header("x-ratelimit-remaining")
        .or_else(|| r.header("ratelimit-remaining"))?
        .trim()
        .parse()
        .ok()?;
    let reset_at = r
        .header("x-ratelimit-reset")
        .or_else(|| r.header("ratelimit-reset"))
        .and_then(|v| v.trim().parse().ok());
    Some(RateState {
        remaining,
        reset_at,
    })
}

/// Human message from a GitHub / GitLab error body.
fn body_message(body: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return body.trim().chars().take(300).collect();
    };
    let mut parts: Vec<String> = Vec::new();
    for key in ["message", "error", "error_description"] {
        if let Some(m) = v.get(key) {
            flatten(m, None, &mut parts);
        }
    }
    // GitHub validation details.
    if let Some(errors) = v.get("errors").and_then(|e| e.as_array()) {
        for e in errors {
            if let Some(m) = e.get("message").and_then(|m| m.as_str()) {
                parts.push(m.to_owned());
            } else if let (Some(field), Some(code)) = (
                e.get("field").and_then(|f| f.as_str()),
                e.get("code").and_then(|c| c.as_str()),
            ) {
                parts.push(format!("{field}: {code}"));
            }
        }
    }
    parts.dedup();
    parts.join(" — ")
}

fn flatten(v: &serde_json::Value, key: Option<&str>, out: &mut Vec<String>) {
    match v {
        serde_json::Value::String(s) => out.push(match key {
            Some(k) => format!("{k} {s}"),
            None => s.clone(),
        }),
        serde_json::Value::Array(items) => {
            for i in items {
                flatten(i, key, out);
            }
        }
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                flatten(v, Some(k), out);
            }
        }
        _ => {}
    }
}

pub(crate) fn map_error(kind: ForgeKind, host: &str, r: &HttpResponse) -> ForgeError {
    let message = body_message(&r.body);
    let rate_exhausted = rate_state(r).is_some_and(|s| s.remaining == 0);
    let secondary = message.to_lowercase().contains("rate limit");
    if r.status == 429 || ((r.status == 403) && (rate_exhausted || secondary)) {
        let now = chrono::Utc::now().timestamp();
        let reset_at = r
            .header("retry-after")
            .and_then(|v| v.trim().parse::<i64>().ok())
            .map(|secs| now + secs)
            .or_else(|| rate_state(r).and_then(|s| s.reset_at));
        return ForgeError::RateLimited { reset_at };
    }
    match r.status {
        401 => ForgeError::Unauthorized {
            host: host.to_owned(),
            hint: format!("It may have expired or been revoked. {}", scope_hint(kind)),
        },
        403 => ForgeError::Forbidden(if message.is_empty() {
            format!("the token lacks permission. {}", scope_hint(kind))
        } else {
            format!("{message}. {}", scope_hint(kind))
        }),
        404 => ForgeError::NotFound(if message.is_empty() || message == "Not Found" {
            String::new()
        } else {
            format!(" ({message})")
        }),
        405 | 409 | 422 => ForgeError::Rejected(if message.is_empty() {
            format!("The request was rejected (HTTP {}).", r.status)
        } else {
            message
        }),
        status => ForgeError::Http {
            host: host.to_owned(),
            status,
            message: if message.is_empty() {
                "no details".to_owned()
            } else {
                message
            },
        },
    }
}

/// Percent-encodes a path segment (GitLab project paths, branch names in queries).
#[must_use]
pub fn encode(segment: &str) -> String {
    use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
    const SET: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');
    utf8_percent_encode(segment, SET).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(status: u16, headers: &[(&str, &str)], body: &str) -> HttpResponse {
        HttpResponse {
            status,
            headers: headers
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            body: body.to_owned(),
        }
    }

    #[test]
    fn maps_http_errors_to_clear_messages() {
        let gh = ForgeKind::GitHub;
        let e = map_error(
            gh,
            "github.com",
            &resp(401, &[], r#"{"message":"Bad credentials"}"#),
        );
        assert!(matches!(e, ForgeError::Unauthorized { .. }));
        assert!(e.to_string().contains("`repo`"), "{e}");
        let e = map_error(
            ForgeKind::GitLab,
            "gitlab.com",
            &resp(401, &[], r#"{"message":"401 Unauthorized"}"#),
        );
        assert!(e.to_string().contains("`api`"), "{e}");

        let e = map_error(
            gh,
            "github.com",
            &resp(
                422,
                &[],
                r#"{"message":"Validation Failed","errors":[{"resource":"PullRequest","code":"custom","message":"A pull request already exists for o:feat."}]}"#,
            ),
        );
        assert_eq!(
            e.to_string(),
            "Validation Failed — A pull request already exists for o:feat."
        );
        let e = map_error(
            ForgeKind::GitLab,
            "gitlab.com",
            &resp(
                409,
                &[],
                r#"{"message":["Another open merge request already exists for this source branch: !4"]}"#,
            ),
        );
        assert!(e.to_string().starts_with("Another open merge request"));
        let e = map_error(
            ForgeKind::GitLab,
            "gitlab.com",
            &resp(400, &[], r#"{"message":{"title":["can't be blank"]}}"#),
        );
        assert!(e.to_string().contains("title can't be blank"), "{e}");
        assert!(matches!(
            map_error(gh, "h", &resp(404, &[], r#"{"message":"Not Found"}"#)),
            ForgeError::NotFound(m) if m.is_empty()
        ));
        assert!(matches!(
            map_error(gh, "h", &resp(502, &[], "<html>bad gateway</html>")),
            ForgeError::Http { status: 502, .. }
        ));
    }

    #[test]
    fn detects_rate_limits() {
        let gh = ForgeKind::GitHub;
        let e = map_error(
            gh,
            "github.com",
            &resp(
                403,
                &[
                    ("x-ratelimit-remaining", "0"),
                    ("x-ratelimit-reset", "1999999999"),
                ],
                r#"{"message":"API rate limit exceeded"}"#,
            ),
        );
        assert_eq!(
            e,
            ForgeError::RateLimited {
                reset_at: Some(1_999_999_999)
            }
        );
        let e = map_error(
            ForgeKind::GitLab,
            "gitlab.com",
            &resp(429, &[("retry-after", "30")], ""),
        );
        assert!(matches!(e, ForgeError::RateLimited { reset_at: Some(_) }));
        // A plain 403 is a permission problem.
        assert!(matches!(
            map_error(
                gh,
                "h",
                &resp(
                    403,
                    &[("x-ratelimit-remaining", "4000")],
                    r#"{"message":"Resource not accessible by personal access token"}"#
                )
            ),
            ForgeError::Forbidden(_)
        ));
    }

    #[test]
    fn exhausted_limit_refuses_until_reset() {
        let l = RateLimits::default();
        assert!(l.check(100).is_ok());
        l.set(RateState {
            remaining: 0,
            reset_at: Some(200),
        });
        assert!(matches!(l.check(100), Err(ForgeError::RateLimited { .. })));
        assert!(l.check(300).is_ok());
        assert_eq!(
            rate_state(&resp(
                200,
                &[("ratelimit-remaining", "12"), ("ratelimit-reset", "5")],
                ""
            )),
            Some(RateState {
                remaining: 12,
                reset_at: Some(5)
            })
        );
    }

    #[test]
    fn encodes_paths() {
        assert_eq!(encode("group/sub/proj"), "group%2Fsub%2Fproj");
        assert_eq!(encode("feature/x y"), "feature%2Fx%20y");
        assert_eq!(encode("v1.2_a-b~c"), "v1.2_a-b~c");
    }
}
