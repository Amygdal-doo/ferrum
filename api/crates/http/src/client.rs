//! [`HttpClient`]: cached reqwest clients plus the `execute` pipeline.

use crate::body::PreparedBody;
use crate::build::prepare;
use crate::errors::{kind, map_reqwest};
use crate::response::read_response;
use crate::signing;
use crate::substitute::substitute;
use ferrumapi_domain::{Auth, ExecError, HttpRequest, HttpResponse, RequestSettings, VariableSet};
use reqwest::header::{AUTHORIZATION, HeaderValue};
use reqwest::{Method, redirect};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

const USER_AGENT: &str = concat!("FerrumAPI/", env!("CARGO_PKG_VERSION"));
const MAX_REDIRECTS: usize = 10;

/// Cheap to clone; reuse one per app. Holds up to four underlying clients,
/// one per (verify TLS, follow redirects) combination, built lazily.
#[derive(Debug, Clone, Default)]
pub struct HttpClient {
    clients: Arc<Mutex<[Option<reqwest::Client>; 4]>>,
}

impl HttpClient {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Substitutes `vars`, applies auth/body/settings and sends `request`.
    /// `request.auth` must already be effective: `Auth::Inherit` is treated as
    /// no auth. Resolves to `Err(ExecError::Cancelled)` promptly when `cancel`
    /// fires.
    pub async fn execute(
        &self,
        request: &HttpRequest,
        vars: &VariableSet,
        cancel: CancellationToken,
    ) -> Result<HttpResponse, ExecError> {
        let method = request.method.as_str();
        if cancel.is_cancelled() {
            return Err(ExecError::Cancelled);
        }
        let request = ferrumapi_domain::protocol::lower(&substitute(request, vars))?;
        let timeout_ms = request.settings.timeout_ms;
        let work = async {
            if timeout_ms == 0 {
                self.run(&request).await
            } else {
                tokio::time::timeout(Duration::from_millis(timeout_ms), self.run(&request))
                    .await
                    .unwrap_or(Err(ExecError::Timeout(timeout_ms)))
            }
        };
        let result = tokio::select! {
            biased;
            () = cancel.cancelled() => Err(ExecError::Cancelled),
            r = work => r,
        };
        match &result {
            Ok(r) => tracing::debug!(
                method,
                status = r.status,
                time_ms = r.time_ms,
                "request done"
            ),
            Err(e) => tracing::debug!(method, error = kind(e), "request failed"),
        }
        result
    }

    async fn run(&self, request: &HttpRequest) -> Result<HttpResponse, ExecError> {
        let client = self.client_for(request.settings)?;
        let timeout_ms = request.settings.timeout_ms;
        let started = Instant::now();
        let req = build_request(&client, request, None).await?;
        let (method, url) = (req.method().as_str().to_owned(), req.url().clone());
        let mut resp = client
            .execute(req)
            .await
            .map_err(|e| map_reqwest(e, timeout_ms))?;
        // Digest: answer the server's challenge once, with a fresh body.
        if let Auth::Digest(cfg) = &request.auth
            && resp.status() == reqwest::StatusCode::UNAUTHORIZED
            && let Some(answer) =
                signing::digest_answer(&cfg.username, &cfg.password, &method, &url, &resp)
        {
            let req = build_request(&client, request, Some(answer)).await?;
            resp = client
                .execute(req)
                .await
                .map_err(|e| map_reqwest(e, timeout_ms))?;
        }
        read_response(resp, started)
            .await
            .map_err(|e| map_reqwest(e, timeout_ms))
    }

    fn client_for(&self, settings: RequestSettings) -> Result<reqwest::Client, ExecError> {
        let idx = usize::from(settings.verify_tls) * 2 + usize::from(settings.follow_redirects);
        let mut cache = self.clients.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(client) = cache.get(idx).and_then(Option::as_ref) {
            return Ok(client.clone());
        }
        let client = build_client(settings.verify_tls, settings.follow_redirects)?;
        if let Some(slot) = cache.get_mut(idx) {
            *slot = Some(client.clone());
        }
        Ok(client)
    }
}

/// Builds the outgoing request and signs it (OAuth 1, AWS). `authorization`
/// is a precomputed header (a Digest answer) that a user header still beats.
async fn build_request(
    client: &reqwest::Client,
    request: &HttpRequest,
    authorization: Option<String>,
) -> Result<reqwest::Request, ExecError> {
    let prepared = prepare(request).await?;
    let method = Method::from_bytes(request.method.as_str().as_bytes())
        .map_err(|_| ExecError::InvalidRequest("invalid method".into()))?;
    let mut builder = client.request(method, prepared.url);
    builder = match prepared.body {
        PreparedBody::Empty => builder,
        PreparedBody::Bytes { data, .. } => builder.body(data),
        PreparedBody::Multipart(form) => builder.multipart(form),
    };
    // `headers` replaces per name, so user values override defaults
    // (User-Agent, multipart Content-Type with boundary).
    builder = builder.headers(prepared.headers);
    let mut req = builder
        .build()
        .map_err(|e| ExecError::InvalidRequest(e.without_url().to_string()))?;
    if let Some(value) = authorization
        && !req.headers().contains_key(AUTHORIZATION)
    {
        let mut value = HeaderValue::from_str(&value).map_err(|_| {
            ExecError::InvalidRequest("auth value contains invalid characters".into())
        })?;
        value.set_sensitive(true);
        req.headers_mut().insert(AUTHORIZATION, value);
    }
    signing::sign(&request.auth, &mut req)?;
    Ok(req)
}

/// rustls is the only TLS backend compiled in; no cookie store (the
/// `cookies` feature is off), so every request is stateless.
fn build_client(verify_tls: bool, follow_redirects: bool) -> Result<reqwest::Client, ExecError> {
    let policy = if follow_redirects {
        redirect::Policy::limited(MAX_REDIRECTS)
    } else {
        redirect::Policy::none()
    };
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .tls_danger_accept_invalid_certs(!verify_tls)
        .redirect(policy)
        .build()
        .map_err(|e| ExecError::Other(format!("could not build HTTP client: {}", e.without_url())))
}
