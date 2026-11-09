//! Releases as published on GitHub, and the naming conventions that tie a
//! release to an app, a version and a platform.

use crate::catalog::AppId;
use crate::platform::Platform;
use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};

/// Rust target triple of this build, if releases are published for it.
#[must_use]
pub fn current_target() -> Option<&'static str> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("aarch64-apple-darwin")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("x86_64-apple-darwin")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("x86_64-unknown-linux-gnu")
    } else if cfg!(all(windows, target_arch = "x86_64")) {
        Some("x86_64-pc-windows-msvc")
    } else {
        None
    }
}

/// Splits `api-v0.2.0` into `(Api, 0.2.0)`. Other tags return `None`.
#[must_use]
pub fn parse_tag(tag: &str) -> Option<(AppId, Version)> {
    let (app, version) = tag.split_once("-v")?;
    Some((app.parse().ok()?, Version::parse(version).ok()?))
}

#[must_use]
pub fn tag_for(app: AppId, version: &Version) -> String {
    format!("{app}-v{version}")
}

/// Plain archive: the executables at the archive root.
#[must_use]
pub fn archive_name(app: AppId, version: &Version, target: &str) -> String {
    format!("{app}-{version}-{target}.tar.gz")
}

/// macOS archive holding `<bundle_name>.app` at its root.
#[must_use]
pub fn bundle_archive_name(app: AppId, version: &Version, target: &str) -> String {
    format!("{app}-{version}-{target}.app.tar.gz")
}

/// Windows archive: the `.exe` files (and the app's `.ico`) at the root.
#[must_use]
pub fn zip_archive_name(app: AppId, version: &Version, target: &str) -> String {
    format!("{app}-{version}-{target}.zip")
}

#[must_use]
pub fn is_macos_target(target: &str) -> bool {
    Platform::of_target(target) == Platform::MacOs
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    /// API URL; download with `Accept: application/octet-stream` (works for private repos).
    pub url: String,
    #[serde(default)]
    pub download_count: u64,
}

/// How mature a release is, from its version and GitHub's prerelease flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
    Alpha,
}

impl Channel {
    /// Stable: no semver prerelease part and not marked prerelease on GitHub.
    /// Alpha: a prerelease part starting with `alpha`. Beta: anything else
    /// that is a prerelease (`-beta.N`, `-rc.N`, or the GitHub flag).
    #[must_use]
    pub fn of(version: &Version, github_prerelease: bool) -> Self {
        let first = version.pre.as_str().split('.').next().unwrap_or_default();
        if first.to_ascii_lowercase().starts_with("alpha") {
            Channel::Alpha
        } else if !version.pre.is_empty() || github_prerelease {
            Channel::Beta
        } else {
            Channel::Stable
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Channel::Stable => "Stable",
            Channel::Beta => "Beta",
            Channel::Alpha => "Alpha",
        }
    }
}

/// Which releases an installed app follows for updates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    Stable,
    Beta,
    /// Every Ferrum app is in alpha, so this is where updates come from.
    #[default]
    Alpha,
}

impl UpdateChannel {
    pub const ALL: [UpdateChannel; 3] = [
        UpdateChannel::Stable,
        UpdateChannel::Beta,
        UpdateChannel::Alpha,
    ];

    /// Each channel also takes the more stable ones, so a stable release
    /// newer than the newest beta or alpha wins.
    #[must_use]
    pub fn accepts(self, channel: Channel) -> bool {
        match self {
            UpdateChannel::Stable => channel == Channel::Stable,
            UpdateChannel::Beta => matches!(channel, Channel::Stable | Channel::Beta),
            UpdateChannel::Alpha => true,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            UpdateChannel::Stable => "Stable",
            UpdateChannel::Beta => "Beta",
            UpdateChannel::Alpha => "Alpha",
        }
    }
}

/// A release of one app, as cached in `state.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub app: AppId,
    pub version: Version,
    pub tag: String,
    pub title: String,
    pub notes: String,
    pub prerelease: bool,
    pub published_at: Option<DateTime<Utc>>,
    pub html_url: String,
    pub assets: Vec<Asset>,
}

impl ReleaseInfo {
    #[must_use]
    pub fn is_prerelease(&self) -> bool {
        self.prerelease || !self.version.pre.is_empty()
    }

    #[must_use]
    pub fn channel(&self) -> Channel {
        Channel::of(&self.version, self.prerelease)
    }

    /// Downloads of every build in the release (checksum files excluded).
    #[must_use]
    pub fn downloads(&self) -> u64 {
        self.assets
            .iter()
            .filter(|asset| !asset.name.ends_with(".sha256"))
            .map(|asset| asset.download_count)
            .sum()
    }

    /// The archive for `target` and its optional `.sha256` companion. On
    /// macOS the `.app` bundle archive wins over the plain one; on Windows
    /// the `.zip` does.
    #[must_use]
    pub fn assets_for(&self, target: &str) -> Option<(&Asset, Option<&Asset>)> {
        let mut names = Vec::with_capacity(2);
        match Platform::of_target(target) {
            Platform::MacOs => names.push(bundle_archive_name(self.app, &self.version, target)),
            Platform::Windows => names.push(zip_archive_name(self.app, &self.version, target)),
            Platform::Linux | Platform::Other => {}
        }
        names.push(archive_name(self.app, &self.version, target));
        names.into_iter().find_map(|name| {
            let archive = self.assets.iter().find(|asset| asset.name == name)?;
            let checksum_name = format!("{name}.sha256");
            let checksum = self.assets.iter().find(|asset| asset.name == checksum_name);
            Some((archive, checksum))
        })
    }
}

/// GitHub's release JSON (only the fields we use).
#[derive(Debug, Deserialize)]
pub(crate) struct GitHubRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<DateTime<Utc>>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    #[serde(default)]
    size: u64,
    url: String,
    #[serde(default)]
    download_count: u64,
}

impl GitHubRelease {
    /// `None` for drafts and tags that don't follow `<app>-v<semver>`.
    pub(crate) fn into_release(self) -> Option<ReleaseInfo> {
        if self.draft {
            return None;
        }
        let (app, version) = parse_tag(&self.tag_name)?;
        Some(ReleaseInfo {
            app,
            version,
            title: self
                .name
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| self.tag_name.clone()),
            tag: self.tag_name,
            notes: self.body.unwrap_or_default(),
            prerelease: self.prerelease,
            published_at: self.published_at,
            html_url: self.html_url,
            assets: self
                .assets
                .into_iter()
                .map(|asset| Asset {
                    name: asset.name,
                    size: asset.size,
                    url: asset.url,
                    download_count: asset.download_count,
                })
                .collect(),
        })
    }
}

/// Parses a page of `GET /repos/{owner}/{repo}/releases`, keeping Ferrum app releases.
pub fn parse_releases(json: &str) -> crate::Result<Vec<ReleaseInfo>> {
    let releases: Vec<GitHubRelease> = serde_json::from_str(json)?;
    Ok(releases
        .into_iter()
        .filter_map(GitHubRelease::into_release)
        .collect())
}

/// Newest release of `app` that `channel` follows.
#[must_use]
pub fn latest(
    releases: &[ReleaseInfo],
    app: AppId,
    channel: UpdateChannel,
) -> Option<&ReleaseInfo> {
    releases
        .iter()
        .filter(|release| release.app == app && channel.accepts(release.channel()))
        .max_by(|a, b| a.version.cmp(&b.version))
}

/// Newest release of `app` published exactly on `channel`.
#[must_use]
pub fn latest_in(releases: &[ReleaseInfo], app: AppId, channel: Channel) -> Option<&ReleaseInfo> {
    releases
        .iter()
        .filter(|release| release.app == app && release.channel() == channel)
        .max_by(|a, b| a.version.cmp(&b.version))
}

/// Parses a `.sha256` file: `<hex>` or `<hex>  <file name>`.
pub fn parse_checksum(text: &str) -> Option<String> {
    let hex = text.split_whitespace().next()?.to_ascii_lowercase();
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASES_JSON: &str = r#"[
      {"tag_name":"api-v0.2.0","name":"FerrumAPI 0.2.0","body":"Faster sends","draft":false,"prerelease":false,
       "published_at":"2026-11-01T10:00:00Z","html_url":"https://github.com/o/r/releases/tag/api-v0.2.0",
       "assets":[
         {"name":"api-0.2.0-aarch64-apple-darwin.tar.gz","size":1000,"url":"https://api.github.com/a/1"},
         {"name":"api-0.2.0-aarch64-apple-darwin.tar.gz.sha256","size":90,"url":"https://api.github.com/a/2"},
         {"name":"api-0.2.0-x86_64-unknown-linux-gnu.tar.gz","size":1100,"url":"https://api.github.com/a/3"}
       ]},
      {"tag_name":"api-v0.3.0-beta.1","name":"","body":null,"draft":false,"prerelease":true,"html_url":"","assets":[]},
      {"tag_name":"api-v0.1.0","draft":false,"prerelease":false,"html_url":"","assets":[]},
      {"tag_name":"git-v1.0.0","draft":true,"prerelease":false,"html_url":"","assets":[]},
      {"tag_name":"v1.0.0","draft":false,"prerelease":false,"html_url":"","assets":[]},
      {"tag_name":"zed-v1.0.0","draft":false,"prerelease":false,"html_url":"","assets":[]},
      {"tag_name":"data-vnot-semver","draft":false,"prerelease":false,"html_url":"","assets":[]}
    ]"#;

    #[test]
    fn parses_tags() {
        assert_eq!(
            parse_tag("api-v0.2.0"),
            Some((AppId::Api, Version::new(0, 2, 0)))
        );
        assert_eq!(
            parse_tag("toolbox-v1.2.3-rc.1").map(|(app, v)| (app, v.to_string())),
            Some((AppId::Toolbox, "1.2.3-rc.1".to_string()))
        );
        assert_eq!(parse_tag("v1.0.0"), None);
        assert_eq!(parse_tag("api-1.0.0"), None);
        assert_eq!(parse_tag("unknown-v1.0.0"), None);
        assert_eq!(parse_tag("api-v1.0"), None);
        assert_eq!(tag_for(AppId::Git, &Version::new(1, 0, 0)), "git-v1.0.0");
    }

    #[test]
    fn keeps_only_published_ferrum_releases() {
        let releases = parse_releases(RELEASES_JSON).expect("parse");
        let tags: Vec<_> = releases.iter().map(|r| r.tag.as_str()).collect();
        assert_eq!(tags, ["api-v0.2.0", "api-v0.3.0-beta.1", "api-v0.1.0"]);
        assert_eq!(releases[1].title, "api-v0.3.0-beta.1");
        assert!(releases[1].is_prerelease());
    }

    #[test]
    fn latest_follows_update_channel() {
        let releases = parse_releases(RELEASES_JSON).expect("parse");
        let stable = latest(&releases, AppId::Api, UpdateChannel::Stable).expect("stable");
        assert_eq!(stable.version, Version::new(0, 2, 0));
        let beta = latest(&releases, AppId::Api, UpdateChannel::Beta).expect("beta");
        assert_eq!(beta.version.to_string(), "0.3.0-beta.1");
        assert!(latest(&releases, AppId::Git, UpdateChannel::Beta).is_none());
        assert_eq!(
            latest_in(&releases, AppId::Api, Channel::Beta).map(|r| r.tag.as_str()),
            Some("api-v0.3.0-beta.1")
        );
        assert!(latest_in(&releases, AppId::Api, Channel::Alpha).is_none());
    }

    fn release_with(version: &str, prerelease: bool) -> ReleaseInfo {
        let version = Version::parse(version).expect("semver");
        ReleaseInfo {
            app: AppId::Api,
            tag: tag_for(AppId::Api, &version),
            version,
            title: String::new(),
            notes: String::new(),
            prerelease,
            published_at: None,
            html_url: String::new(),
            assets: Vec::new(),
        }
    }

    #[test]
    fn classifies_channels() {
        let channel = |v: &str, pre: bool| release_with(v, pre).channel();
        assert_eq!(channel("1.0.0", false), Channel::Stable);
        assert_eq!(channel("1.0.0", true), Channel::Beta);
        assert_eq!(channel("1.0.0-beta.2", false), Channel::Beta);
        assert_eq!(channel("1.0.0-rc.1", true), Channel::Beta);
        assert_eq!(channel("1.0.0-alpha.3", false), Channel::Alpha);
        assert_eq!(channel("1.0.0-Alpha", true), Channel::Alpha);
        assert_eq!(channel("1.0.0-dev.1", false), Channel::Beta);
    }

    #[test]
    fn beta_channel_prefers_a_newer_stable_and_never_offers_alpha() {
        let releases = vec![
            release_with("0.3.0-beta.1", true),
            release_with("0.3.0", false),
            release_with("0.4.0-alpha.1", true),
        ];
        let beta = latest(&releases, AppId::Api, UpdateChannel::Beta).expect("beta");
        assert_eq!(beta.version, Version::new(0, 3, 0));
        let stable = latest(&releases, AppId::Api, UpdateChannel::Stable).expect("stable");
        assert_eq!(stable.version, Version::new(0, 3, 0));
        assert!(UpdateChannel::Beta.accepts(Channel::Stable));
        assert!(!UpdateChannel::Beta.accepts(Channel::Alpha));
        assert!(!UpdateChannel::Stable.accepts(Channel::Beta));
    }

    #[test]
    fn alpha_channel_takes_every_release_newest_first() {
        let mut releases = vec![
            release_with("0.3.0", false),
            release_with("0.4.0-alpha.1", false),
            release_with("0.4.0-alpha.2", false),
        ];
        let alpha = latest(&releases, AppId::Api, UpdateChannel::Alpha).expect("alpha");
        assert_eq!(alpha.version.to_string(), "0.4.0-alpha.2");
        releases.push(release_with("0.4.0", false));
        let alpha = latest(&releases, AppId::Api, UpdateChannel::Alpha).expect("alpha");
        assert_eq!(alpha.version, Version::new(0, 4, 0));
        assert_eq!(UpdateChannel::default(), UpdateChannel::Alpha);
    }

    #[test]
    fn sums_downloads_without_checksums() {
        let json = r#"[{"tag_name":"data-v0.1.0","draft":false,"prerelease":false,"html_url":"",
          "published_at":"2026-11-09T00:30:00Z",
          "assets":[
            {"name":"data-0.1.0-aarch64-apple-darwin.app.tar.gz","size":10,"url":"u1","download_count":4},
            {"name":"data-0.1.0-aarch64-apple-darwin.app.tar.gz.sha256","size":1,"url":"u2","download_count":4},
            {"name":"FerrumData-0.1.0-aarch64.dmg","size":12,"url":"u3","download_count":7}
          ]}]"#;
        let release = parse_releases(json).expect("parse").remove(0);
        assert_eq!(release.downloads(), 11);
        assert_eq!(release.assets[0].download_count, 4);
        assert!(release.published_at.is_some());
    }

    #[test]
    fn selects_asset_for_target() {
        let releases = parse_releases(RELEASES_JSON).expect("parse");
        let (archive, checksum) = releases[0]
            .assets_for("aarch64-apple-darwin")
            .expect("mac build");
        assert_eq!(archive.url, "https://api.github.com/a/1");
        assert_eq!(
            checksum.map(|c| c.url.as_str()),
            Some("https://api.github.com/a/2")
        );
        let (_, checksum) = releases[0]
            .assets_for("x86_64-unknown-linux-gnu")
            .expect("linux build");
        assert!(checksum.is_none());
        assert!(releases[0].assets_for("x86_64-apple-darwin").is_none());
    }

    #[test]
    fn prefers_app_bundle_on_macos_only() {
        let asset = |name: &str| Asset {
            name: name.into(),
            size: 1,
            url: format!("https://api.github.com/{name}"),
            download_count: 0,
        };
        let mut release = parse_releases(RELEASES_JSON).expect("parse").remove(0);
        release.assets = vec![
            asset("api-0.2.0-aarch64-apple-darwin.tar.gz"),
            asset("api-0.2.0-aarch64-apple-darwin.app.tar.gz"),
            asset("api-0.2.0-aarch64-apple-darwin.app.tar.gz.sha256"),
            asset("api-0.2.0-x86_64-unknown-linux-gnu.tar.gz"),
            asset("api-0.2.0-x86_64-unknown-linux-gnu.app.tar.gz"),
        ];
        let (archive, checksum) = release.assets_for("aarch64-apple-darwin").expect("mac");
        assert_eq!(archive.name, "api-0.2.0-aarch64-apple-darwin.app.tar.gz");
        assert_eq!(
            checksum.map(|c| c.name.as_str()),
            Some("api-0.2.0-aarch64-apple-darwin.app.tar.gz.sha256")
        );
        let (archive, _) = release
            .assets_for("x86_64-unknown-linux-gnu")
            .expect("linux");
        assert_eq!(archive.name, "api-0.2.0-x86_64-unknown-linux-gnu.tar.gz");

        release.assets.retain(|a| !a.name.contains(".app."));
        let (archive, _) = release
            .assets_for("aarch64-apple-darwin")
            .expect("fallback");
        assert_eq!(archive.name, "api-0.2.0-aarch64-apple-darwin.tar.gz");
    }

    #[test]
    fn prefers_zip_on_windows_and_ignores_installers() {
        let asset = |name: &str| Asset {
            name: name.into(),
            size: 1,
            url: format!("https://api.github.com/{name}"),
            download_count: 0,
        };
        let mut release = parse_releases(RELEASES_JSON).expect("parse").remove(0);
        release.assets = vec![
            asset("FerrumAPI-0.2.0-x86_64-setup.exe"),
            asset("api-0.2.0-x86_64-pc-windows-msvc.tar.gz"),
            asset("api-0.2.0-x86_64-pc-windows-msvc.zip"),
            asset("api-0.2.0-x86_64-pc-windows-msvc.zip.sha256"),
            asset("ferrumapi_0.2.0_amd64.deb"),
            asset("FerrumAPI-0.2.0-aarch64.dmg"),
        ];
        let (archive, checksum) = release
            .assets_for("x86_64-pc-windows-msvc")
            .expect("windows");
        assert_eq!(archive.name, "api-0.2.0-x86_64-pc-windows-msvc.zip");
        assert_eq!(
            checksum.map(|c| c.name.as_str()),
            Some("api-0.2.0-x86_64-pc-windows-msvc.zip.sha256")
        );
        // Installers are for people, never picked by the toolbox.
        assert!(release.assets_for("x86_64-unknown-linux-gnu").is_none());
        assert!(release.assets_for("aarch64-apple-darwin").is_none());
    }

    #[test]
    fn parses_checksum_files() {
        let hex = "a".repeat(64);
        assert_eq!(parse_checksum(&hex), Some(hex.clone()));
        assert_eq!(
            parse_checksum(&format!("{}  api-0.2.0.tar.gz\n", hex.to_uppercase())),
            Some(hex)
        );
        assert_eq!(parse_checksum("xyz"), None);
        assert_eq!(parse_checksum(""), None);
    }
}
