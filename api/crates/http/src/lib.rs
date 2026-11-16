//! HTTP execution for FerrumAPI: variable substitution, auth, bodies,
//! settings and cancellation on top of `reqwest` (rustls backend).
//!
//! Nothing in this crate logs URLs, header values, bodies or credentials.

mod auth;
mod body;
mod build;
mod client;
pub mod cookies;
mod errors;
mod response;
pub mod signing;
mod substitute;
pub mod tls;
pub mod ws;

pub use tokio_util::sync::CancellationToken;

pub use build::parse_url;
pub use client::HttpClient;
pub use cookies::{parse_set_cookie, parse_set_cookies};
pub use substitute::{substitute, unresolved_variables};

/// Enabled headers plus auth as `(name, value)` pairs, for transports that
/// are not plain HTTP (gRPC metadata). An API key "in query" is sent as a
/// pair as well. `request` must be substituted and its auth effective.
pub fn metadata_pairs(
    request: &ferrumapi_domain::HttpRequest,
) -> Result<Vec<(String, String)>, ferrumapi_domain::ExecError> {
    let mut headers = build::user_headers(&request.headers)?;
    let mut url = url::Url::parse("http://localhost/")
        .map_err(|e| ferrumapi_domain::ExecError::Other(e.to_string()))?;
    auth::apply_auth(&request.auth, &mut headers, &mut url)?;
    let mut out: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    out.extend(
        url.query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned())),
    );
    Ok(out)
}
