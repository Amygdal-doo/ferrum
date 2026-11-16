//! OAuth 2.0 end to end against a local axum authorization server:
//! client credentials (Basic client auth, secret from the keychain store),
//! per-environment token caching, authorization code with PKCE through the
//! loopback redirect, and refresh.

use axum::Router;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use base64::Engine as _;
use ferrumapi_application::domain::{
    Auth, ClientAuthentication, Environment, ExecError, HttpMethod, HttpRequest, OAuth2Auth,
    OAuth2Grant, VariableSet,
};
use ferrumapi_application::http::HttpClient;
use ferrumapi_application::secrets::MemoryStore;
use ferrumapi_application::{AppServices, CancellationToken, Prepared, pkce_challenge};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Server {
    token_calls: Mutex<Vec<HashMap<String, String>>>,
    challenge: Mutex<String>,
}

async fn authorize(
    State(s): State<Arc<Server>>,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    *s.challenge.lock().expect("lock") = q.get("code_challenge").cloned().unwrap_or_default();
    let to = format!(
        "{}?code=the-code&state={}",
        q.get("redirect_uri").cloned().unwrap_or_default(),
        q.get("state").cloned().unwrap_or_default()
    );
    Redirect::to(&to)
}

async fn token(State(s): State<Arc<Server>>, h: HeaderMap, body: String) -> impl IntoResponse {
    let form: HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    s.token_calls.lock().expect("lock").push(form.clone());
    let basic = h
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let deny = |e: &str| (StatusCode::BAD_REQUEST, axum::Json(json!({"error": e})));
    match form.get("grant_type").map(String::as_str) {
        Some("client_credentials") => {
            let expected = format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode("cli:s3cret")
            );
            if basic != expected {
                return deny("invalid_client");
            }
            let n = s.token_calls.lock().expect("lock").len();
            (
                StatusCode::OK,
                axum::Json(json!({"access_token": format!("cc-{n}"), "expires_in": 3600})),
            )
        }
        Some("authorization_code") => {
            let verifier = form.get("code_verifier").cloned().unwrap_or_default();
            let ok = form.get("code").map(String::as_str) == Some("the-code")
                && form.get("client_id").map(String::as_str) == Some("app")
                && pkce_challenge(&verifier) == *s.challenge.lock().expect("lock")
                && form
                    .get("redirect_uri")
                    .is_some_and(|r| r.starts_with("http://127.0.0.1:"));
            if !ok {
                return deny("invalid_grant");
            }
            (
                StatusCode::OK,
                axum::Json(json!({"access_token": "ac", "expires_in": 0, "refresh_token": "r1"})),
            )
        }
        Some("refresh_token") if form.get("refresh_token").map(String::as_str) == Some("r1") => (
            StatusCode::OK,
            axum::Json(json!({"access_token": "fresh", "expires_in": 3600})),
        ),
        _ => deny("unsupported_grant_type"),
    }
}

async fn api(h: HeaderMap) -> String {
    h.get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("none")
        .to_owned()
}

async fn start(state: Arc<Server>) -> String {
    let app = Router::new()
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .route("/api", get(api))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{addr}")
}

fn prepared(auth: &Auth, env: Option<&Environment>, base: &str) -> Prepared {
    let mut request = HttpRequest::new(HttpMethod::Get, "{{base}}/api");
    request.auth = auth.clone();
    let mut variables = VariableSet::new();
    variables.insert("base", base);
    Prepared {
        request,
        variables,
        environment: env.map(|e| e.id),
    }
}

async fn call(svc: &AppServices, p: Prepared) -> Result<String, ExecError> {
    let r = svc.execute(p, CancellationToken::new()).await?;
    Ok(r.body_text())
}

fn services() -> (AppServices, MemoryStore, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("ferrumapi-oauth-{}", uuid::Uuid::new_v4()));
    let store = MemoryStore::default();
    let svc = AppServices::start_with_secrets(&dir, Arc::new(store.clone())).expect("start");
    (svc, store, dir)
}

#[test]
fn client_credentials_cached_per_environment_with_keychain_secret() {
    let (svc, store, dir) = services();
    let s = svc.clone();
    svc.spawn(async move {
        let server = Arc::new(Server::default());
        let base = start(server.clone()).await;
        let mut auth = Auth::OAuth2(Box::new(OAuth2Auth {
            grant: OAuth2Grant::ClientCredentials,
            token_url: "{{base}}/token".into(),
            client_id: "cli".into(),
            client_secret: "s3cret".into(),
            client_auth: ClientAuthentication::BasicHeader,
            ..OAuth2Auth::default()
        }));
        s.stash_secrets(vec![auth.clone()]).await.expect("stash");
        // What a tab or collection holds after a restart: no secret.
        auth.set_secret_field("client_secret", String::new());
        let (dev, prod) = (Environment::new("dev"), Environment::new("prod"));
        assert_eq!(
            call(&s, prepared(&auth, Some(&dev), &base))
                .await
                .expect("1"),
            "Bearer cc-1"
        );
        assert_eq!(
            call(&s, prepared(&auth, Some(&dev), &base))
                .await
                .expect("2"),
            "Bearer cc-1"
        );
        assert_eq!(
            call(&s, prepared(&auth, Some(&prod), &base))
                .await
                .expect("3"),
            "Bearer cc-2"
        );
        assert_eq!(server.token_calls.lock().expect("lock").len(), 2);
        let secrets = store.dump();
        assert!(
            secrets.values().any(|v| v.contains("cc-1")),
            "token cached in the store"
        );
        assert!(
            secrets
                .values()
                .any(|v| v == r#"{"client_secret":"s3cret"}"#)
        );

        // A wrong secret is reported from the token endpoint.
        let bad = Auth::OAuth2(Box::new(OAuth2Auth {
            grant: OAuth2Grant::ClientCredentials,
            token_url: format!("{base}/token"),
            client_id: "cli".into(),
            client_secret: "nope".into(),
            client_auth: ClientAuthentication::BasicHeader,
            ..OAuth2Auth::default()
        }));
        let err = call(&s, prepared(&bad, None, &base))
            .await
            .expect_err("denied");
        assert!(err.to_string().contains("invalid_client"), "{err}");
    })
    .join_blocking();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn authorization_code_with_pkce_then_refresh() {
    let (svc, _store, dir) = services();
    let s = svc.clone();
    svc.spawn(async move {
        let server = Arc::new(Server::default());
        let base = start(server.clone()).await;
        let auth = Auth::OAuth2(Box::new(OAuth2Auth {
            auth_url: format!("{base}/authorize"),
            token_url: format!("{base}/token"),
            client_id: "app".into(),
            scope: "read".into(),
            ..OAuth2Auth::default()
        }));
        let err = call(&s, prepared(&auth, None, &base))
            .await
            .expect_err("no token");
        assert!(err.to_string().contains("Get new access token"), "{err}");

        // The "browser": follows the authorize redirect to the loopback listener.
        let open: ferrumapi_application::OAuthOpener = Box::new(|url: String| {
            tokio::spawn(async move {
                let r = HttpRequest::new(HttpMethod::Get, url);
                let _ = HttpClient::new()
                    .execute(&r, &VariableSet::new(), CancellationToken::new())
                    .await;
            });
        });
        let token = s
            .oauth2_authorize(prepared(&auth, None, &base), open, CancellationToken::new())
            .await
            .expect("signed in");
        assert_eq!(token.access_token, "ac");
        assert!(
            !server.challenge.lock().expect("lock").is_empty(),
            "PKCE challenge sent"
        );
        // `ac` expired at once, so the next call refreshes it.
        assert_eq!(
            call(&s, prepared(&auth, None, &base)).await.expect("call"),
            "Bearer fresh"
        );
        assert_eq!(
            call(&s, prepared(&auth, None, &base)).await.expect("call"),
            "Bearer fresh"
        );
        let grants: Vec<String> = server
            .token_calls
            .lock()
            .expect("lock")
            .iter()
            .filter_map(|f| f.get("grant_type").cloned())
            .collect();
        assert_eq!(grants, ["authorization_code", "refresh_token"]);
        let Auth::OAuth2(cfg) = &auth else {
            unreachable!()
        };
        s.oauth2_clear(&cfg.secret_ref, None).await;
        assert!(call(&s, prepared(&auth, None, &base)).await.is_err());
    })
    .join_blocking();
    let _ = std::fs::remove_dir_all(dir);
}

/// Blocks on a spawned task from a plain test thread.
trait JoinBlocking {
    fn join_blocking(self);
}

impl JoinBlocking for tokio::task::JoinHandle<()> {
    fn join_blocking(self) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("rt");
        rt.block_on(self).expect("task");
    }
}
