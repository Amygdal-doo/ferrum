//! Mock server model: mock responses attached to saved requests, route
//! patterns derived from request URLs, and request → route matching.
//!
//! A route path comes from the saved request's URL: a leading `{{base}}`
//! variable or `scheme://host` is dropped, the query string is ignored, and
//! segments written `:id`, `{id}` or `{{id}}` match any single segment.

use crate::ids::{ItemId, MockId};
use crate::request::{HttpMethod, KeyValue};
use crate::vars::VariableSet;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Port the mock server suggests by default.
pub const DEFAULT_MOCK_PORT: u16 = 4010;
/// Longest accepted artificial delay.
pub const MAX_MOCK_DELAY_MS: u64 = 60_000;

/// One canned response for a saved request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MockResponse {
    pub id: MockId,
    /// The saved request whose method and URL define the route.
    pub item: ItemId,
    pub name: String,
    pub enabled: bool,
    pub status: u16,
    pub headers: Vec<KeyValue>,
    pub body: String,
    pub delay_ms: u64,
    /// Only answer when every active row is in the query (empty value: any value).
    pub match_query: Vec<KeyValue>,
    /// Only answer when every active row is a request header (name case-insensitive).
    pub match_headers: Vec<KeyValue>,
}

impl Default for MockResponse {
    fn default() -> Self {
        Self {
            id: MockId::new(),
            item: ItemId::new(),
            name: String::new(),
            enabled: true,
            status: 200,
            headers: vec![KeyValue::new("Content-Type", "application/json")],
            body: "{}".into(),
            delay_ms: 0,
            match_query: Vec::new(),
            match_headers: Vec::new(),
        }
    }
}

impl MockResponse {
    #[must_use]
    pub fn for_item(item: ItemId) -> Self {
        Self {
            item,
            ..Self::default()
        }
    }
}

/// Mock definitions of one collection (persisted next to it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CollectionMocks {
    pub port: u16,
    /// In priority order within each request.
    pub responses: Vec<MockResponse>,
}

impl Default for CollectionMocks {
    fn default() -> Self {
        Self {
            port: DEFAULT_MOCK_PORT,
            responses: Vec::new(),
        }
    }
}

impl CollectionMocks {
    pub fn for_item(&self, item: ItemId) -> impl Iterator<Item = &MockResponse> {
        self.responses.iter().filter(move |m| m.item == item)
    }
}

/// An enabled mock response bound to its method and path pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockRoute {
    /// Saved request name (for logs and the 404 listing).
    pub name: String,
    pub method: HttpMethod,
    pub path: String,
    pub response: MockResponse,
}

/// An incoming request as the matcher sees it.
#[derive(Debug, Clone, Copy)]
pub struct MockRequest<'a> {
    pub method: &'a str,
    pub path: &'a str,
    /// Raw query string without `?`.
    pub query: &'a str,
    pub headers: &'a [(String, String)],
}

/// The route that answered and the captured path parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockMatch<'a> {
    pub route: &'a MockRoute,
    pub params: Vec<(String, String)>,
}

/// One line of the mock server's live log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockLogEntry {
    pub at: DateTime<Utc>,
    pub method: String,
    /// Path plus `?query` when present.
    pub target: String,
    pub status: u16,
    /// `request name · mock name`; `None` when nothing matched (404).
    pub matched: Option<String>,
}

/// Route path for a saved request URL (see the module docs). `vars` resolve
/// variables after a leading `{{base}}`; unresolved ones stay as parameters.
#[must_use]
pub fn route_path(url: &str, vars: &VariableSet) -> String {
    let url = url.trim();
    let rest = match url.strip_prefix("{{").and_then(|r| r.split_once("}}")) {
        Some((_, after)) => vars.resolve(after),
        None => {
            let resolved = vars.resolve(url);
            match resolved.split_once("://") {
                Some((_, after)) => after.find('/').map_or(String::new(), |i| after[i..].into()),
                None if resolved.starts_with('/') => resolved,
                None => resolved
                    .find('/')
                    .map_or(String::new(), |i| resolved[i..].into()),
            }
        }
    };
    let path = rest.split(['?', '#']).next().unwrap_or_default();
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    format!("/{}", segments.join("/"))
}

/// Parameter name when `segment` is `:name`, `{name}` or `{{name}}`.
fn param_name(segment: &str) -> Option<&str> {
    if let Some(name) = segment.strip_prefix(':') {
        return (!name.is_empty()).then_some(name);
    }
    let inner = segment
        .strip_prefix("{{")
        .and_then(|s| s.strip_suffix("}}"));
    let inner = inner.or_else(|| segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')));
    inner.map(str::trim).filter(|n| !n.is_empty())
}

/// Matches `path` against `pattern`; returns the parameters and the number
/// of literal segments (more literals = more specific).
#[must_use]
pub fn match_path(pattern: &str, path: &str) -> Option<(Vec<(String, String)>, usize)> {
    let pat: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let got: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if pat.len() != got.len() {
        return None;
    }
    let mut params = Vec::new();
    let mut literals = 0;
    for (p, g) in pat.iter().zip(&got) {
        match param_name(p) {
            Some(name) => params.push((name.to_owned(), percent_decode(g))),
            None if p == g => literals += 1,
            None => return None,
        }
    }
    Some((params, literals))
}

/// Decodes `%XX` escapes and `+` (as space); invalid escapes are kept.
#[must_use]
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let escaped = (b == b'%')
            .then(|| s.get(i + 1..i + 3))
            .flatten()
            .filter(|h| h.bytes().all(|c| c.is_ascii_hexdigit()))
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (b, escaped) {
            (_, Some(decoded)) => {
                out.push(decoded);
                i += 3;
            }
            (b'+', None) => {
                out.push(b' ');
                i += 1;
            }
            _ => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Decoded `key=value` pairs of a raw query string.
#[must_use]
pub fn query_pairs(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode(k), percent_decode(v))
        })
        .collect()
}

fn conditions_hold(response: &MockResponse, req: &MockRequest<'_>) -> bool {
    let query = query_pairs(req.query);
    let query_ok = response
        .match_query
        .iter()
        .filter(|c| c.is_active())
        .all(|c| {
            let (key, want) = (c.key.trim(), c.value.trim());
            query
                .iter()
                .any(|(k, v)| k == key && (want.is_empty() || v == want))
        });
    let headers_ok = response
        .match_headers
        .iter()
        .filter(|c| c.is_active())
        .all(|c| {
            let (key, want) = (c.key.trim(), c.value.trim());
            req.headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case(key) && (want.is_empty() || v.trim() == want))
        });
    query_ok && headers_ok
}

/// The best route for `req`: same method, matching path and conditions.
/// Ties go to more literal segments, then more conditions, then list order.
#[must_use]
pub fn find_route<'a>(routes: &'a [MockRoute], req: &MockRequest<'_>) -> Option<MockMatch<'a>> {
    let mut best: Option<((usize, usize), MockMatch<'a>)> = None;
    for route in routes {
        if !route.method.as_str().eq_ignore_ascii_case(req.method) {
            continue;
        }
        let Some((params, literals)) = match_path(&route.path, req.path) else {
            continue;
        };
        if !conditions_hold(&route.response, req) {
            continue;
        }
        let r = &route.response;
        let conditions = r.match_query.iter().chain(&r.match_headers);
        let score = (literals, conditions.filter(|c| c.is_active()).count());
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, MockMatch { route, params }));
        }
    }
    best.map(|(_, m)| m)
}

/// JSON body of the 404 sent when no route matches, listing every route.
#[must_use]
pub fn not_found_body(routes: &[MockRoute], method: &str, path: &str) -> String {
    let listed: Vec<serde_json::Value> = routes
        .iter()
        .map(|r| {
            serde_json::json!({
                "method": r.method.as_str(),
                "path": r.path,
                "request": r.name,
                "mock": r.response.name,
                "status": r.response.status,
            })
        })
        .collect();
    let body = serde_json::json!({
        "error": format!("No mock matches {method} {path}"),
        "hint": "Routes match by method and path (:param / {param} segments match any value), plus any query/header conditions.",
        "routes": listed,
    });
    serde_json::to_string_pretty(&body).unwrap_or_default()
}

#[cfg(test)]
#[path = "mock_tests.rs"]
mod tests;
