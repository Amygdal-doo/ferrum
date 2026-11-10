//! Resolves the newest stable Ferrum Launcher DMG in the public releases repo.
//!
//! Selection is a pure function over the GitHub API JSON
//! ([`newest_launcher_dmg`]). [`MacosDownload`] wraps it with an injectable
//! fetcher and an in-memory cache, and falls back to [`site::RELEASES_URL`].

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::Deserialize;

use crate::site;

/// Launcher release tags look like `launcher-v0.6.0`. The repo also holds the
/// other apps' releases, so GitHub's "latest release" is not reliable.
pub const LAUNCHER_TAG_PREFIX: &str = "launcher-v";
/// The launcher was called Ferrum Toolbox up to 0.5.0, released as
/// `toolbox-v*`; those (and the `toolbox-v*` bridge releases) still count.
pub const TOOLBOX_TAG_PREFIX: &str = "toolbox-v";
/// Asset name suffix of the Apple Silicon installer.
pub const MACOS_DMG_SUFFIX: &str = "-aarch64.dmg";
/// How long a resolved download URL is reused.
pub const CACHE_TTL: Duration = Duration::from_secs(10 * 60);
/// How long the fallback is reused after a failed lookup, so a GitHub outage
/// or rate limit does not trigger an API call on every click.
pub const FAILURE_TTL: Duration = Duration::from_secs(60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// `launcher-v1.2.3` -> `((1, 2, 3), true)`, `toolbox-v1.2.3` ->
/// `((1, 2, 3), false)`. Anything else (including pre-release suffixes such
/// as `-rc.1`) is `None`. The flag makes a launcher release win a tie with
/// the toolbox bridge release of the same version.
fn launcher_version(tag: &str) -> Option<((u64, u64, u64), bool)> {
    let (rest, is_launcher) = match tag.strip_prefix(LAUNCHER_TAG_PREFIX) {
        Some(rest) => (rest, true),
        None => (tag.strip_prefix(TOOLBOX_TAG_PREFIX)?, false),
    };
    let mut parts = rest.split('.');
    let mut next = || parts.next()?.parse::<u64>().ok();
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some((version, is_launcher))
}

/// Download URL of the `*-aarch64.dmg` asset of the newest non-draft,
/// non-prerelease `launcher-v*` (or older `toolbox-v*`) release in a
/// `GET /repos/{repo}/releases` response. `None` if the JSON is invalid or
/// nothing matches.
pub fn newest_launcher_dmg(json: &str) -> Option<String> {
    let releases: Vec<Release> = serde_json::from_str(json).ok()?;
    releases
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| {
            let version = launcher_version(&r.tag_name)?;
            let dmg = r
                .assets
                .into_iter()
                .find(|a| a.name.ends_with(MACOS_DMG_SUFFIX))?;
            Some((version, dmg.browser_download_url))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, url)| url)
}

pub type FetchFuture = Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;
/// Returns the body of the releases API response.
pub type Fetcher = Arc<dyn Fn() -> FetchFuture + Send + Sync>;

/// Fetches [`site::RELEASES_API_URL`] anonymously (public repo).
pub fn github_fetcher() -> Fetcher {
    let client = reqwest::Client::builder()
        .user_agent(concat!("web-ferrum/", env!("CARGO_PKG_VERSION")))
        .timeout(FETCH_TIMEOUT)
        .build()
        .unwrap_or_default();
    Arc::new(move || {
        let client = client.clone();
        Box::pin(async move {
            let res = client
                .get(site::RELEASES_API_URL)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|e| e.to_string())?;
            res.text().await.map_err(|e| e.to_string())
        })
    })
}

/// Cached resolver behind `GET /download/macos`.
pub struct MacosDownload {
    fetch: Fetcher,
    /// `(expires_at, url)` of the last lookup, successful or not.
    cache: Mutex<Option<(Instant, String)>>,
}

impl std::fmt::Debug for MacosDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MacosDownload").finish_non_exhaustive()
    }
}

impl MacosDownload {
    pub fn new(fetch: Fetcher) -> Self {
        Self {
            fetch,
            cache: Mutex::new(None),
        }
    }

    /// Resolver that queries the public GitHub API.
    pub fn github() -> Self {
        Self::new(github_fetcher())
    }

    fn cached(&self) -> Option<String> {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .as_ref()
            .filter(|(expires, _)| Instant::now() < *expires)
            .map(|(_, url)| url.clone())
    }

    /// Newest launcher DMG URL, or [`site::RELEASES_URL`] if it can't be resolved.
    pub async fn url(&self) -> String {
        if let Some(url) = self.cached() {
            return url;
        }
        let (url, ttl) = match (self.fetch)().await {
            Ok(body) => match newest_launcher_dmg(&body) {
                Some(url) => (url, CACHE_TTL),
                None => {
                    tracing::warn!("no stable launcher DMG found in releases");
                    (site::RELEASES_URL.to_owned(), FAILURE_TTL)
                }
            },
            Err(err) => {
                tracing::warn!(%err, "failed to fetch launcher releases");
                (site::RELEASES_URL.to_owned(), FAILURE_TTL)
            }
        };
        *self.cache.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((Instant::now() + ttl, url.clone()));
        url
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    const SAMPLE: &str = r#"[
      {"tag_name": "api-v0.9.0", "draft": false, "prerelease": false,
       "assets": [{"name": "FerrumAPI-0.9.0-aarch64.dmg",
                   "browser_download_url": "https://dl.test/api-0.9.0.dmg"}]},
      {"tag_name": "toolbox-v0.5.0", "draft": true, "prerelease": false,
       "assets": [{"name": "FerrumToolbox-0.5.0-aarch64.dmg",
                   "browser_download_url": "https://dl.test/draft.dmg"}]},
      {"tag_name": "toolbox-v0.4.0-rc.1", "draft": false, "prerelease": true,
       "assets": [{"name": "FerrumToolbox-0.4.0-rc.1-aarch64.dmg",
                   "browser_download_url": "https://dl.test/rc.dmg"}]},
      {"tag_name": "toolbox-v0.3.10", "draft": false, "prerelease": false,
       "assets": [
         {"name": "toolbox-0.3.10-x86_64-apple-darwin.tar.gz",
          "browser_download_url": "https://dl.test/0.3.10.tar.gz"},
         {"name": "FerrumToolbox-0.3.10-aarch64.dmg",
          "browser_download_url": "https://dl.test/0.3.10.dmg"}]},
      {"tag_name": "toolbox-v0.3.9", "draft": false, "prerelease": false,
       "assets": [{"name": "FerrumToolbox-0.3.9-aarch64.dmg",
                   "browser_download_url": "https://dl.test/0.3.9.dmg"}]},
      {"tag_name": "toolbox-v0.3.11", "draft": false, "prerelease": false,
       "assets": [{"name": "toolbox-0.3.11-linux.tar.gz",
                   "browser_download_url": "https://dl.test/0.3.11.tar.gz"}]}
    ]"#;

    #[test]
    fn parses_launcher_and_toolbox_tags() {
        assert_eq!(launcher_version("launcher-v1.2.3"), Some(((1, 2, 3), true)));
        assert_eq!(launcher_version("toolbox-v1.2.3"), Some(((1, 2, 3), false)));
        assert_eq!(launcher_version("launcher-v1.2"), None);
        assert_eq!(launcher_version("toolbox-v1.2.3.4"), None);
        assert_eq!(launcher_version("launcher-v1.2.3-rc.1"), None);
        assert_eq!(launcher_version("api-v1.2.3"), None);
    }

    #[test]
    fn launcher_release_wins_over_its_toolbox_bridge() {
        let json = r#"[
          {"tag_name": "toolbox-v0.6.0", "draft": false, "prerelease": false,
           "assets": [{"name": "FerrumToolbox-0.6.0-aarch64.dmg",
                       "browser_download_url": "https://dl.test/bridge.dmg"}]},
          {"tag_name": "launcher-v0.6.0", "draft": false, "prerelease": false,
           "assets": [{"name": "FerrumLauncher-0.6.0-aarch64.dmg",
                       "browser_download_url": "https://dl.test/launcher.dmg"}]},
          {"tag_name": "toolbox-v0.5.0", "draft": false, "prerelease": false,
           "assets": [{"name": "FerrumToolbox-0.5.0-aarch64.dmg",
                       "browser_download_url": "https://dl.test/0.5.0.dmg"}]}
        ]"#;
        assert_eq!(
            newest_launcher_dmg(json).as_deref(),
            Some("https://dl.test/launcher.dmg")
        );
    }

    #[test]
    fn picks_newest_stable_dmg() {
        // Skips other apps, drafts, prereleases and releases without a DMG;
        // compares versions numerically (0.3.10 > 0.3.9), not by list order.
        assert_eq!(
            newest_launcher_dmg(SAMPLE).as_deref(),
            Some("https://dl.test/0.3.10.dmg")
        );
    }

    #[test]
    fn none_when_nothing_matches_or_json_is_invalid() {
        assert_eq!(newest_launcher_dmg("[]"), None);
        assert_eq!(
            newest_launcher_dmg(r#"{"message": "API rate limit exceeded"}"#),
            None
        );
        assert_eq!(newest_launcher_dmg("not json"), None);
    }

    fn counting(result: Result<&'static str, &'static str>) -> (Fetcher, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let fetch: Fetcher = Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { result.map(str::to_owned).map_err(str::to_owned) })
        });
        (fetch, calls)
    }

    #[tokio::test]
    async fn resolves_once_then_serves_from_cache() {
        let (fetch, calls) = counting(Ok(SAMPLE));
        let resolver = MacosDownload::new(fetch);
        assert_eq!(resolver.url().await, "https://dl.test/0.3.10.dmg");
        assert_eq!(resolver.url().await, "https://dl.test/0.3.10.dmg");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn falls_back_to_releases_page() {
        let (fetch, calls) = counting(Err("offline"));
        let resolver = MacosDownload::new(fetch);
        assert_eq!(resolver.url().await, site::RELEASES_URL);
        assert_eq!(resolver.url().await, site::RELEASES_URL);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "failure is cached briefly");

        let (fetch, _) = counting(Ok("[]"));
        assert_eq!(MacosDownload::new(fetch).url().await, site::RELEASES_URL);
    }
}
