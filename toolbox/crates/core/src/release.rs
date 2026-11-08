//! Releases as published on GitHub, and the naming conventions that tie a
//! release to an app, a version and a platform.

use crate::catalog::AppId;
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

#[must_use]
pub fn is_macos_target(target: &str) -> bool {
    target.ends_with("-apple-darwin")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    /// API URL; download with `Accept: application/octet-stream` (works for private repos).
    pub url: String,
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

    /// The archive for `target` and its optional `.sha256` companion. On
    /// macOS the `.app` bundle archive wins over the plain one.
    #[must_use]
    pub fn assets_for(&self, target: &str) -> Option<(&Asset, Option<&Asset>)> {
        let mut names = Vec::with_capacity(2);
        if is_macos_target(target) {
            names.push(bundle_archive_name(self.app, &self.version, target));
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
    size: u64,
    url: String,
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

/// Newest release of `app`, skipping prereleases unless `include_prereleases`.
#[must_use]
pub fn latest(
    releases: &[ReleaseInfo],
    app: AppId,
    include_prereleases: bool,
) -> Option<&ReleaseInfo> {
    releases
        .iter()
        .filter(|release| release.app == app)
        .filter(|release| include_prereleases || !release.is_prerelease())
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
    fn latest_respects_prerelease_setting() {
        let releases = parse_releases(RELEASES_JSON).expect("parse");
        let stable = latest(&releases, AppId::Api, false).expect("stable");
        assert_eq!(stable.version, Version::new(0, 2, 0));
        let any = latest(&releases, AppId::Api, true).expect("any");
        assert_eq!(any.version.to_string(), "0.3.0-beta.1");
        assert!(latest(&releases, AppId::Git, true).is_none());
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
