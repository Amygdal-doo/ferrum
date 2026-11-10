//! The HTTP seam: clients build [`HttpRequest`]s and a [`Transport`] sends
//! them. Production uses reqwest; tests replay recorded JSON fixtures.

use crate::error::{ForgeError, ForgeResult};
use async_trait::async_trait;
use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
}

impl Method {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// JSON body.
    pub body: Option<String>,
}

/// Authorization headers are redacted.
impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let headers: Vec<(&str, &str)> = self
            .headers
            .iter()
            .map(|(k, v)| {
                let secret = k.eq_ignore_ascii_case("authorization")
                    || k.eq_ignore_ascii_case("private-token");
                (k.as_str(), if secret { "<redacted>" } else { v.as_str() })
            })
            .collect();
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &headers)
            .field("body", &self.body.as_ref().map(String::len))
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HttpResponse {
    pub status: u16,
    /// Lower-case header names.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl HttpResponse {
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, request: HttpRequest) -> ForgeResult<HttpResponse>;
}

/// reqwest + rustls.
#[derive(Debug, Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> ForgeResult<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("FerrumGIT/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| ForgeError::Invalid(format!("could not start the HTTP client: {e}")))?;
        Ok(Self { client })
    }
}

fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|r| r.split('/').next())
        .unwrap_or(url)
        .to_owned()
}

#[async_trait]
impl Transport for ReqwestTransport {
    async fn send(&self, request: HttpRequest) -> ForgeResult<HttpResponse> {
        let host = host_of(&request.url);
        let method = match request.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
        };
        let mut builder = self.client.request(method, &request.url);
        for (k, v) in &request.headers {
            builder = builder.header(k, v);
        }
        if let Some(body) = request.body {
            builder = builder
                .header("content-type", "application/json")
                .body(body);
        }
        // reqwest errors carry the URL only (tokens are headers, never in URLs).
        let network = |e: reqwest::Error| ForgeError::Network {
            host: host.clone(),
            message: e.without_url().to_string(),
        };
        let response = builder.send().await.map_err(network)?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(k, v)| {
                v.to_str()
                    .ok()
                    .map(|v| (k.as_str().to_ascii_lowercase(), v.to_owned()))
            })
            .collect();
        let body = response.text().await.map_err(network)?;
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_tokens() {
        let r = HttpRequest {
            method: Method::Get,
            url: "https://api.github.com/user".into(),
            headers: vec![
                ("Authorization".into(), "Bearer ghp_secret".into()),
                ("PRIVATE-TOKEN".into(), "glpat-secret".into()),
            ],
            body: None,
        };
        let shown = format!("{r:?}");
        assert!(!shown.contains("secret"), "{shown}");
        assert_eq!(
            host_of("https://git.example.com:8443/api/v4/x"),
            "git.example.com:8443"
        );
    }
}
