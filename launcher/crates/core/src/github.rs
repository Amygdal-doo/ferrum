//! Minimal GitHub Releases client.

use crate::auth::GitHubToken;
use crate::release::{Asset, GitHubRelease, ReleaseInfo};
use crate::{LauncherError, Result};
use futures::StreamExt;
use reqwest::{Response, StatusCode, header};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

const PER_PAGE: usize = 100;
const MAX_PAGES: usize = 20;
pub(crate) const USER_AGENT: &str = concat!("FerrumLauncher/", env!("CARGO_PKG_VERSION"));
/// Catalog logos are small icons; anything bigger is not a logo.
const MAX_LOGO_BYTES: usize = 1024 * 1024;

/// Called with `(downloaded_bytes, total_bytes)` as an asset streams in.
pub type DownloadProgress = Arc<dyn Fn(u64, Option<u64>) + Send + Sync>;

#[derive(Clone)]
pub struct GitHubClient {
    http: reqwest::Client,
    api_base: String,
    owner: String,
    repo: String,
    token: Option<GitHubToken>,
}

impl GitHubClient {
    pub fn new(owner: &str, repo: &str, token: Option<GitHubToken>) -> Result<Self> {
        let owner = owner.trim();
        let repo = repo.trim();
        let valid = |part: &str| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        if !valid(owner) || !valid(repo) {
            return Err(LauncherError::Invalid(format!(
                "`{owner}/{repo}` is not a valid GitHub repository"
            )));
        }
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| LauncherError::Network(e.to_string()))?;
        Ok(Self {
            http,
            api_base: "https://api.github.com".into(),
            owner: owner.into(),
            repo: repo.into(),
            token,
        })
    }

    /// Points the client at another API host (tests, GitHub Enterprise).
    #[must_use]
    pub fn with_api_base(mut self, api_base: impl Into<String>) -> Self {
        self.api_base = api_base.into().trim_end_matches('/').to_string();
        self
    }

    #[must_use]
    pub fn token_source(&self) -> Option<crate::TokenSource> {
        self.token.as_ref().map(|token| token.source)
    }

    #[must_use]
    pub fn repo_slug(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    fn get(&self, url: &str, accept: &str, token: Option<&GitHubToken>) -> reqwest::RequestBuilder {
        let request = self
            .http
            .get(url)
            .header(header::ACCEPT, accept)
            .header("X-GitHub-Api-Version", "2022-11-28");
        match token {
            Some(token) => request.bearer_auth(token.expose()),
            None => request,
        }
    }

    /// Sends a GET with the token. If GitHub rejects the token (expired or
    /// revoked), tries once more without it: the repo is public, so only a
    /// private repo still ends in `Unauthorized`.
    async fn send(&self, url: &str, accept: &str) -> Result<Response> {
        let response = self
            .get(url, accept, self.token.as_ref())
            .send()
            .await
            .map_err(network_error)?;
        if response.status() != StatusCode::UNAUTHORIZED || self.token.is_none() {
            return Ok(response);
        }
        tracing::warn!("GitHub rejected the saved token; retrying without it");
        self.get(url, accept, None)
            .send()
            .await
            .map_err(network_error)
    }

    /// All Ferrum app releases in the repo (drafts and foreign tags skipped).
    pub async fn list_releases(&self) -> Result<Vec<ReleaseInfo>> {
        let mut releases = Vec::new();
        for page in 1..=MAX_PAGES {
            let url = format!(
                "{}/repos/{}/{}/releases?per_page={PER_PAGE}&page={page}",
                self.api_base, self.owner, self.repo
            );
            let response = self.send(&url, "application/vnd.github+json").await?;
            let response = check_status(response, &self.repo_slug()).await?;
            let page_releases: Vec<GitHubRelease> = response
                .json()
                .await
                .map_err(|e| LauncherError::Network(format!("unexpected response: {e}")))?;
            let count = page_releases.len();
            releases.extend(
                page_releases
                    .into_iter()
                    .filter_map(GitHubRelease::into_release),
            );
            if count < PER_PAGE {
                break;
            }
        }
        Ok(releases)
    }

    /// `catalog.json` from the repo's default branch (raw contents API, so
    /// private repos work with the same token).
    pub async fn fetch_catalog(&self) -> Result<crate::Catalog> {
        let url = format!(
            "{}/repos/{}/{}/contents/catalog.json",
            self.api_base, self.owner, self.repo
        );
        let response = self.send(&url, "application/vnd.github.raw+json").await?;
        let response = check_status(response, "catalog.json").await?;
        let text = response.text().await.map_err(network_error)?;
        crate::Catalog::parse(&text)
    }

    /// Downloads a catalog logo. The URL comes from `catalog.json`, not
    /// GitHub's API, so the token is never sent with it. Only PNGs up to
    /// 1 MiB are accepted.
    pub async fn fetch_logo(&self, url: &str) -> Result<Vec<u8>> {
        let response = self
            .http
            .get(url)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(network_error)?;
        let response = check_status(response, url).await?;
        if response
            .content_length()
            .is_some_and(|len| len > MAX_LOGO_BYTES as u64)
        {
            return Err(LauncherError::Invalid(format!("logo too large: {url}")));
        }
        let bytes = response.bytes().await.map_err(network_error)?;
        if bytes.len() > MAX_LOGO_BYTES || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(LauncherError::Invalid(format!("not a PNG logo: {url}")));
        }
        Ok(bytes.to_vec())
    }

    /// Downloads a small text asset (e.g. a `.sha256` file).
    pub async fn download_text(&self, asset: &Asset) -> Result<String> {
        let response = self.send(&asset.url, "application/octet-stream").await?;
        let response = check_status(response, &asset.name).await?;
        response.text().await.map_err(network_error)
    }

    /// Streams `asset` to `destination`, returning the lowercase hex SHA-256.
    /// The partial file is removed on error or cancellation.
    pub async fn download_to(
        &self,
        asset: &Asset,
        destination: &Path,
        progress: DownloadProgress,
        cancel: CancellationToken,
    ) -> Result<String> {
        let result = self
            .download_inner(asset, destination, progress, cancel)
            .await;
        if result.is_err()
            && let Err(e) = tokio::fs::remove_file(destination).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(error = %e, "could not remove partial download");
        }
        result
    }

    async fn download_inner(
        &self,
        asset: &Asset,
        destination: &Path,
        progress: DownloadProgress,
        cancel: CancellationToken,
    ) -> Result<String> {
        let request = self.send(&asset.url, "application/octet-stream");
        let response = tokio::select! {
            () = cancel.cancelled() => return Err(LauncherError::Cancelled),
            response = request => response?,
        };
        let response = check_status(response, &asset.name).await?;
        let total = response
            .content_length()
            .or(Some(asset.size).filter(|s| *s > 0));

        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::File::create(destination).await?;
        let mut hasher = Sha256::new();
        let mut downloaded = 0u64;
        let mut stream = response.bytes_stream();
        progress(0, total);
        loop {
            let chunk = tokio::select! {
                () = cancel.cancelled() => return Err(LauncherError::Cancelled),
                chunk = stream.next() => chunk,
            };
            let Some(chunk) = chunk else { break };
            let chunk = chunk.map_err(network_error)?;
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            progress(downloaded, total);
        }
        file.flush().await?;
        file.sync_all().await?;
        Ok(to_hex(&hasher.finalize()))
    }
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn network_error(e: reqwest::Error) -> LauncherError {
    LauncherError::Network(e.to_string())
}

async fn check_status(response: Response, what: &str) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let headers = response.headers().clone();
    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    if status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN && remaining == Some(0))
    {
        let reset = headers
            .get("x-ratelimit-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i64>().ok())
            .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
            .map(|t| t.with_timezone(&chrono::Local));
        return Err(LauncherError::RateLimited { reset });
    }
    match status {
        StatusCode::UNAUTHORIZED => Err(LauncherError::Unauthorized),
        StatusCode::NOT_FOUND => Err(LauncherError::NotFound(what.to_string())),
        _ => {
            let body = response.text().await.unwrap_or_default();
            let message = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| {
                    v.get("message")
                        .and_then(|m| m.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| body.chars().take(200).collect());
            Err(LauncherError::Http {
                status: status.as_u16(),
                message,
            })
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    /// One canned HTTP response per request path; records request headers.
    pub(crate) struct FakeServer {
        pub base: String,
        pub requests: Arc<Mutex<Vec<String>>>,
    }

    pub(crate) type Route = (String, u16, Vec<(String, String)>, Vec<u8>);

    impl FakeServer {
        pub(crate) async fn start(routes: Vec<Route>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
            let base = format!("http://{}", listener.local_addr().expect("addr"));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let recorded = requests.clone();
            let routes = Arc::new(routes);
            tokio::spawn(async move {
                loop {
                    let Ok((mut socket, _)) = listener.accept().await else {
                        return;
                    };
                    let routes = routes.clone();
                    let recorded = recorded.clone();
                    tokio::spawn(async move {
                        let mut buffer = Vec::new();
                        let mut chunk = [0u8; 4096];
                        while !buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                            match socket.read(&mut chunk).await {
                                Ok(0) | Err(_) => return,
                                Ok(n) => buffer.extend_from_slice(&chunk[..n]),
                            }
                        }
                        let request = String::from_utf8_lossy(&buffer).to_string();
                        let path = request
                            .split_whitespace()
                            .nth(1)
                            .unwrap_or_default()
                            .to_string();
                        recorded.lock().expect("lock").push(request);
                        let (status, headers, body) = routes
                            .iter()
                            .find(|(route, ..)| *route == path)
                            .map(|(_, s, h, b)| (*s, h.clone(), b.clone()))
                            .unwrap_or((404, vec![], b"{\"message\":\"Not Found\"}".to_vec()));
                        let mut head = format!(
                            "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n",
                            body.len()
                        );
                        for (name, value) in headers {
                            head.push_str(&format!("{name}: {value}\r\n"));
                        }
                        head.push_str("\r\n");
                        if socket.write_all(head.as_bytes()).await.is_ok() {
                            socket.write_all(&body).await.ok();
                        }
                    });
                }
            });
            Self { base, requests }
        }
    }

    fn release_page(start: usize, count: usize) -> Vec<u8> {
        let items: Vec<String> = (start..start + count)
            .map(|i| format!(r#"{{"tag_name":"api-v0.0.{i}","draft":false,"prerelease":false,"html_url":"","assets":[]}}"#))
            .collect();
        format!("[{}]", items.join(",")).into_bytes()
    }

    #[tokio::test]
    async fn fetches_catalog_as_raw_contents() {
        let body = br#"{"apps":[{"id":"term","status":"beta","summary":"Terminal"}]}"#.to_vec();
        let server = FakeServer::start(vec![(
            "/repos/o/r/contents/catalog.json".into(),
            200,
            vec![],
            body,
        )])
        .await;
        let client = GitHubClient::new("o", "r", None)
            .expect("client")
            .with_api_base(&server.base);
        let catalog = client.fetch_catalog().await.expect("catalog");
        assert_eq!(
            catalog.entry(crate::AppId::TERM).map(|e| e.status),
            Some(crate::AppStatus::Beta)
        );
        let request = server.requests.lock().expect("lock")[0].to_lowercase();
        assert!(request.contains("accept: application/vnd.github.raw+json"));

        let missing = GitHubClient::new("o", "missing", None)
            .expect("client")
            .with_api_base(&server.base);
        assert!(matches!(
            missing.fetch_catalog().await,
            Err(LauncherError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn lists_releases_across_pages_with_token() {
        let server = FakeServer::start(vec![
            (
                "/repos/o/r/releases?per_page=100&page=1".into(),
                200,
                vec![],
                release_page(0, 100),
            ),
            (
                "/repos/o/r/releases?per_page=100&page=2".into(),
                200,
                vec![],
                release_page(100, 3),
            ),
        ])
        .await;
        let token = GitHubToken::new("tok123", crate::TokenSource::Environment);
        let client = GitHubClient::new("o", "r", Some(token))
            .expect("client")
            .with_api_base(&server.base);
        let releases = client.list_releases().await.expect("list");
        assert_eq!(releases.len(), 103);
        let requests = server.requests.lock().expect("lock");
        assert_eq!(requests.len(), 2);
        assert!(
            requests[0]
                .to_lowercase()
                .contains("authorization: bearer tok123")
        );
    }

    #[tokio::test]
    async fn retries_without_a_rejected_token() {
        let server = FakeServer::start(vec![(
            "/repos/o/r/contents/catalog.json".into(),
            401,
            vec![],
            b"{}".to_vec(),
        )])
        .await;
        let token = GitHubToken::new("expired", crate::TokenSource::Keychain);
        let client = GitHubClient::new("o", "r", Some(token))
            .expect("client")
            .with_api_base(&server.base);
        assert!(matches!(
            client.fetch_catalog().await,
            Err(LauncherError::Unauthorized)
        ));
        let requests = server.requests.lock().expect("lock");
        assert_eq!(requests.len(), 2);
        assert!(
            requests[0]
                .to_lowercase()
                .contains("authorization: bearer expired")
        );
        assert!(!requests[1].to_lowercase().contains("authorization"));
    }

    #[tokio::test]
    async fn maps_error_statuses() {
        let server = FakeServer::start(vec![
            (
                "/repos/limited/r/releases?per_page=100&page=1".into(),
                403,
                vec![
                    ("x-ratelimit-remaining".into(), "0".into()),
                    ("x-ratelimit-reset".into(), "1900000000".into()),
                ],
                b"{}".to_vec(),
            ),
            (
                "/repos/auth/r/releases?per_page=100&page=1".into(),
                401,
                vec![],
                b"{}".to_vec(),
            ),
            (
                "/repos/boom/r/releases?per_page=100&page=1".into(),
                500,
                vec![],
                br#"{"message":"Server Error"}"#.to_vec(),
            ),
        ])
        .await;
        let client = |owner: &str| {
            GitHubClient::new(owner, "r", None)
                .expect("client")
                .with_api_base(&server.base)
        };
        assert!(matches!(
            client("limited").list_releases().await,
            Err(LauncherError::RateLimited { reset: Some(_) })
        ));
        assert!(matches!(
            client("auth").list_releases().await,
            Err(LauncherError::Unauthorized)
        ));
        assert!(matches!(
            client("missing").list_releases().await,
            Err(LauncherError::NotFound(_))
        ));
        match client("boom").list_releases().await {
            Err(LauncherError::Http { status, message }) => {
                assert_eq!(status, 500);
                assert_eq!(message, "Server Error");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn downloads_with_progress_and_hash() {
        let body = b"hello ferrum".to_vec();
        let server = FakeServer::start(vec![("/asset/1".into(), 200, vec![], body.clone())]).await;
        let client = GitHubClient::new("o", "r", None)
            .expect("client")
            .with_api_base(&server.base);
        let asset = Asset {
            name: "a.tar.gz".into(),
            size: body.len() as u64,
            url: format!("{}/asset/1", server.base),
            download_count: 0,
        };
        let dir = tempfile::tempdir().expect("tmp");
        let destination = dir.path().join("a.tar.gz");
        let last = Arc::new(AtomicU64::new(0));
        let seen = last.clone();
        let sha = client
            .download_to(
                &asset,
                &destination,
                Arc::new(move |done, _| seen.store(done, Ordering::SeqCst)),
                CancellationToken::new(),
            )
            .await
            .expect("download");
        assert_eq!(std::fs::read(&destination).expect("read"), body);
        assert_eq!(last.load(Ordering::SeqCst), body.len() as u64);
        assert_eq!(sha, to_hex(&Sha256::digest(&body)));
        let requests = server.requests.lock().expect("lock");
        assert!(
            requests[0]
                .to_lowercase()
                .contains("accept: application/octet-stream")
        );
    }

    #[tokio::test]
    async fn cancelled_download_leaves_no_file() {
        let server = FakeServer::start(vec![("/asset/1".into(), 200, vec![], vec![0; 16])]).await;
        let client = GitHubClient::new("o", "r", None)
            .expect("client")
            .with_api_base(&server.base);
        let asset = Asset {
            name: "a".into(),
            size: 16,
            url: format!("{}/asset/1", server.base),
            download_count: 0,
        };
        let dir = tempfile::tempdir().expect("tmp");
        let destination = dir.path().join("a");
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = client
            .download_to(&asset, &destination, Arc::new(|_, _| {}), cancel)
            .await;
        assert!(matches!(result, Err(LauncherError::Cancelled)));
        assert!(!destination.exists());
    }

    #[test]
    fn rejects_bad_repo_names() {
        assert!(GitHubClient::new("Amygdal-doo", "ferrum", None).is_ok());
        assert!(GitHubClient::new("", "ferrum", None).is_err());
        assert!(GitHubClient::new("a/b", "ferrum", None).is_err());
    }

    #[tokio::test]
    async fn fetches_png_logos_without_the_token() {
        let png = b"\x89PNG\r\n\x1a\nrest".to_vec();
        let server = FakeServer::start(vec![
            ("/logos/forge.png".into(), 200, vec![], png.clone()),
            ("/logos/html.png".into(), 200, vec![], b"<html>".to_vec()),
        ])
        .await;
        let token = GitHubToken::new("secret123", crate::TokenSource::Environment);
        let client = GitHubClient::new("o", "r", Some(token)).expect("client");
        let logo = client
            .fetch_logo(&format!("{}/logos/forge.png", server.base))
            .await
            .expect("logo");
        assert_eq!(logo, png);
        assert!(
            client
                .fetch_logo(&format!("{}/logos/html.png", server.base))
                .await
                .is_err()
        );
        assert!(
            client
                .fetch_logo(&format!("{}/logos/missing.png", server.base))
                .await
                .is_err()
        );
        let requests = server.requests.lock().expect("lock");
        assert!(
            requests
                .iter()
                .all(|r| !r.to_lowercase().contains("authorization"))
        );
    }
}
