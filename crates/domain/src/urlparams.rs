//! Two-way sync between a URL's query string and the params table.
//!
//! Text is kept exactly as typed (no percent-encoding or decoding), so
//! `{{variables}}` survive. Disabled rows live only in the table.

use crate::request::KeyValue;

/// Splits `url` into (before `?`, query, `#fragment` including `#`).
fn split(url: &str) -> (&str, Option<&str>, &str) {
    let (rest, frag) = match url.find('#') {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, ""),
    };
    match rest.find('?') {
        Some(i) => (&rest[..i], Some(&rest[i + 1..]), frag),
        None => (rest, None, frag),
    }
}

/// Parses the query string of `url` into enabled rows.
#[must_use]
pub fn parse_query(url: &str) -> Vec<KeyValue> {
    let (_, query, _) = split(url);
    let Some(query) = query else {
        return Vec::new();
    };
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) => KeyValue::new(k, v),
            None => KeyValue::new(pair, ""),
        })
        .collect()
}

/// URL edited → new params table. Enabled rows come from the URL; disabled
/// rows from `previous` are kept (appended) and description is carried over
/// for rows at the same position with the same key.
#[must_use]
pub fn params_from_url(url: &str, previous: &[KeyValue]) -> Vec<KeyValue> {
    let enabled_prev: Vec<&KeyValue> = previous.iter().filter(|p| p.enabled).collect();
    let mut rows: Vec<KeyValue> = parse_query(url)
        .into_iter()
        .enumerate()
        .map(|(i, mut kv)| {
            if let Some(p) = enabled_prev.get(i).filter(|p| p.key == kv.key) {
                kv.description.clone_from(&p.description);
            }
            kv
        })
        .collect();
    rows.extend(previous.iter().filter(|p| !p.enabled).cloned());
    rows
}

/// Params table edited → new URL. Enabled rows with a key or value are
/// written; an empty table removes the `?`.
#[must_use]
pub fn url_with_params(url: &str, params: &[KeyValue]) -> String {
    let (base, _, frag) = split(url);
    let query: Vec<String> = params
        .iter()
        .filter(|p| p.enabled && !(p.key.is_empty() && p.value.is_empty()))
        .map(|p| {
            if p.value.is_empty() {
                p.key.clone()
            } else {
                format!("{}={}", p.key, p.value)
            }
        })
        .collect();
    if query.is_empty() {
        format!("{base}{frag}")
    } else {
        format!("{base}?{}{frag}", query.join("&"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_query() {
        let p = parse_query("https://x/a?q=1&flag&e=&v={{v}}#top");
        let pairs: Vec<_> = p
            .iter()
            .map(|k| (k.key.as_str(), k.value.as_str()))
            .collect();
        assert_eq!(pairs, [("q", "1"), ("flag", ""), ("e", ""), ("v", "{{v}}")]);
    }

    #[test]
    fn round_trip_keeps_disabled_rows() {
        let mut off = KeyValue::new("debug", "1");
        off.enabled = false;
        let rows = params_from_url("https://x?a=1&b=2", &[off.clone()]);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2], off);
        assert_eq!(
            url_with_params("https://x?old=1#f", &rows),
            "https://x?a=1&b=2#f"
        );
    }

    #[test]
    fn empty_table_drops_question_mark() {
        assert_eq!(url_with_params("https://x/p?a=1", &[]), "https://x/p");
        assert!(parse_query("https://x/p").is_empty());
    }
}
