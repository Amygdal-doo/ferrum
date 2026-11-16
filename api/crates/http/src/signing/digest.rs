//! HTTP Digest access authentication (RFC 7616, RFC 2617 compatible):
//! parses a `WWW-Authenticate: Digest …` challenge and answers it.

use super::hex;
use md5::{Digest as _, Md5};
use sha2::Sha256;

/// A parsed `Digest` challenge.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Challenge {
    pub realm: String,
    pub nonce: String,
    pub opaque: Option<String>,
    /// `MD5`, `MD5-sess`, `SHA-256` or `SHA-256-sess` (default `MD5`).
    pub algorithm: String,
    /// Whether the server offered `qop=auth` (else the RFC 2069 form is used).
    pub qop_auth: bool,
}

/// The first `Digest` challenge among `WWW-Authenticate` values.
#[must_use]
pub fn parse_challenge<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<Challenge> {
    values.into_iter().find_map(|v| {
        let v = v.trim();
        let rest = v
            .get(..7)
            .filter(|s| s.eq_ignore_ascii_case("digest "))
            .map(|_| &v[7..])?;
        let mut c = Challenge {
            algorithm: "MD5".to_owned(),
            ..Challenge::default()
        };
        for (k, val) in params(rest) {
            match k.to_ascii_lowercase().as_str() {
                "realm" => c.realm = val,
                "nonce" => c.nonce = val,
                "opaque" => c.opaque = Some(val),
                "algorithm" => c.algorithm = val,
                "qop" => c.qop_auth = val.split(',').any(|q| q.trim() == "auth"),
                _ => {}
            }
        }
        (!c.nonce.is_empty()).then_some(c)
    })
}

/// `key=value` / `key="quoted, value"` pairs separated by commas.
fn params(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| *c == ',' || c.is_whitespace()) {
            chars.next();
        }
        let key: String = chars.by_ref().take_while(|c| *c != '=').collect();
        if key.trim().is_empty() {
            break;
        }
        let mut value = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => value.extend(chars.next()),
                    '"' => break,
                    c => value.push(c),
                }
            }
        } else {
            value = chars.by_ref().take_while(|c| *c != ',').collect();
        }
        out.push((key.trim().to_owned(), value.trim().to_owned()));
    }
    out
}

fn hash(algorithm: &str, data: &str) -> String {
    if algorithm.to_ascii_uppercase().starts_with("SHA-256") {
        hex(&Sha256::digest(data.as_bytes()))
    } else {
        hex(&Md5::digest(data.as_bytes()))
    }
}

/// The `Authorization` value answering `c` for `method uri` (`uri` is the
/// request target: path and query).
#[must_use]
pub fn authorization(
    c: &Challenge,
    username: &str,
    password: &str,
    method: &str,
    uri: &str,
    cnonce: &str,
    nc: u32,
) -> String {
    let alg = c.algorithm.as_str();
    let mut ha1 = hash(alg, &format!("{username}:{}:{password}", c.realm));
    if alg.to_ascii_lowercase().ends_with("-sess") {
        ha1 = hash(alg, &format!("{ha1}:{}:{cnonce}", c.nonce));
    }
    let ha2 = hash(alg, &format!("{method}:{uri}"));
    let nc = format!("{nc:08x}");
    let response = if c.qop_auth {
        hash(alg, &format!("{ha1}:{}:{nc}:{cnonce}:auth:{ha2}", c.nonce))
    } else {
        hash(alg, &format!("{ha1}:{}:{ha2}", c.nonce))
    };
    let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut out = format!(
        "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", algorithm={alg}, response=\"{response}\"",
        q(username),
        q(&c.realm),
        q(&c.nonce),
        q(uri)
    );
    if c.qop_auth {
        out.push_str(&format!(", qop=auth, nc={nc}, cnonce=\"{cnonce}\""));
    }
    if let Some(opaque) = &c.opaque {
        out.push_str(&format!(", opaque=\"{}\"", q(opaque)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 2617 §3.5 example.
    #[test]
    fn rfc_2617_example() {
        let c = parse_challenge([
            "Basic realm=\"x\"",
            r#"Digest realm="testrealm@host.com", qop="auth,auth-int", nonce="dcd98b7102dd2f0e8b11d0f600bfb0c093", opaque="5ccc069c403ebaf9f0171e9517f40e41""#,
        ])
        .expect("challenge");
        assert_eq!(c.realm, "testrealm@host.com");
        assert!(c.qop_auth);
        let h = authorization(
            &c,
            "Mufasa",
            "Circle Of Life",
            "GET",
            "/dir/index.html",
            "0a4f113b",
            1,
        );
        assert!(
            h.contains(r#"response="6629fae49393a05397450978507c4ef1""#),
            "{h}"
        );
        assert!(
            h.contains("nc=00000001") && h.contains(r#"opaque="5ccc069c403ebaf9f0171e9517f40e41""#)
        );
    }

    /// RFC 7616 §3.9.1 example (SHA-256).
    #[test]
    fn rfc_7616_sha256_example() {
        let c = parse_challenge([
            r#"Digest realm="http-auth@example.org", qop="auth, auth-int", algorithm=SHA-256, nonce="7ypf/xlj9XXwfDPEoM4URrv/xwf94BcCAzFZH4GiTo0v", opaque="FQhe/qaU925kfnzjCev0ciny7QMkPqMAFRtzCUYo5tdS""#,
        ])
        .expect("challenge");
        let h = authorization(
            &c,
            "Mufasa",
            "Circle of Life",
            "GET",
            "/dir/index.html",
            "f2/wE4q74E6zIJEtWaHKaf5wv/H5QzzpXusqGemxURZJ",
            1,
        );
        assert!(
            h.contains(
                r#"response="753927fa0e85d155564e2e272a28d1802ca10daf4496794697cf8db5856cb6c1""#
            ),
            "{h}"
        );
    }

    #[test]
    fn no_digest_challenge() {
        assert!(parse_challenge(["Basic realm=\"x\"", "Digest realm=\"r\""]).is_none());
        let c = parse_challenge(["digest nonce=abc, realm=r"]).expect("lenient");
        assert!(!c.qop_auth);
        let h = authorization(&c, "u", "p", "GET", "/", "c", 1);
        assert!(!h.contains("qop=") && !h.contains("opaque"));
    }
}
