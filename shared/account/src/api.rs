//! The account server's JSON API (`/api/v1/…`) and the browser's
//! `/oauth/authorize` URL, as in the accounts contract v1.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::{AccessToken, AccountError, App, AuthCode, RefreshToken, Result};

const TIMEOUT: Duration = Duration::from_secs(20);
const USER_AGENT: &str = concat!("ferrum-account/", env!("CARGO_PKG_VERSION"));

/// The signed-in user (`GET /api/v1/me`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Me {
    pub id: String,
    pub username: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    /// `{SITE}/account`.
    pub account_url: String,
    pub organizations: Vec<Organization>,
}

impl Me {
    /// The name to show: the full name, else the username.
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.name
            .as_deref()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(&self.username)
    }

    /// One or two letters for an avatar placeholder.
    #[must_use]
    pub fn initials(&self) -> String {
        let initials: String = self
            .display_name()
            .split_whitespace()
            .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
            .take(2)
            .flat_map(char::to_uppercase)
            .collect();
        if initials.is_empty() {
            "?".into()
        } else {
            initials
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Organization {
    pub id: String,
    pub name: String,
    /// `free`, …
    pub plan: String,
    pub personal: bool,
    /// `owner`, `admin` or `member`.
    pub role: String,
}

impl Organization {
    /// The plan as a badge label: `free` → `Free`.
    #[must_use]
    pub fn plan_label(&self) -> String {
        let mut chars = self.plan.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => "Free".into(),
        }
    }
}

/// A successful `POST /api/v1/oauth/token`.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: AccessToken,
    pub expires_in: u64,
    pub refresh_token: RefreshToken,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub user: Option<Me>,
}

#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_description: String,
}

/// What [`authorize_url`] needs.
pub struct AuthorizeRequest<'a> {
    pub app: App,
    pub redirect_uri: &'a str,
    pub code_challenge: &'a str,
    pub state: &'a str,
    pub device_name: &'a str,
}

/// `{server}/oauth/authorize?client_id=ferrum-{app}&redirect_uri=…&…`.
pub fn authorize_url(server: &str, request: &AuthorizeRequest<'_>) -> Result<String> {
    let mut url = url::Url::parse(&format!("{server}/oauth/authorize"))
        .map_err(|e| AccountError::Invalid(format!("bad server URL {server}: {e}")))?;
    url.query_pairs_mut()
        .append_pair("client_id", &request.app.client_id())
        .append_pair("redirect_uri", request.redirect_uri)
        .append_pair("code_challenge", request.code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", request.state)
        .append_pair("device_name", request.device_name);
    Ok(url.into())
}

/// An HTTP client for one server.
#[derive(Clone)]
pub struct Api {
    http: reqwest::Client,
    server: String,
}

impl Api {
    pub fn new(server: impl Into<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(TIMEOUT)
            .build()?;
        Ok(Self {
            http,
            server: server.into(),
        })
    }

    #[must_use]
    pub fn server(&self) -> &str {
        &self.server
    }

    pub async fn exchange_code(
        &self,
        app: App,
        code: &AuthCode,
        verifier: &str,
        redirect_uri: &str,
        device_id: &str,
    ) -> Result<TokenResponse> {
        self.token(serde_json::json!({
            "grant_type": "authorization_code",
            "code": code.expose(),
            "code_verifier": verifier,
            "redirect_uri": redirect_uri,
            "client_id": app.client_id(),
            "device_id": device_id,
        }))
        .await
    }

    /// Rotates `refresh_token`: the response carries its successor.
    pub async fn refresh(&self, refresh_token: &RefreshToken) -> Result<TokenResponse> {
        self.token(serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token.expose(),
        }))
        .await
    }

    /// A child session for `app` from the launcher's session (which does
    /// not rotate).
    pub async fn exchange(
        &self,
        parent: &RefreshToken,
        app: App,
        device_id: &str,
    ) -> Result<TokenResponse> {
        self.token(serde_json::json!({
            "grant_type": "exchange",
            "parent_refresh_token": parent.expose(),
            "client_id": app.client_id(),
            "device_id": device_id,
        }))
        .await
    }

    async fn token(&self, body: serde_json::Value) -> Result<TokenResponse> {
        let response = self
            .http
            .post(format!("{}/api/v1/oauth/token", self.server))
            .json(&body)
            .send()
            .await?;
        parse(response).await
    }

    /// Ends the session (and its children). The server answers 200 even for
    /// unknown tokens.
    pub async fn revoke(&self, refresh_token: &RefreshToken) -> Result<()> {
        let response = self
            .http
            .post(format!("{}/api/v1/oauth/revoke", self.server))
            .json(&serde_json::json!({ "refresh_token": refresh_token.expose() }))
            .send()
            .await?;
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(error_from(status.as_u16(), response).await)
        }
    }

    pub async fn me(&self, access_token: &AccessToken) -> Result<Me> {
        let response = self
            .http
            .get(format!("{}/api/v1/me", self.server))
            .bearer_auth(access_token.expose())
            .send()
            .await?;
        parse(response).await
    }
}

async fn parse<T: serde::de::DeserializeOwned>(response: reqwest::Response) -> Result<T> {
    let status = response.status();
    if !status.is_success() {
        return Err(error_from(status.as_u16(), response).await);
    }
    response
        .json()
        .await
        .map_err(|e| AccountError::Network(format!("unexpected response: {}", e.without_url())))
}

async fn error_from(status: u16, response: reqwest::Response) -> AccountError {
    match response.json::<ErrorBody>().await {
        Ok(body) if !body.error.is_empty() => AccountError::Server {
            status,
            code: body.error,
            description: body.error_description,
        },
        _ => AccountError::Http(status),
    }
}

/// Downloads an avatar image (`?s=<px>` asks GitHub for that size).
pub async fn fetch_avatar(url: &str, size: u32) -> Result<Vec<u8>> {
    let mut url = url::Url::parse(url).map_err(|e| AccountError::Invalid(e.to_string()))?;
    if url.host_str() == Some("avatars.githubusercontent.com") {
        url.query_pairs_mut().append_pair("s", &size.to_string());
    }
    let http = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(TIMEOUT)
        .build()?;
    let response = http.get(url).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(AccountError::Http(status.as_u16()));
    }
    Ok(response.bytes().await?.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_authorize_url() {
        let url = authorize_url(
            "https://tryferrum.dev",
            &AuthorizeRequest {
                app: App::Launcher,
                redirect_uri: "http://127.0.0.1:5123/callback",
                code_challenge: "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
                state: "abc",
                device_name: "Ada's MacBook Pro",
            },
        )
        .expect("url");
        assert_eq!(
            url,
            "https://tryferrum.dev/oauth/authorize?client_id=ferrum-launcher\
             &redirect_uri=http%3A%2F%2F127.0.0.1%3A5123%2Fcallback\
             &code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM\
             &code_challenge_method=S256&state=abc&device_name=Ada%27s+MacBook+Pro"
        );
        assert!(
            authorize_url(
                "not a url",
                &AuthorizeRequest {
                    app: App::Editor,
                    redirect_uri: "",
                    code_challenge: "",
                    state: "",
                    device_name: "",
                }
            )
            .is_err()
        );
    }

    #[test]
    fn display_name_initials_and_plan() {
        let mut me = Me {
            username: "ada".into(),
            ..Me::default()
        };
        assert_eq!(me.display_name(), "ada");
        assert_eq!(me.initials(), "A");
        me.name = Some("Ada King Lovelace".into());
        assert_eq!(me.initials(), "AK");
        let org = Organization {
            plan: "free".into(),
            ..Organization::default()
        };
        assert_eq!(org.plan_label(), "Free");
    }

    #[test]
    fn parses_a_token_response_with_unknown_fields() {
        let json = r#"{"access_token":"fat_a","token_type":"Bearer","expires_in":900,
            "refresh_token":"frt_b","session_id":"s","user":{"id":"u","username":"ada",
            "account_url":"x","organizations":[],"new_field":1}}"#;
        let parsed: TokenResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(parsed.refresh_token.expose(), "frt_b");
        assert_eq!(parsed.user.expect("user").username, "ada");
        assert!(!format!("{:?}", parsed.access_token).contains("fat_a"));
    }
}
