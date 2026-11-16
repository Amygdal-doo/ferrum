//! OAuth 1.0a HMAC-SHA1 signatures (RFC 5849 §3.4), sent in the
//! `Authorization` header.

use super::{enc, hmac_sha1};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ferrumapi_domain::OAuth1Auth;
use url::Url;

/// The `Authorization` header value for a request. `form` holds the
/// decoded pairs of an `application/x-www-form-urlencoded` body (they are
/// signed too); `nonce` / `timestamp` are passed in so tests are exact.
#[must_use]
pub fn authorization(
    method: &str,
    url: &Url,
    form: &[(String, String)],
    cfg: &OAuth1Auth,
    nonce: &str,
    timestamp: i64,
) -> String {
    let mut oauth: Vec<(&str, String)> = vec![
        ("oauth_consumer_key", cfg.consumer_key.clone()),
        ("oauth_nonce", nonce.to_owned()),
        ("oauth_signature_method", "HMAC-SHA1".to_owned()),
        ("oauth_timestamp", timestamp.to_string()),
        ("oauth_version", "1.0".to_owned()),
    ];
    if !cfg.token.is_empty() {
        oauth.push(("oauth_token", cfg.token.clone()));
    }
    let mut params: Vec<(String, String)> = oauth
        .iter()
        .map(|(k, v)| (enc(k), enc(v)))
        .chain(url.query_pairs().map(|(k, v)| (enc(&k), enc(&v))))
        .chain(form.iter().map(|(k, v)| (enc(k), enc(v))))
        .collect();
    params.sort();
    let normalized = params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let base = format!(
        "{}&{}&{}",
        method.to_ascii_uppercase(),
        enc(&base_url(url)),
        enc(&normalized)
    );
    let key = format!("{}&{}", enc(&cfg.consumer_secret), enc(&cfg.token_secret));
    let signature = STANDARD.encode(hmac_sha1(key.as_bytes(), base.as_bytes()));
    oauth.push(("oauth_signature", signature));
    oauth.sort_by(|a, b| a.0.cmp(b.0));
    let mut parts: Vec<String> = Vec::new();
    if !cfg.realm.is_empty() {
        parts.push(format!("realm=\"{}\"", enc(&cfg.realm)));
    }
    parts.extend(oauth.iter().map(|(k, v)| format!("{k}=\"{}\"", enc(v))));
    format!("OAuth {}", parts.join(", "))
}

/// `scheme://host[:port]/path`, lower-cased scheme and host, default port
/// dropped, no query or fragment (§3.4.1.2).
fn base_url(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
    format!("{}://{host}{port}{}", url.scheme(), url.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worked example of Twitter's "Creating a signature" guide.
    #[test]
    fn matches_the_published_example() {
        let cfg = OAuth1Auth {
            consumer_key: "xvz1evFS4wEEPTGEFPHBog".into(),
            consumer_secret: "kAcSOqF21Fu85e7zjz7ZN2U4ZRhfV3WpwPAoE3Z7kBw".into(),
            token: "370773112-GmHxMAgYyLbNEtIKZeRNFsMKPR9EyMZeS9weJAEb".into(),
            token_secret: "LswwdoUaIvS8ltyTt5jkRh4J50vUPVVHtR2YPi5kE".into(),
            ..OAuth1Auth::default()
        };
        let url =
            Url::parse("https://api.twitter.com/1.1/statuses/update.json?include_entities=true")
                .expect("url");
        let form = vec![(
            "status".to_owned(),
            "Hello Ladies + Gentlemen, a signed OAuth request!".to_owned(),
        )];
        let header = authorization(
            "post",
            &url,
            &form,
            &cfg,
            "kYjzVBB8Y0ZFabxSWbWovY3uYSQ2pTgmZeNu2VS4cg",
            1_318_622_958,
        );
        assert!(
            header.contains(r#"oauth_signature="hCtSmYh%2BiHYCEqBWrE7C7hYmtUk%3D""#),
            "{header}"
        );
        assert!(header.starts_with("OAuth oauth_consumer_key=\"xvz1evFS4wEEPTGEFPHBog\""));
        assert!(
            header.contains(r#"oauth_token="370773112-GmHxMAgYyLbNEtIKZeRNFsMKPR9EyMZeS9weJAEb""#)
        );
    }

    #[test]
    fn two_legged_with_realm_and_port() {
        let cfg = OAuth1Auth {
            consumer_key: "key".into(),
            consumer_secret: "secret".into(),
            realm: "Photos".into(),
            ..OAuth1Auth::default()
        };
        let url = Url::parse("HTTP://Example.COM:8080/p?b=2&a=1").expect("url");
        let header = authorization("GET", &url, &[], &cfg, "n", 1);
        assert!(header.starts_with("OAuth realm=\"Photos\", "), "{header}");
        assert!(!header.contains("oauth_token="));
        assert_eq!(base_url(&url), "http://example.com:8080/p");
    }
}
