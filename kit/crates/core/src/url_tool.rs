//! Percent-encoding and URL parsing.

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};

/// Characters left as-is by `encodeURIComponent`: `A-Z a-z 0-9 - _ . ! ~ * ' ( )`.
const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

/// Like JavaScript's `encodeURIComponent`.
pub fn encode_component(text: &str) -> String {
    utf8_percent_encode(text, COMPONENT).to_string()
}

/// Decodes `%XX` escapes; with `plus_as_space`, `+` becomes a space (form
/// encoding). Fails if the result is not UTF-8.
pub fn decode_component(text: &str, plus_as_space: bool) -> Result<String, String> {
    let text = if plus_as_space {
        text.replace('+', " ")
    } else {
        text.to_string()
    };
    percent_decode_str(&text)
        .decode_utf8()
        .map(|s| s.into_owned())
        .map_err(|_| "Decoded bytes are not valid UTF-8".to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlParts {
    pub scheme: String,
    pub username: String,
    pub password: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub path: String,
    /// Raw query string without `?`.
    pub query: Option<String>,
    /// Decoded query parameters in order (repeats kept).
    pub params: Vec<(String, String)>,
    pub fragment: Option<String>,
}

impl UrlParts {
    /// Rows for a "part / value" table, skipping empty parts.
    pub fn rows(&self) -> Vec<(&'static str, String)> {
        let mut rows = vec![("Scheme", self.scheme.clone())];
        if !self.username.is_empty() {
            rows.push(("Username", self.username.clone()));
        }
        if let Some(password) = &self.password {
            rows.push(("Password", password.clone()));
        }
        if let Some(host) = &self.host {
            rows.push(("Host", host.clone()));
        }
        if let Some(port) = self.port {
            rows.push(("Port", port.to_string()));
        }
        rows.push(("Path", self.path.clone()));
        if let Some(query) = &self.query {
            rows.push(("Query", query.clone()));
        }
        if let Some(fragment) = &self.fragment {
            rows.push(("Fragment", fragment.clone()));
        }
        rows
    }
}

/// Parses an absolute URL. The port is shown only when it is explicit and not
/// the scheme's default.
pub fn parse_url(input: &str) -> Result<UrlParts, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Paste a URL".into());
    }
    let url = url::Url::parse(input).map_err(|e| match e {
        url::ParseError::RelativeUrlWithoutBase => {
            "Not an absolute URL (add a scheme such as https://)".to_string()
        }
        other => format!("Invalid URL: {other}"),
    })?;
    Ok(UrlParts {
        scheme: url.scheme().to_string(),
        username: url.username().to_string(),
        password: url.password().map(str::to_string),
        host: url.host_str().map(str::to_string),
        port: url.port(),
        path: url.path().to_string(),
        query: url.query().map(str::to_string),
        params: url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect(),
        fragment: url.fragment().map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_like_encode_uri_component() {
        assert_eq!(encode_component("a b&c=d/é"), "a%20b%26c%3Dd%2F%C3%A9");
        assert_eq!(encode_component("-_.!~*'()"), "-_.!~*'()");
        assert_eq!(encode_component(""), "");
    }

    #[test]
    fn decodes_and_roundtrips() {
        assert_eq!(decode_component("a%20b%26c", false).unwrap(), "a b&c");
        assert_eq!(decode_component("a+b", false).unwrap(), "a+b");
        assert_eq!(decode_component("a+b%2B", true).unwrap(), "a b+");
        let text = "naïve ✓ 100%";
        assert_eq!(
            decode_component(&encode_component(text), false).unwrap(),
            text
        );
    }

    #[test]
    fn decode_leaves_malformed_escapes_and_rejects_bad_utf8() {
        assert_eq!(decode_component("100%", false).unwrap(), "100%");
        assert_eq!(decode_component("%zz", false).unwrap(), "%zz");
        assert!(decode_component("%ff", false).is_err());
    }

    #[test]
    fn parses_full_url() {
        let parts =
            parse_url("https://user:pw@example.com:8443/a/b?x=1&y=two%20words&x=3#frag").unwrap();
        assert_eq!(parts.scheme, "https");
        assert_eq!(parts.username, "user");
        assert_eq!(parts.password.as_deref(), Some("pw"));
        assert_eq!(parts.host.as_deref(), Some("example.com"));
        assert_eq!(parts.port, Some(8443));
        assert_eq!(parts.path, "/a/b");
        assert_eq!(parts.query.as_deref(), Some("x=1&y=two%20words&x=3"));
        assert_eq!(
            parts.params,
            vec![
                ("x".into(), "1".into()),
                ("y".into(), "two words".into()),
                ("x".into(), "3".into())
            ]
        );
        assert_eq!(parts.fragment.as_deref(), Some("frag"));
        assert_eq!(parts.rows().len(), 8);
    }

    #[test]
    fn default_port_and_minimal_url() {
        let parts = parse_url("http://example.com:80").unwrap();
        assert_eq!(parts.port, None);
        assert_eq!(parts.path, "/");
        assert!(parts.params.is_empty());
        let rows = parts.rows();
        assert_eq!(
            rows.iter().map(|r| r.0).collect::<Vec<_>>(),
            ["Scheme", "Host", "Path"]
        );
    }

    #[test]
    fn rejects_relative_and_empty() {
        assert!(parse_url("").is_err());
        assert!(parse_url("/just/a/path").unwrap_err().contains("absolute"));
        assert!(parse_url("http://exa mple.com").is_err());
    }
}
