//! `Set-Cookie` header parsing (RFC 6265, lenient). Attribute names are
//! case-insensitive; unknown attributes are ignored.

use ferrumapi_domain::ResponseCookie;

/// Parses one `Set-Cookie` header value. Returns `None` for an empty header
/// or a cookie without a name.
#[must_use]
pub fn parse_set_cookie(header: &str) -> Option<ResponseCookie> {
    let mut parts = header.split(';');
    let first = parts.next()?.trim();
    let (name, value) = match first.split_once('=') {
        Some((n, v)) => (n.trim(), v.trim()),
        None => (first, ""),
    };
    if name.is_empty() {
        return None;
    }
    let mut cookie = ResponseCookie {
        name: name.to_owned(),
        value: value.to_owned(),
        ..ResponseCookie::default()
    };
    let mut max_age: Option<String> = None;
    for attr in parts {
        let (key, val) = match attr.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => (attr.trim(), ""),
        };
        match key.to_ascii_lowercase().as_str() {
            "domain" if !val.is_empty() => {
                cookie.domain = Some(val.trim_start_matches('.').to_owned());
            }
            "path" if !val.is_empty() => cookie.path = Some(val.to_owned()),
            "expires" if !val.is_empty() => cookie.expires = Some(val.to_owned()),
            "max-age" if !val.is_empty() => max_age = Some(format!("Max-Age={val}")),
            "httponly" => cookie.http_only = true,
            "secure" => cookie.secure = true,
            _ => {}
        }
    }
    // Max-Age takes precedence over Expires (RFC 6265 §5.3 step 3).
    if max_age.is_some() {
        cookie.expires = max_age;
    }
    Some(cookie)
}

/// Parses every `Set-Cookie` header value, skipping invalid ones.
pub fn parse_set_cookies<'a>(headers: impl IntoIterator<Item = &'a str>) -> Vec<ResponseCookie> {
    headers.into_iter().filter_map(parse_set_cookie).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_attributes_case_insensitive() {
        let c = parse_set_cookie(
            "sid=abc123; DOMAIN=.example.com; path=/app; Expires=Wed, 21 Oct 2026 07:28:00 GMT; httponly; SECURE; SameSite=Lax",
        )
        .expect("cookie");
        assert_eq!(c.name, "sid");
        assert_eq!(c.value, "abc123");
        assert_eq!(c.domain.as_deref(), Some("example.com"));
        assert_eq!(c.path.as_deref(), Some("/app"));
        assert_eq!(c.expires.as_deref(), Some("Wed, 21 Oct 2026 07:28:00 GMT"));
        assert!(c.http_only && c.secure);
    }

    #[test]
    fn max_age_wins_over_expires() {
        let c = parse_set_cookie("a=1; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Max-Age=3600")
            .expect("cookie");
        assert_eq!(c.expires.as_deref(), Some("Max-Age=3600"));
        assert!(!c.http_only && !c.secure);
        assert!(c.domain.is_none() && c.path.is_none());
    }

    #[test]
    fn value_with_equals_and_empty_value() {
        let c = parse_set_cookie("tok=a=b==; Path=/").expect("cookie");
        assert_eq!((c.name.as_str(), c.value.as_str()), ("tok", "a=b=="));
        let c = parse_set_cookie("empty=").expect("cookie");
        assert_eq!(c.value, "");
        let c = parse_set_cookie("flag").expect("cookie");
        assert_eq!((c.name.as_str(), c.value.as_str()), ("flag", ""));
    }

    #[test]
    fn rejects_nameless_and_parses_many() {
        assert!(parse_set_cookie("").is_none());
        assert!(parse_set_cookie("=value").is_none());
        let all = parse_set_cookies(["a=1", "", "b=2; Secure"]);
        assert_eq!(all.len(), 2);
        assert!(all[1].secure);
    }
}
