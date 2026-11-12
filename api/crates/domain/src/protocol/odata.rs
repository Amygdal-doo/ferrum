//! OData: the service root URL, an entity set (or any resource path) and
//! the system query options, built into one URL.

use super::{append_query, default_header};
use crate::request::HttpRequest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ODataSpec {
    /// Resource path after the service root, e.g. `People` or `People('ann')`.
    pub entity_set: String,
    pub filter: String,
    pub select: String,
    pub expand: String,
    pub orderby: String,
    /// Text so it can hold `{{variables}}`; blank is omitted.
    pub top: String,
    pub skip: String,
    pub search: String,
    /// `$count=true`.
    pub count: bool,
}

/// Encodes an OData option value: spaces and URL delimiters are escaped,
/// while `$ , ( ) ' / :` and the like stay readable.
#[must_use]
pub fn encode_option(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        let keep = b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'-' | b'.' | b'_' | b'~' | b'$' | b',' | b'(' | b')' | b'\'' | b'/' | b':' | b'*'
            );
        if keep {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `(name, value)` of every non-blank option, in the conventional order.
#[must_use]
pub fn options(spec: &ODataSpec) -> Vec<(&'static str, String)> {
    let text = [
        ("$filter", &spec.filter),
        ("$select", &spec.select),
        ("$expand", &spec.expand),
        ("$orderby", &spec.orderby),
        ("$top", &spec.top),
        ("$skip", &spec.skip),
        ("$search", &spec.search),
    ];
    let mut out: Vec<(&'static str, String)> = text
        .into_iter()
        .filter(|(_, v)| !v.trim().is_empty())
        .map(|(k, v)| (k, v.trim().to_owned()))
        .collect();
    if spec.count {
        out.push(("$count", "true".to_owned()));
    }
    out
}

/// `root/entity_set?$options` (keeps any query already in `root`).
#[must_use]
pub fn build_url(root: &str, spec: &ODataSpec) -> String {
    let (base, query) = root
        .split_once('?')
        .map_or((root, None), |(b, q)| (b, Some(q)));
    let path = spec.entity_set.trim().trim_start_matches('/');
    let mut url = if path.is_empty() {
        base.to_owned()
    } else {
        format!("{}/{path}", base.trim_end_matches('/'))
    };
    if let Some(q) = query.filter(|q| !q.is_empty()) {
        url = format!("{url}?{q}");
    }
    let pairs: Vec<(String, String)> = options(spec)
        .into_iter()
        .map(|(k, v)| (k.to_owned(), encode_option(&v)))
        .collect();
    append_query(&url, &pairs)
}

/// `$metadata` document of the service root in `url`.
#[must_use]
pub fn metadata_url(root: &str) -> String {
    let base = root.split(['?', '#']).next().unwrap_or(root);
    format!("{}/$metadata", base.trim_end_matches('/'))
}

pub(super) fn lower(request: &HttpRequest) -> HttpRequest {
    let mut out = request.clone();
    out.url = build_url(&request.url, &request.specs.odata);
    default_header(&mut out.headers, "Accept", "application/json");
    out
}
