//! OAuth 2.0: authorization code with PKCE through a loopback redirect
//! (RFC 8252), client credentials and refresh. Tokens are cached per auth
//! config and environment (memory + keychain, see `secrets`); requests send
//! `Authorization: {prefix} {access token}`.

use crate::{AppServices, Prepared};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ferrumapi_domain::{
    ApiKeyLocation, Auth, ClientAuthentication, EnvironmentId, ExecError, HttpMethod, HttpRequest,
    HttpResponse, KeyValue, OAuth2Auth, OAuth2Grant, OAuth2Token, RequestBody, RequestSettings,
    VariableSet,
};
use ferrumapi_http::CancellationToken;
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;

/// Opens the provider's sign-in page (the default browser in the app).
pub type OAuthOpener = Box<dyn FnOnce(String) + Send>;

/// How long the loopback listener waits for the browser.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

/// `BASE64URL(SHA256(verifier))`, the S256 PKCE challenge (RFC 7636).
#[must_use]
pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn random() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// The provider's authorization URL for this sign-in.
fn authorize_url(
    cfg: &OAuth2Auth,
    redirect_uri: &str,
    state: &str,
    challenge: Option<&str>,
) -> Result<String, ExecError> {
    let mut url = ferrumapi_http::parse_url(&cfg.auth_url)?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", cfg.client_id.trim())
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("state", state);
        if !cfg.scope.trim().is_empty() {
            q.append_pair("scope", cfg.scope.trim());
        }
        if let Some(c) = challenge {
            q.append_pair("code_challenge", c)
                .append_pair("code_challenge_method", "S256");
        }
    }
    Ok(url.into())
}

/// A token endpoint response: JSON (RFC 6749 §5.1) or, from providers that
/// ignore `Accept`, a form-encoded body.
fn parse_token(resp: &HttpResponse, now: i64) -> Result<OAuth2Token, ExecError> {
    let fields: HashMap<String, Value> = match serde_json::from_slice(&resp.body) {
        Ok(Value::Object(map)) => map.into_iter().collect(),
        _ => url::form_urlencoded::parse(&resp.body)
            .map(|(k, v)| (k.into_owned(), Value::String(v.into_owned())))
            .collect(),
    };
    let text = |k: &str| match fields.get(k) {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    };
    if let Some(error) = text("error") {
        let detail = text("error_description").map_or_else(String::new, |d| format!(": {d}"));
        return Err(ExecError::Other(format!(
            "token endpoint returned {error}{detail}"
        )));
    }
    let Some(access_token) = text("access_token").filter(|t| !t.is_empty()) else {
        return Err(ExecError::Other(format!(
            "token endpoint returned {} without an access_token",
            resp.status
        )));
    };
    Ok(OAuth2Token {
        access_token,
        token_type: text("token_type").unwrap_or_else(|| "Bearer".to_owned()),
        refresh_token: text("refresh_token"),
        expires_at: text("expires_in")
            .and_then(|s| s.parse::<i64>().ok())
            .map(|s| now + s),
        scope: text("scope").unwrap_or_default(),
    })
}

/// Waits for the browser's redirect to `/callback` and returns its query.
/// Other paths (a favicon) get a 404 and are ignored.
async fn wait_for_callback(listener: &TcpListener) -> Result<HashMap<String, String>, ExecError> {
    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|e| ExecError::Other(format!("sign-in listener: {e}")))?;
        let mut buf = Vec::new();
        let mut chunk = [0u8; 2048];
        while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 16 * 1024 {
            match stream.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => buf.extend_from_slice(chunk.get(..n).unwrap_or_default()),
            }
        }
        let head = String::from_utf8_lossy(&buf);
        let target = head.split_whitespace().nth(1).unwrap_or("/");
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let (status, body) = if path == "/callback" {
            (
                "200 OK",
                "<!doctype html><title>FerrumAPI</title><p>Signed in. You can close this window and return to FerrumAPI.</p>",
            )
        } else {
            ("404 Not Found", "")
        };
        let reply = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(reply.as_bytes()).await;
        let _ = stream.shutdown().await;
        if path == "/callback" {
            return Ok(url::form_urlencoded::parse(query.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect());
        }
    }
}

impl AppServices {
    /// Loads keychain secrets into the request's auth and turns OAuth 2
    /// into its token header (fetching or refreshing the token if needed;
    /// an authorization-code grant without a token asks for a sign-in).
    pub(crate) async fn resolve_auth(
        &self,
        mut prepared: Prepared,
        cancel: &CancellationToken,
    ) -> Result<Prepared, ExecError> {
        self.hydrate_secrets(vec![&mut prepared.request.auth]).await;
        if let Auth::OAuth2(cfg) = &prepared.request.auth {
            let cfg = substituted(cfg, &prepared.variables);
            let settings = prepared.request.settings;
            let token = self
                .oauth2_token(&cfg, settings, prepared.environment, None, cancel)
                .await?;
            let prefix = cfg.header_prefix.trim();
            prepared.request.auth = Auth::ApiKey {
                key: "Authorization".to_owned(),
                value: if prefix.is_empty() {
                    token.access_token
                } else {
                    format!("{prefix} {}", token.access_token)
                },
                location: ApiKeyLocation::Header,
            };
        }
        Ok(prepared)
    }

    /// "Get new access token": runs the grant from scratch (the browser
    /// sign-in for authorization code, via `open`) and caches the token for
    /// the request's environment.
    pub async fn oauth2_authorize(
        &self,
        mut prepared: Prepared,
        open: OAuthOpener,
        cancel: CancellationToken,
    ) -> Result<OAuth2Token, ExecError> {
        self.hydrate_secrets(vec![&mut prepared.request.auth]).await;
        let Auth::OAuth2(cfg) = &prepared.request.auth else {
            return Err(ExecError::InvalidRequest(
                "the auth type is not OAuth 2.0".into(),
            ));
        };
        let cfg = substituted(cfg, &prepared.variables);
        let settings = prepared.request.settings;
        self.oauth2_token(&cfg, settings, prepared.environment, Some(open), &cancel)
            .await
    }

    /// Forgets the token cached for `secret_ref` in `environment`.
    pub async fn oauth2_clear(&self, secret_ref: &str, environment: Option<EnvironmentId>) {
        if let Err(e) = self.store_token(secret_ref, environment, None).await {
            tracing::warn!(error = %e, "could not forget an OAuth 2 token");
        }
    }

    /// A usable token: cached, refreshed, or newly obtained. `open` forces a
    /// new grant (and is the only way to run the browser sign-in).
    async fn oauth2_token(
        &self,
        cfg: &OAuth2Auth,
        settings: RequestSettings,
        env: Option<EnvironmentId>,
        open: Option<OAuthOpener>,
        cancel: &CancellationToken,
    ) -> Result<OAuth2Token, ExecError> {
        if open.is_none()
            && let Some(cached) = self.cached_token(&cfg.secret_ref, env).await
        {
            if cached.is_fresh(now()) {
                return Ok(cached);
            }
            if let Some(refresh) = cached.refresh_token.clone() {
                let fields = vec![
                    ("grant_type", "refresh_token".to_owned()),
                    ("refresh_token", refresh.clone()),
                ];
                match self.token_request(cfg, settings, fields, cancel).await {
                    Ok(mut token) => {
                        token.refresh_token.get_or_insert(refresh);
                        self.keep_token(cfg, env, &token).await;
                        return Ok(token);
                    }
                    Err(ExecError::Cancelled) => return Err(ExecError::Cancelled),
                    Err(e) => tracing::debug!(error = %e, "OAuth 2 refresh failed"),
                }
            }
        }
        let token = match (cfg.grant, open) {
            (OAuth2Grant::ClientCredentials, _) => {
                let mut fields = vec![("grant_type", "client_credentials".to_owned())];
                if !cfg.scope.trim().is_empty() {
                    fields.push(("scope", cfg.scope.trim().to_owned()));
                }
                self.token_request(cfg, settings, fields, cancel).await?
            }
            (OAuth2Grant::AuthorizationCode, Some(open)) => {
                self.authorization_code(cfg, settings, open, cancel).await?
            }
            (OAuth2Grant::AuthorizationCode, None) => {
                return Err(ExecError::InvalidRequest(
                    "no OAuth 2.0 access token yet: use Get new access token in the Auth tab"
                        .into(),
                ));
            }
        };
        self.keep_token(cfg, env, &token).await;
        Ok(token)
    }

    async fn keep_token(&self, cfg: &OAuth2Auth, env: Option<EnvironmentId>, token: &OAuth2Token) {
        if let Err(e) = self
            .store_token(&cfg.secret_ref, env, Some(token.clone()))
            .await
        {
            tracing::warn!(error = %e, "OAuth 2 token kept in memory only");
        }
    }

    /// Browser sign-in: listen on the loopback port, open the authorization
    /// URL, take the code from the redirect and exchange it.
    async fn authorization_code(
        &self,
        cfg: &OAuth2Auth,
        settings: RequestSettings,
        open: OAuthOpener,
        cancel: &CancellationToken,
    ) -> Result<OAuth2Token, ExecError> {
        let port = cfg.redirect_port;
        let listener = TcpListener::bind(("127.0.0.1", port))
            .await
            .map_err(|e| ExecError::Other(format!("could not listen on 127.0.0.1:{port}: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| ExecError::Other(e.to_string()))?
            .port();
        let redirect_uri = OAuth2Auth::redirect_uri(port);
        let state = random();
        let verifier = format!("{}{}", random(), random());
        let challenge = cfg.pkce.then(|| pkce_challenge(&verifier));
        open(authorize_url(
            cfg,
            &redirect_uri,
            &state,
            challenge.as_deref(),
        )?);
        let params = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(ExecError::Cancelled),
            () = tokio::time::sleep(SIGN_IN_TIMEOUT) => {
                return Err(ExecError::Other("no sign-in within 5 minutes".into()));
            }
            p = wait_for_callback(&listener) => p?,
        };
        if let Some(error) = params.get("error") {
            let detail = params
                .get("error_description")
                .map_or_else(String::new, |d| format!(": {d}"));
            return Err(ExecError::Other(format!("sign-in failed: {error}{detail}")));
        }
        if params.get("state") != Some(&state) {
            return Err(ExecError::Other(
                "sign-in redirect had the wrong state; try again".into(),
            ));
        }
        let code = params
            .get("code")
            .ok_or_else(|| ExecError::Other("sign-in redirect had no code".into()))?;
        let mut fields = vec![
            ("grant_type", "authorization_code".to_owned()),
            ("code", code.clone()),
            ("redirect_uri", redirect_uri),
        ];
        if cfg.pkce {
            fields.push(("code_verifier", verifier));
        }
        self.token_request(cfg, settings, fields, cancel).await
    }

    /// POSTs a form to the token endpoint with the client's credentials.
    async fn token_request(
        &self,
        cfg: &OAuth2Auth,
        settings: RequestSettings,
        fields: Vec<(&str, String)>,
        cancel: &CancellationToken,
    ) -> Result<OAuth2Token, ExecError> {
        let mut req = HttpRequest::new(HttpMethod::Post, cfg.token_url.trim());
        req.settings = settings;
        req.headers
            .push(KeyValue::new("Accept", "application/json"));
        let mut rows: Vec<KeyValue> = fields
            .into_iter()
            .map(|(k, v)| KeyValue::new(k, v))
            .collect();
        let (id, secret) = (cfg.client_id.trim(), cfg.client_secret.trim());
        match cfg.client_auth {
            ClientAuthentication::Body => {
                rows.push(KeyValue::new("client_id", id));
                if !secret.is_empty() {
                    rows.push(KeyValue::new("client_secret", secret));
                }
            }
            // RFC 6749 §2.3.1: both form-encoded before Basic encoding.
            ClientAuthentication::BasicHeader => {
                let e = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect();
                req.auth = Auth::Basic {
                    username: e(id),
                    password: e(secret),
                };
            }
        }
        req.body = RequestBody::UrlEncoded { fields: rows };
        let resp = self
            .http
            .execute(&req, &VariableSet::new(), cancel.clone())
            .await?;
        parse_token(&resp, now())
    }
}

/// `cfg` with `{{variables}}` resolved.
fn substituted(cfg: &OAuth2Auth, vars: &VariableSet) -> OAuth2Auth {
    let mut auth = Auth::OAuth2(Box::new(cfg.clone()));
    auth.map_texts(&mut |t| *t = vars.resolve(t));
    match auth {
        Auth::OAuth2(c) => *c,
        _ => cfg.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_rfc_7636_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn authorize_url_has_pkce_and_scope() {
        let cfg = OAuth2Auth {
            auth_url: "https://id.example/authorize?audience=api".into(),
            client_id: "app".into(),
            scope: "read write".into(),
            ..OAuth2Auth::default()
        };
        let url =
            authorize_url(&cfg, "http://127.0.0.1:5/callback", "st", Some("ch")).expect("url");
        assert_eq!(
            url,
            "https://id.example/authorize?audience=api&response_type=code&client_id=app\
             &redirect_uri=http%3A%2F%2F127.0.0.1%3A5%2Fcallback&state=st&scope=read+write\
             &code_challenge=ch&code_challenge_method=S256"
        );
    }

    fn resp(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            body: body.as_bytes().to_vec(),
            ..HttpResponse::default()
        }
    }

    #[test]
    fn token_responses() {
        let t = parse_token(
            &resp(
                200,
                r#"{"access_token":"a","token_type":"bearer","expires_in":60,"refresh_token":"r"}"#,
            ),
            1_000,
        )
        .expect("token");
        assert_eq!(
            (
                t.access_token.as_str(),
                t.expires_at,
                t.refresh_token.as_deref()
            ),
            ("a", Some(1_060), Some("r"))
        );
        let form =
            parse_token(&resp(200, "access_token=b&expires_in=10&scope=x"), 0).expect("form");
        assert_eq!(
            (form.access_token.as_str(), form.expires_at),
            ("b", Some(10))
        );
        let err = parse_token(
            &resp(
                400,
                r#"{"error":"invalid_client","error_description":"bad secret"}"#,
            ),
            0,
        )
        .expect_err("error");
        assert_eq!(
            err.to_string(),
            "request failed: token endpoint returned invalid_client: bad secret"
        );
        assert!(parse_token(&resp(500, "oops"), 0).is_err());
    }
}
