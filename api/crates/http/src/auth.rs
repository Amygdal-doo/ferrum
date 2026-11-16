//! Applies [`Auth`] to the outgoing header map / URL. An explicit user header
//! with the same name always wins over generated auth.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ferrumapi_domain::{ApiKeyLocation, Auth, ExecError};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use url::Url;

/// Adds the auth header or query pair. `Inherit` is treated as `None`
/// (the caller resolves inheritance before executing). OAuth 1, AWS and
/// Digest are applied to the built request (`signing`); OAuth 2 reaches
/// here already turned into a token header by the application layer.
pub(crate) fn apply_auth(
    auth: &Auth,
    headers: &mut HeaderMap,
    url: &mut Url,
) -> Result<(), ExecError> {
    match auth {
        Auth::Inherit
        | Auth::None
        | Auth::OAuth2(_)
        | Auth::OAuth1(_)
        | Auth::Digest(_)
        | Auth::AwsSigV4(_) => Ok(()),
        Auth::Basic { username, password } => {
            let encoded = STANDARD.encode(format!("{username}:{password}"));
            set_if_absent(headers, AUTHORIZATION, &format!("Basic {encoded}"))
        }
        Auth::Bearer { token } => {
            set_if_absent(headers, AUTHORIZATION, &format!("Bearer {}", token.trim()))
        }
        Auth::ApiKey {
            key,
            value,
            location,
        } => {
            let key = key.trim();
            if key.is_empty() {
                return Ok(());
            }
            match location {
                ApiKeyLocation::Header => {
                    let name = HeaderName::from_bytes(key.as_bytes()).map_err(|_| {
                        ExecError::InvalidRequest(format!("invalid API key header name {key:?}"))
                    })?;
                    set_if_absent(headers, name, value)
                }
                ApiKeyLocation::Query => {
                    url.query_pairs_mut().append_pair(key, value);
                    Ok(())
                }
            }
        }
    }
}

fn set_if_absent(headers: &mut HeaderMap, name: HeaderName, value: &str) -> Result<(), ExecError> {
    if headers.contains_key(&name) {
        return Ok(());
    }
    let mut value = HeaderValue::from_str(value).map_err(|_| {
        ExecError::InvalidRequest(format!("auth value for {name} contains invalid characters"))
    })?;
    value.set_sensitive(true);
    headers.insert(name, value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(auth: &Auth, headers: &mut HeaderMap) -> Url {
        let mut url = Url::parse("http://h/p?a=1").expect("url");
        apply_auth(auth, headers, &mut url).expect("apply");
        url
    }

    fn header(h: &HeaderMap, name: &str) -> Option<String> {
        h.get(name).map(|v| v.to_str().expect("ascii").to_owned())
    }

    #[test]
    fn basic_and_bearer() {
        let mut h = HeaderMap::new();
        let basic = Auth::Basic {
            username: "user".into(),
            password: "pass".into(),
        };
        run(&basic, &mut h);
        assert_eq!(
            header(&h, "authorization").as_deref(),
            Some("Basic dXNlcjpwYXNz")
        );
        assert!(h.get(AUTHORIZATION).expect("set").is_sensitive());
        let mut h = HeaderMap::new();
        run(
            &Auth::Bearer {
                token: "tok".into(),
            },
            &mut h,
        );
        assert_eq!(header(&h, "authorization").as_deref(), Some("Bearer tok"));
    }

    #[test]
    fn inherit_and_none_do_nothing() {
        for auth in [Auth::Inherit, Auth::None] {
            let mut h = HeaderMap::new();
            let url = run(&auth, &mut h);
            assert!(h.is_empty());
            assert_eq!(url.as_str(), "http://h/p?a=1");
        }
    }

    #[test]
    fn user_authorization_header_wins() {
        let mut h = HeaderMap::new();
        h.insert(AUTHORIZATION, HeaderValue::from_static("Custom x"));
        run(
            &Auth::Bearer {
                token: "tok".into(),
            },
            &mut h,
        );
        assert_eq!(header(&h, "authorization").as_deref(), Some("Custom x"));
        assert_eq!(h.get_all(AUTHORIZATION).iter().count(), 1);
    }

    #[test]
    fn api_key_header_and_query() {
        let mut h = HeaderMap::new();
        let hdr = Auth::ApiKey {
            key: "X-Api-Key".into(),
            value: "secret".into(),
            location: ApiKeyLocation::Header,
        };
        run(&hdr, &mut h);
        assert_eq!(header(&h, "x-api-key").as_deref(), Some("secret"));
        let mut h = HeaderMap::new();
        let query = Auth::ApiKey {
            key: "api key".into(),
            value: "a&b".into(),
            location: ApiKeyLocation::Query,
        };
        let url = run(&query, &mut h);
        assert!(h.is_empty());
        assert_eq!(url.query(), Some("a=1&api+key=a%26b"));
    }

    #[test]
    fn invalid_api_key_header_name_is_invalid_request() {
        let mut url = Url::parse("http://h").expect("url");
        let bad = Auth::ApiKey {
            key: "bad header".into(),
            value: "v".into(),
            location: ApiKeyLocation::Header,
        };
        let err = apply_auth(&bad, &mut HeaderMap::new(), &mut url).expect_err("invalid");
        assert!(matches!(err, ExecError::InvalidRequest(_)));
    }
}
