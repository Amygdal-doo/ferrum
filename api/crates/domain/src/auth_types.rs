//! Settings of the auth types that sign or fetch credentials: OAuth 2.0,
//! OAuth 1.0a, Digest and AWS Signature v4.
//!
//! Their secrets (client / consumer / token secrets, passwords, AWS secret
//! keys) are never serialized: each config carries a `secret_ref` naming its
//! OS keychain entry, and the secret fields are filled from the keychain in
//! memory only. `Debug` never prints them.

use serde::{Deserialize, Serialize};
use std::fmt;

fn new_ref() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuth2Grant {
    /// Browser sign-in with PKCE, redirected to a loopback listener.
    #[default]
    AuthorizationCode,
    ClientCredentials,
}

/// How the client authenticates at the token endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientAuthentication {
    /// `client_id` / `client_secret` in the form body.
    #[default]
    Body,
    /// HTTP Basic with the client id and secret.
    BasicHeader,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OAuth2Auth {
    pub grant: OAuth2Grant,
    pub auth_url: String,
    pub token_url: String,
    pub client_id: String,
    /// Space-separated scopes.
    pub scope: String,
    /// Port of the redirect `http://127.0.0.1:{port}/callback`; 0 picks a
    /// free port (register a fixed one if the provider needs exact URIs).
    pub redirect_port: u16,
    pub pkce: bool,
    pub client_auth: ClientAuthentication,
    /// `Authorization: {prefix} {token}`.
    pub header_prefix: String,
    pub secret_ref: String,
    #[serde(skip_serializing)]
    pub client_secret: String,
}

impl Default for OAuth2Auth {
    fn default() -> Self {
        Self {
            grant: OAuth2Grant::default(),
            auth_url: String::new(),
            token_url: String::new(),
            client_id: String::new(),
            scope: String::new(),
            redirect_port: 0,
            pkce: true,
            client_auth: ClientAuthentication::default(),
            header_prefix: "Bearer".to_owned(),
            secret_ref: new_ref(),
            client_secret: String::new(),
        }
    }
}

impl OAuth2Auth {
    /// The redirect URI sent to the provider for `port`.
    #[must_use]
    pub fn redirect_uri(port: u16) -> String {
        format!("http://127.0.0.1:{port}/callback")
    }
}

/// OAuth 1.0a with HMAC-SHA1 signatures, sent in the `Authorization` header.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OAuth1Auth {
    pub consumer_key: String,
    /// Access token (empty for two-legged requests).
    pub token: String,
    pub realm: String,
    pub secret_ref: String,
    #[serde(skip_serializing)]
    pub consumer_secret: String,
    #[serde(skip_serializing)]
    pub token_secret: String,
}

impl Default for OAuth1Auth {
    fn default() -> Self {
        Self {
            consumer_key: String::new(),
            token: String::new(),
            realm: String::new(),
            secret_ref: new_ref(),
            consumer_secret: String::new(),
            token_secret: String::new(),
        }
    }
}

/// HTTP Digest (RFC 7616): the first request's `401` challenge is answered.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DigestAuth {
    pub username: String,
    pub secret_ref: String,
    #[serde(skip_serializing)]
    pub password: String,
}

impl Default for DigestAuth {
    fn default() -> Self {
        Self {
            username: String::new(),
            secret_ref: new_ref(),
            password: String::new(),
        }
    }
}

/// AWS Signature Version 4.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AwsSigV4Auth {
    pub access_key_id: String,
    pub region: String,
    /// e.g. `s3`, `execute-api`, `sts`.
    pub service: String,
    pub secret_ref: String,
    #[serde(skip_serializing)]
    pub secret_access_key: String,
    /// Temporary credentials only.
    #[serde(skip_serializing)]
    pub session_token: String,
}

impl Default for AwsSigV4Auth {
    fn default() -> Self {
        Self {
            access_key_id: String::new(),
            region: "us-east-1".to_owned(),
            service: String::new(),
            secret_ref: new_ref(),
            secret_access_key: String::new(),
            session_token: String::new(),
        }
    }
}

const R: &str = "***";

impl fmt::Debug for OAuth2Auth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuth2")
            .field("grant", &self.grant)
            .field("client_id", &self.client_id)
            .field("client_secret", &R)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for OAuth1Auth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuth1")
            .field("consumer_key", &self.consumer_key)
            .field("secrets", &R)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for DigestAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Digest")
            .field("username", &self.username)
            .field("password", &R)
            .finish()
    }
}

impl fmt::Debug for AwsSigV4Auth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AwsSigV4")
            .field("access_key_id", &self.access_key_id)
            .field("region", &self.region)
            .field("service", &self.service)
            .field("secret_access_key", &R)
            .finish_non_exhaustive()
    }
}

/// An OAuth 2.0 token as cached per auth and environment (keychain only).
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OAuth2Token {
    pub access_token: String,
    pub token_type: String,
    pub refresh_token: Option<String>,
    /// Unix seconds; `None` when the provider gave no `expires_in`.
    pub expires_at: Option<i64>,
    pub scope: String,
}

impl OAuth2Token {
    /// Usable at `now` (unix seconds), with 30 s of slack for clock skew
    /// and the request's own travel time.
    #[must_use]
    pub fn is_fresh(&self, now: i64) -> bool {
        !self.access_token.is_empty() && self.expires_at.is_none_or(|t| t - 30 > now)
    }
}

impl fmt::Debug for OAuth2Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuth2Token")
            .field("token_type", &self.token_type)
            .field("expires_at", &self.expires_at)
            .field("has_refresh_token", &self.refresh_token.is_some())
            .finish_non_exhaustive()
    }
}
