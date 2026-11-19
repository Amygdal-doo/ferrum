//! Releases of every Ferrum app, from the public releases repo.
//!
//! Pure functions read the GitHub API JSON: [`newest_launcher_dmg`]
//! (behind `/download/macos`), [`newest_installer`] (behind
//! `/download/<app>/<installer>`) and [`changelog`] (behind `/changelog`).
//! [`Releases`] wraps them with an injectable fetcher and one in-memory
//! cache: a good response is reused for [`CACHE_TTL`]; after a failed fetch
//! the last good response (if any) keeps being served, and GitHub is asked
//! again after [`FAILURE_TTL`]. Tests inject a fetcher, so they never touch
//! the network.

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
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    download_count: u64,
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

/// An installer for downloading one app by hand, without the launcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installer {
    MacAppleSilicon,
    MacIntel,
    LinuxDeb,
}

impl Installer {
    pub const ALL: [Installer; 3] = [Self::MacAppleSilicon, Self::MacIntel, Self::LinuxDeb];

    /// Last segment of `/download/<app>/<installer>`.
    pub fn slug(self) -> &'static str {
        match self {
            Self::MacAppleSilicon => "macos-apple-silicon",
            Self::MacIntel => "macos-intel",
            Self::LinuxDeb => "linux-deb",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|i| i.slug() == slug)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::MacAppleSilicon => "macOS, Apple Silicon",
            Self::MacIntel => "macOS, Intel",
            Self::LinuxDeb => "Linux (.deb)",
        }
    }

    /// File type, shown next to the label.
    pub fn file_kind(self) -> &'static str {
        match self {
            Self::MacAppleSilicon | Self::MacIntel => ".dmg disk image",
            Self::LinuxDeb => "Debian and Ubuntu package",
        }
    }

    /// End of the asset name (`.github/workflows/release.yml`).
    fn suffix(self) -> &'static str {
        match self {
            Self::MacAppleSilicon => MACOS_DMG_SUFFIX,
            Self::MacIntel => "-x86_64.dmg",
            Self::LinuxDeb => "_amd64.deb",
        }
    }
}

/// Download URL of `installer` from the newest release of the app whose tags
/// are `<release_id>-v<version>`. GitHub lists releases newest first, so the
/// first match wins. Drafts never count; pre-releases only with
/// `prereleases` (every app but the launcher is still in alpha).
pub fn newest_installer(
    json: &str,
    release_id: &str,
    installer: Installer,
    prereleases: bool,
) -> Option<String> {
    let releases: Vec<Release> = serde_json::from_str(json).ok()?;
    let prefix = format!("{release_id}-v");
    releases
        .into_iter()
        .filter(|r| !r.draft && (prereleases || !r.prerelease))
        .filter(|r| r.tag_name.starts_with(&prefix))
        .find_map(|r| {
            r.assets
                .into_iter()
                .find(|a| a.name.ends_with(installer.suffix()))
        })
        .map(|a| a.browser_download_url)
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

/// Release channel, read from the version and GitHub's prerelease flag
/// (the launcher's rule, see `launcher/README.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Beta,
    Alpha,
}

impl Channel {
    pub fn label(self) -> &'static str {
        match self {
            Channel::Stable => "Stable",
            Channel::Beta => "Beta",
            Channel::Alpha => "Alpha",
        }
    }

    fn of(version: &str, prerelease: bool) -> Self {
        match version.split_once('-') {
            Some((_, pre)) if pre.starts_with("alpha") => Channel::Alpha,
            Some(_) => Channel::Beta,
            None if prerelease => Channel::Beta,
            None => Channel::Stable,
        }
    }
}

/// One published release on `/changelog`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangelogEntry {
    /// App id from the tag (`api`, `editor`, …); `toolbox` reads as `launcher`.
    pub app_id: String,
    pub version: String,
    pub channel: Channel,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// Release notes as written (Markdown-ish plain text).
    pub notes: String,
    /// The release page on GitHub.
    pub url: String,
}

/// `api-v0.2.0` -> `("api", "0.2.0")`. The id is 1–32 of `a-z 0-9 _`
/// starting with a letter; the version starts with a digit.
fn split_tag(tag: &str) -> Option<(&str, &str)> {
    let (id, version) = tag.split_once("-v")?;
    let id_ok = !id.is_empty()
        && id.len() <= 32
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    let version_ok = version.starts_with(|c: char| c.is_ascii_digit());
    (id_ok && version_ok).then_some((id, version))
}

/// Every non-draft `<app>-v<version>` release, newest first. `None` if the
/// JSON is not a release list.
pub fn changelog(json: &str) -> Option<Vec<ChangelogEntry>> {
    let releases: Vec<Release> = serde_json::from_str(json).ok()?;
    let mut entries: Vec<ChangelogEntry> = releases
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let (id, version) = split_tag(&r.tag_name)?;
            let app_id = if id == "toolbox" { "launcher" } else { id };
            let date = r.published_at.as_deref()?.get(..10)?.to_owned();
            Some(ChangelogEntry {
                app_id: app_id.to_owned(),
                version: version.to_owned(),
                channel: Channel::of(version, r.prerelease),
                date,
                notes: r.body.unwrap_or_default().trim().to_owned(),
                url: r.html_url.unwrap_or_else(|| site::RELEASES_URL.to_owned()),
            })
        })
        .collect();
    entries.sort_by(|a, b| b.date.cmp(&a.date));
    Some(entries)
}

/// GitHub download counts of release assets (checksum files excluded).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadTotals {
    pub total: u64,
    /// `(app id, downloads)`, most first; `toolbox` counts as `launcher`.
    pub by_app: Vec<(String, u64)>,
}

/// Download totals over every non-draft `<app>-v<version>` release in the
/// response. `None` if the JSON is not a release list.
pub fn download_totals(json: &str) -> Option<DownloadTotals> {
    let releases: Vec<Release> = serde_json::from_str(json).ok()?;
    let mut by_app: Vec<(String, u64)> = Vec::new();
    for r in releases.into_iter().filter(|r| !r.draft) {
        let Some((id, _)) = split_tag(&r.tag_name) else {
            continue;
        };
        let id = if id == "toolbox" { "launcher" } else { id };
        let count: u64 = r
            .assets
            .iter()
            .filter(|a| !a.name.ends_with(".sha256"))
            .map(|a| a.download_count)
            .sum();
        match by_app.iter_mut().find(|(app, _)| app == id) {
            Some((_, n)) => *n += count,
            None => by_app.push((id.to_owned(), count)),
        }
    }
    by_app.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Some(DownloadTotals {
        total: by_app.iter().map(|(_, n)| n).sum(),
        by_app,
    })
}

/// The last fetch: when to ask GitHub again, and the newest good body.
struct Cached {
    refresh_at: Instant,
    body: Option<Arc<str>>,
}

/// Cached view of the releases repo, shared by `/download/macos`,
/// `/changelog` and the sitemap.
pub struct Releases {
    fetch: Fetcher,
    cache: Mutex<Option<Cached>>,
}

impl std::fmt::Debug for Releases {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Releases").finish_non_exhaustive()
    }
}

impl Releases {
    pub fn new(fetch: Fetcher) -> Self {
        Self {
            fetch,
            cache: Mutex::new(None),
        }
    }

    /// Releases from the public GitHub API.
    pub fn github() -> Self {
        Self::new(github_fetcher())
    }

    /// A source that always fails, as if offline (tests, local previews).
    pub fn offline() -> Self {
        Self::new(Arc::new(|| Box::pin(async { Err("offline".to_owned()) })))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Cached>> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The releases JSON: cached, freshly fetched, or the last good copy
    /// after a failure. `None` when GitHub has never answered.
    async fn body(&self) -> Option<Arc<str>> {
        let stale = {
            let cache = self.lock();
            match cache.as_ref() {
                Some(c) if Instant::now() < c.refresh_at => return c.body.clone(),
                Some(c) => c.body.clone(),
                None => None,
            }
        };
        let (body, ttl) = match (self.fetch)().await {
            Ok(body) => (Some(Arc::from(body)), CACHE_TTL),
            Err(err) => {
                tracing::warn!(%err, "failed to fetch releases");
                (stale, FAILURE_TTL)
            }
        };
        *self.lock() = Some(Cached {
            refresh_at: Instant::now() + ttl,
            body: body.clone(),
        });
        body
    }

    /// Newest launcher DMG URL, or [`site::RELEASES_URL`] if it can't be resolved.
    pub async fn launcher_dmg_url(&self) -> String {
        match self.body().await.as_deref().and_then(newest_launcher_dmg) {
            Some(url) => url,
            None => {
                tracing::warn!("no stable launcher DMG found; linking the releases page");
                site::RELEASES_URL.to_owned()
            }
        }
    }

    /// Every release, newest first; `None` when they can't be loaded.
    /// Where `/download/<app>/<installer>` sends the browser: the file, or
    /// the releases page when it can't be found.
    pub async fn installer_url(
        &self,
        release_id: &str,
        installer: Installer,
        prereleases: bool,
    ) -> String {
        let body = self.body().await;
        match body
            .as_deref()
            .and_then(|json| newest_installer(json, release_id, installer, prereleases))
        {
            Some(url) => url,
            None => {
                tracing::warn!(
                    release_id,
                    installer = installer.slug(),
                    "no installer found; linking the releases page"
                );
                site::RELEASES_URL.to_owned()
            }
        }
    }

    pub async fn changelog(&self) -> Option<Vec<ChangelogEntry>> {
        changelog(&self.body().await?)
    }

    /// GitHub download totals; `None` when releases can't be loaded.
    pub async fn download_totals(&self) -> Option<DownloadTotals> {
        download_totals(&self.body().await?)
    }

    /// Date of the newest release already in the cache, without fetching.
    pub fn newest_cached_date(&self) -> Option<String> {
        let body = self.lock().as_ref()?.body.clone()?;
        changelog(&body)?.into_iter().next().map(|e| e.date)
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
    fn finds_each_installer_of_the_newest_app_release() {
        let json = r#"[
          {"tag_name": "git-v0.10.0", "draft": true, "assets": [
             {"name": "FerrumGIT-0.10.0-aarch64.dmg", "browser_download_url": "draft"}]},
          {"tag_name": "git-v0.9.1-alpha.1", "prerelease": true, "assets": [
             {"name": "FerrumGIT-0.9.1-alpha.1-aarch64.dmg", "browser_download_url": "arm"},
             {"name": "FerrumGIT-0.9.1-alpha.1-aarch64.dmg.sha256", "browser_download_url": "sum"},
             {"name": "FerrumGIT-0.9.1-alpha.1-x86_64.dmg", "browser_download_url": "intel"},
             {"name": "ferrumgit_0.9.1.alpha.1_amd64.deb", "browser_download_url": "deb"}]},
          {"tag_name": "gitx-v1.0.0", "assets": [
             {"name": "X-1.0.0-aarch64.dmg", "browser_download_url": "other app"}]},
          {"tag_name": "git-v0.8.0", "assets": [
             {"name": "FerrumGIT-0.8.0-aarch64.dmg", "browser_download_url": "stable"}]}
        ]"#;
        let find = |i, pre| newest_installer(json, "git", i, pre);
        assert_eq!(
            find(Installer::MacAppleSilicon, true).as_deref(),
            Some("arm")
        );
        assert_eq!(find(Installer::MacIntel, true).as_deref(), Some("intel"));
        assert_eq!(find(Installer::LinuxDeb, true).as_deref(), Some("deb"));
        assert_eq!(
            find(Installer::MacAppleSilicon, false).as_deref(),
            Some("stable")
        );
        assert_eq!(find(Installer::LinuxDeb, false), None);
        assert_eq!(
            newest_installer(json, "api", Installer::LinuxDeb, true),
            None
        );
        assert_eq!(
            newest_installer("not json", "git", Installer::LinuxDeb, true),
            None
        );
    }

    #[test]
    fn installer_slugs_round_trip() {
        for i in Installer::ALL {
            assert_eq!(Installer::from_slug(i.slug()), Some(i));
        }
        assert_eq!(Installer::from_slug("windows"), None);
    }

    #[test]
    fn totals_downloads_per_app_without_checksums() {
        let json = r#"[
          {"tag_name": "api-v0.9.0", "assets": [
             {"name": "a.dmg", "browser_download_url": "u", "download_count": 5},
             {"name": "a.dmg.sha256", "browser_download_url": "u", "download_count": 99}]},
          {"tag_name": "toolbox-v0.3.0", "assets": [
             {"name": "t.tar.gz", "browser_download_url": "u", "download_count": 7}]},
          {"tag_name": "launcher-v0.6.0", "assets": [
             {"name": "l.dmg", "browser_download_url": "u", "download_count": 3}]},
          {"tag_name": "git-v1.0.0", "draft": true, "assets": [
             {"name": "g.dmg", "browser_download_url": "u", "download_count": 50}]},
          {"tag_name": "random", "assets": [
             {"name": "r.dmg", "browser_download_url": "u", "download_count": 50}]}
        ]"#;
        let totals = download_totals(json).expect("a release list");
        assert_eq!(totals.total, 15);
        assert_eq!(
            totals.by_app,
            vec![("launcher".to_owned(), 10), ("api".to_owned(), 5)]
        );
        assert_eq!(download_totals("{}"), None);
    }

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
        let releases = Releases::new(fetch);
        assert_eq!(
            releases.launcher_dmg_url().await,
            "https://dl.test/0.3.10.dmg"
        );
        assert_eq!(
            releases.launcher_dmg_url().await,
            "https://dl.test/0.3.10.dmg"
        );
        assert!(releases.changelog().await.is_some());
        assert_eq!(calls.load(Ordering::SeqCst), 1, "one fetch serves both");
    }

    #[tokio::test]
    async fn falls_back_to_releases_page() {
        let (fetch, calls) = counting(Err("offline"));
        let releases = Releases::new(fetch);
        assert_eq!(releases.launcher_dmg_url().await, site::RELEASES_URL);
        assert_eq!(releases.launcher_dmg_url().await, site::RELEASES_URL);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "failure is cached briefly");
        assert_eq!(releases.changelog().await, None);
        assert_eq!(releases.newest_cached_date(), None);

        let (fetch, _) = counting(Ok("[]"));
        assert_eq!(
            Releases::new(fetch).launcher_dmg_url().await,
            site::RELEASES_URL
        );
    }

    #[tokio::test]
    async fn keeps_the_last_good_copy_when_github_fails() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let fetch: Fetcher = Arc::new(move || {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                if n == 0 {
                    Ok(CHANGELOG.to_owned())
                } else {
                    Err("down".to_owned())
                }
            })
        });
        let releases = Releases::new(fetch);
        assert_eq!(releases.changelog().await.map(|c| c.len()), Some(3));
        // Expire the cache by hand, then fail: the old list is still served.
        if let Some(c) = releases.lock().as_mut() {
            c.refresh_at = Instant::now();
        }
        assert_eq!(releases.changelog().await.map(|c| c.len()), Some(3));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(releases.newest_cached_date().as_deref(), Some("2026-11-10"));
    }

    const CHANGELOG: &str = r#"[
      {"tag_name": "api-v0.2.0-alpha.3", "prerelease": true, "published_at": "2026-11-02T09:00:00Z",
       "body": "Faster sends", "html_url": "https://gh.test/api"},
      {"tag_name": "toolbox-v0.5.0", "published_at": "2026-10-01T09:00:00Z", "body": ""},
      {"tag_name": "launcher-v0.6.0", "published_at": "2026-11-10T09:00:00Z", "body": "Renamed"},
      {"tag_name": "data-v1.0.0", "draft": true, "published_at": "2026-11-11T09:00:00Z"},
      {"tag_name": "not a tag", "published_at": "2026-11-11T09:00:00Z"},
      {"tag_name": "git-v0.1.0"}
    ]"#;

    #[test]
    fn changelog_lists_published_app_releases_newest_first() {
        let entries = changelog(CHANGELOG).unwrap_or_default();
        let tags: Vec<_> = entries
            .iter()
            .map(|e| format!("{}@{}", e.app_id, e.version))
            .collect();
        assert_eq!(
            tags,
            ["launcher@0.6.0", "api@0.2.0-alpha.3", "launcher@0.5.0"]
        );
        assert_eq!(entries[1].channel, Channel::Alpha);
        assert_eq!(entries[1].url, "https://gh.test/api");
        assert_eq!(entries[0].channel, Channel::Stable);
        assert_eq!(entries[2].url, site::RELEASES_URL);
        assert_eq!(changelog("{}"), None);
    }

    #[test]
    fn channels_follow_the_launcher_rules() {
        assert_eq!(Channel::of("1.2.0", false), Channel::Stable);
        assert_eq!(Channel::of("1.2.0", true), Channel::Beta);
        assert_eq!(Channel::of("1.3.0-rc.1", false), Channel::Beta);
        assert_eq!(Channel::of("1.4.0-alpha.2", false), Channel::Alpha);
        assert_eq!(
            split_tag("term-v1.0.0-beta.1"),
            Some(("term", "1.0.0-beta.1"))
        );
        assert_eq!(split_tag("Api-v1.0.0"), None);
        assert_eq!(split_tag("api-vx"), None);
    }
}
