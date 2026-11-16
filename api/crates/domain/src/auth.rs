//! Request authorization. `Debug` never prints credentials.

use crate::auth_types::{AwsSigV4Auth, DigestAuth, OAuth1Auth, OAuth2Auth};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyLocation {
    #[default]
    Header,
    Query,
}

#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Auth {
    /// Use the nearest folder / collection auth. Outside a collection this is
    /// the same as [`Auth::None`].
    #[default]
    Inherit,
    None,
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
    },
    ApiKey {
        key: String,
        value: String,
        location: ApiKeyLocation,
    },
    /// Secrets of the boxed configs live in the OS keychain (see
    /// [`Auth::secret_ref`]).
    #[serde(rename = "oauth2")]
    OAuth2(Box<OAuth2Auth>),
    #[serde(rename = "oauth1")]
    OAuth1(Box<OAuth1Auth>),
    Digest(Box<DigestAuth>),
    #[serde(rename = "awsv4")]
    AwsSigV4(Box<AwsSigV4Auth>),
}

impl Auth {
    #[must_use]
    pub const fn kind_label(&self) -> &'static str {
        match self {
            Self::Inherit => "Inherit from parent",
            Self::None => "No Auth",
            Self::Basic { .. } => "Basic Auth",
            Self::Bearer { .. } => "Bearer Token",
            Self::ApiKey { .. } => "API Key",
            Self::OAuth2(_) => "OAuth 2.0",
            Self::OAuth1(_) => "OAuth 1.0",
            Self::Digest(_) => "Digest Auth",
            Self::AwsSigV4(_) => "AWS Signature",
        }
    }

    /// Keychain account holding this auth's secrets; `None` for the types
    /// that keep their values in the request itself.
    #[must_use]
    pub fn secret_ref(&self) -> Option<&str> {
        let r = match self {
            Self::OAuth2(a) => &a.secret_ref,
            Self::OAuth1(a) => &a.secret_ref,
            Self::Digest(a) => &a.secret_ref,
            Self::AwsSigV4(a) => &a.secret_ref,
            _ => return None,
        };
        Some(r.as_str()).filter(|r| !r.is_empty())
    }

    /// The keychain-held fields as `(name, value)`, empty values included.
    #[must_use]
    pub fn secret_fields(&self) -> Vec<(&'static str, &str)> {
        match self {
            Self::OAuth2(a) => vec![("client_secret", &a.client_secret)],
            Self::OAuth1(a) => vec![
                ("consumer_secret", &a.consumer_secret),
                ("token_secret", &a.token_secret),
            ],
            Self::Digest(a) => vec![("password", &a.password)],
            Self::AwsSigV4(a) => vec![
                ("secret_access_key", &a.secret_access_key),
                ("session_token", &a.session_token),
            ],
            _ => Vec::new(),
        }
    }

    /// Sets a field named by [`Auth::secret_fields`]; other names are ignored.
    pub fn set_secret_field(&mut self, name: &str, value: String) {
        let slot = match (self, name) {
            (Self::OAuth2(a), "client_secret") => &mut a.client_secret,
            (Self::OAuth1(a), "consumer_secret") => &mut a.consumer_secret,
            (Self::OAuth1(a), "token_secret") => &mut a.token_secret,
            (Self::Digest(a), "password") => &mut a.password,
            (Self::AwsSigV4(a), "secret_access_key") => &mut a.secret_access_key,
            (Self::AwsSigV4(a), "session_token") => &mut a.session_token,
            _ => return,
        };
        *slot = value;
    }

    /// Applies `f` to every text of the OAuth / Digest / AWS configs where
    /// `{{variables}}` may appear (secrets included). Other types are untouched.
    pub fn map_texts(&mut self, f: &mut dyn FnMut(&mut String)) {
        let texts: Vec<&mut String> = match self {
            Self::OAuth2(a) => vec![
                &mut a.auth_url,
                &mut a.token_url,
                &mut a.client_id,
                &mut a.scope,
                &mut a.header_prefix,
                &mut a.client_secret,
            ],
            Self::OAuth1(a) => vec![
                &mut a.consumer_key,
                &mut a.token,
                &mut a.realm,
                &mut a.consumer_secret,
                &mut a.token_secret,
            ],
            Self::Digest(a) => vec![&mut a.username, &mut a.password],
            Self::AwsSigV4(a) => vec![
                &mut a.access_key_id,
                &mut a.region,
                &mut a.service,
                &mut a.secret_access_key,
                &mut a.session_token,
            ],
            _ => Vec::new(),
        };
        for t in texts {
            f(t);
        }
    }

    /// The texts [`Auth::map_texts`] visits.
    #[must_use]
    pub fn texts(&self) -> Vec<&str> {
        match self {
            Self::OAuth2(a) => vec![
                &a.auth_url,
                &a.token_url,
                &a.client_id,
                &a.scope,
                &a.header_prefix,
                &a.client_secret,
            ],
            Self::OAuth1(a) => vec![
                &a.consumer_key,
                &a.token,
                &a.realm,
                &a.consumer_secret,
                &a.token_secret,
            ],
            Self::Digest(a) => vec![&a.username, &a.password],
            Self::AwsSigV4(a) => vec![
                &a.access_key_id,
                &a.region,
                &a.service,
                &a.secret_access_key,
                &a.session_token,
            ],
            _ => Vec::new(),
        }
        .into_iter()
        .map(String::as_str)
        .collect()
    }

    /// Whether every keychain-held field is empty (not loaded yet, or unset).
    #[must_use]
    pub fn secrets_empty(&self) -> bool {
        self.secret_fields().iter().all(|(_, v)| v.is_empty())
    }

    #[must_use]
    pub const fn is_inherit(&self) -> bool {
        matches!(self, Self::Inherit)
    }
}

impl fmt::Debug for Auth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const R: &str = "***";
        match self {
            Self::Inherit => f.write_str("Inherit"),
            Self::None => f.write_str("None"),
            Self::Basic { username, .. } => f
                .debug_struct("Basic")
                .field("username", username)
                .field("password", &R)
                .finish(),
            Self::Bearer { .. } => f.debug_struct("Bearer").field("token", &R).finish(),
            Self::ApiKey { key, location, .. } => f
                .debug_struct("ApiKey")
                .field("key", key)
                .field("value", &R)
                .field("location", location)
                .finish(),
            Self::OAuth2(a) => a.fmt(f),
            Self::OAuth1(a) => a.fmt(f),
            Self::Digest(a) => a.fmt(f),
            Self::AwsSigV4(a) => a.fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_secrets() {
        let a = Auth::Basic {
            username: "u".into(),
            password: "hunter2".into(),
        };
        let b = Auth::Bearer {
            token: "tok123".into(),
        };
        let c = Auth::ApiKey {
            key: "X-Key".into(),
            value: "sekret".into(),
            location: ApiKeyLocation::Header,
        };
        let s = format!("{a:?}{b:?}{c:?}");
        assert!(!s.contains("hunter2") && !s.contains("tok123") && !s.contains("sekret"));
    }

    #[test]
    fn serde_round_trip() {
        let a = Auth::Bearer { token: "t".into() };
        let json = serde_json::to_string(&a).expect("ser");
        assert_eq!(json, r#"{"type":"bearer","token":"t"}"#);
        assert_eq!(serde_json::from_str::<Auth>(&json).expect("de"), a);
    }

    #[test]
    fn keychain_fields_are_never_serialized_or_printed() {
        let mut a = Auth::AwsSigV4(Box::default());
        a.set_secret_field("secret_access_key", "wJalr-secret".into());
        a.set_secret_field("session_token", "sess-tok".into());
        a.set_secret_field("unknown", "x".into());
        assert!(!a.secrets_empty());
        let json = serde_json::to_string(&a).expect("ser");
        assert!(json.starts_with(r#"{"type":"awsv4""#), "{json}");
        assert!(
            !json.contains("wJalr") && !json.contains("sess-tok"),
            "{json}"
        );
        assert!(!format!("{a:?}").contains("wJalr"));
        let back: Auth = serde_json::from_str(&json).expect("de");
        assert!(back.secrets_empty());
        assert_eq!(back.secret_ref(), a.secret_ref());
        assert!(a.secret_ref().is_some_and(|r| r.len() == 36));

        for (auth, names) in [
            (Auth::OAuth2(Box::default()), vec!["client_secret"]),
            (
                Auth::OAuth1(Box::default()),
                vec!["consumer_secret", "token_secret"],
            ),
            (Auth::Digest(Box::default()), vec!["password"]),
        ] {
            let mut auth = auth;
            for n in &names {
                auth.set_secret_field(n, format!("s3cr3t-{n}"));
            }
            let json = serde_json::to_string(&auth).expect("ser");
            assert!(!json.contains("s3cr3t"), "{json}");
            let got: Vec<_> = auth.secret_fields().into_iter().map(|(n, _)| n).collect();
            assert_eq!(got, names);
            assert_eq!(
                serde_json::from_str::<Auth>(&json)
                    .expect("de")
                    .secret_ref(),
                auth.secret_ref()
            );
        }
        assert!(
            Auth::Basic {
                username: "u".into(),
                password: "p".into()
            }
            .secret_ref()
            .is_none()
        );
    }

    #[test]
    fn oauth2_token_freshness() {
        let t = crate::OAuth2Token {
            access_token: "a".into(),
            expires_at: Some(1_000),
            ..Default::default()
        };
        assert!(t.is_fresh(900) && !t.is_fresh(980));
        let forever = crate::OAuth2Token {
            expires_at: None,
            ..t.clone()
        };
        assert!(forever.is_fresh(i64::MAX - 1));
        assert!(!crate::OAuth2Token::default().is_fresh(0));
    }
}
