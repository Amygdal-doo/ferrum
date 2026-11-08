//! Local state: settings, installed versions, the active version per app and
//! the cached release list. Persisted as `state.json` (never holds the token).

use crate::catalog::AppId;
use crate::github::{DownloadProgress, GitHubClient};
use crate::release::{self, ReleaseInfo};
use crate::{Result, ToolboxError, install};
use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

/// Where the toolbox keeps its files.
#[derive(Debug, Clone)]
pub struct Paths {
    /// `state.json`, `apps/<id>/<version>/` and `downloads/`.
    pub data_dir: PathBuf,
    /// Symlinks to the active binaries (`~/.ferrum/bin`).
    pub bin_dir: PathBuf,
}

impl Paths {
    /// Platform defaults: the OS app-data dir and `~/.ferrum/bin`.
    #[must_use]
    pub fn default_for_user() -> Option<Self> {
        let project = directories::ProjectDirs::from("dev", "Ferrum", "FerrumToolbox")?;
        let home = directories::BaseDirs::new()?;
        Some(Self {
            data_dir: project.data_dir().to_path_buf(),
            bin_dir: home.home_dir().join(".ferrum").join("bin"),
        })
    }

    #[must_use]
    pub fn state_file(&self) -> PathBuf {
        self.data_dir.join("state.json")
    }

    #[must_use]
    pub fn version_dir(&self, app: AppId, version: &Version) -> PathBuf {
        self.data_dir
            .join("apps")
            .join(app.as_str())
            .join(version.to_string())
    }

    #[must_use]
    pub fn downloads_dir(&self) -> PathBuf {
        self.data_dir.join("downloads")
    }

    #[must_use]
    pub fn binary_path(&self, app: AppId, version: &Version) -> PathBuf {
        self.version_dir(app, version).join(app.info().binary)
    }

    #[must_use]
    pub fn link_path(&self, app: AppId) -> PathBuf {
        self.bin_dir.join(app.info().binary)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub repo_owner: String,
    pub repo_name: String,
    pub check_interval_hours: u32,
    /// Versions to keep per app, including the active one. `0` keeps all.
    pub keep_versions: u32,
    pub include_prereleases: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            repo_owner: "Amygdal-doo".into(),
            repo_name: "ferrum".into(),
            check_interval_hours: 6,
            keep_versions: 3,
            include_prereleases: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledVersion {
    pub version: Version,
    pub tag: String,
    pub asset: String,
    pub size: u64,
    pub installed_at: DateTime<Utc>,
    pub notes: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    pub installed: Vec<InstalledVersion>,
    pub active: Option<Version>,
}

impl AppState {
    #[must_use]
    pub fn is_installed(&self, version: &Version) -> bool {
        self.installed.iter().any(|v| &v.version == version)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub settings: Settings,
    pub apps: BTreeMap<AppId, AppState>,
    pub last_check: Option<DateTime<Utc>>,
    pub releases: Vec<ReleaseInfo>,
}

impl State {
    /// Loads `state.json`; a missing file yields defaults.
    pub fn load(paths: &Paths) -> Result<Self> {
        match std::fs::read(paths.state_file()) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Writes via a temp file + rename so the file is never half-written.
    pub fn save(&self, paths: &Paths) -> Result<()> {
        std::fs::create_dir_all(&paths.data_dir)?;
        let path = paths.state_file();
        let temp = path.with_extension(format!("json.{}.tmp", uuid::Uuid::new_v4()));
        let bytes = serde_json::to_vec_pretty(self)?;
        {
            use std::io::Write;
            let mut file = std::fs::File::create(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        if let Err(e) = std::fs::rename(&temp, &path) {
            std::fs::remove_file(&temp).ok();
            return Err(e.into());
        }
        Ok(())
    }

    #[must_use]
    pub fn app(&self, app: AppId) -> AppState {
        self.apps.get(&app).cloned().unwrap_or_default()
    }

    #[must_use]
    pub fn active(&self, app: AppId) -> Option<&Version> {
        self.apps.get(&app).and_then(|state| state.active.as_ref())
    }

    #[must_use]
    pub fn releases_for(&self, app: AppId) -> Vec<&ReleaseInfo> {
        let mut releases: Vec<_> = self
            .releases
            .iter()
            .filter(|release| release.app == app)
            .filter(|release| self.settings.include_prereleases || !release.is_prerelease())
            .collect();
        releases.sort_by(|a, b| b.version.cmp(&a.version));
        releases
    }

    #[must_use]
    pub fn latest(&self, app: AppId) -> Option<&ReleaseInfo> {
        release::latest(&self.releases, app, self.settings.include_prereleases)
    }

    /// The newer release if `app` is installed and behind the latest.
    #[must_use]
    pub fn update_for(&self, app: AppId) -> Option<&ReleaseInfo> {
        let active = self.active(app)?;
        self.latest(app).filter(|latest| &latest.version > active)
    }

    #[must_use]
    pub fn updates(&self) -> Vec<(AppId, Version)> {
        AppId::ALL
            .into_iter()
            .filter_map(|app| self.update_for(app).map(|r| (app, r.version.clone())))
            .collect()
    }

    pub fn record_releases(&mut self, releases: Vec<ReleaseInfo>) {
        self.releases = releases;
        self.last_check = Some(Utc::now());
    }

    /// Adds (or replaces) an installed version; the first install becomes active.
    pub fn record_install(&mut self, app: AppId, installed: InstalledVersion) {
        let state = self.apps.entry(app).or_default();
        state.installed.retain(|v| v.version != installed.version);
        if state.active.is_none() {
            state.active = Some(installed.version.clone());
        }
        state.installed.push(installed);
        state.installed.sort_by(|a, b| b.version.cmp(&a.version));
    }

    /// Marks `version` active and points `~/.ferrum/bin/<binary>` at it.
    pub fn activate(&mut self, app: AppId, version: &Version, paths: &Paths) -> Result<()> {
        let state = self.apps.entry(app).or_default();
        if !state.is_installed(version) {
            return Err(ToolboxError::Invalid(format!(
                "{} {version} is not installed",
                app.info().name
            )));
        }
        state.active = Some(version.clone());
        link_active(paths, app, version)
    }

    /// Removes an installed version. The active one can only be removed when
    /// it is the last installed version.
    pub fn uninstall(&mut self, app: AppId, version: &Version, paths: &Paths) -> Result<()> {
        let state = self.apps.entry(app).or_default();
        if !state.is_installed(version) {
            return Err(ToolboxError::Invalid(format!(
                "{} {version} is not installed",
                app.info().name
            )));
        }
        let is_active = state.active.as_ref() == Some(version);
        if is_active && state.installed.len() > 1 {
            return Err(ToolboxError::Invalid(format!(
                "{version} is the active version; activate another version first"
            )));
        }
        let dir = paths.version_dir(app, version);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        state.installed.retain(|v| &v.version != version);
        if is_active {
            state.active = None;
            unlink(paths, app)?;
        }
        Ok(())
    }

    /// Versions `prune` would delete: everything beyond the newest
    /// `keep_versions`, never the active one.
    #[must_use]
    pub fn prune_candidates(&self, app: AppId) -> Vec<Version> {
        let keep = self.settings.keep_versions as usize;
        let state = self.app(app);
        if keep == 0 || state.installed.len() <= keep {
            return Vec::new();
        }
        let mut versions: Vec<Version> =
            state.installed.iter().map(|v| v.version.clone()).collect();
        versions.sort_by(|a, b| b.cmp(a));
        let mut kept: Vec<Version> = versions.iter().take(keep).cloned().collect();
        if let Some(active) = &state.active
            && !kept.contains(active)
        {
            kept.pop();
            kept.push(active.clone());
        }
        versions.into_iter().filter(|v| !kept.contains(v)).collect()
    }

    pub fn prune(&mut self, app: AppId, paths: &Paths) -> Result<Vec<Version>> {
        let removed = self.prune_candidates(app);
        for version in &removed {
            self.uninstall(app, version, paths)?;
        }
        Ok(removed)
    }
}

#[cfg(unix)]
fn link_active(paths: &Paths, app: AppId, version: &Version) -> Result<()> {
    std::fs::create_dir_all(&paths.bin_dir)?;
    let link = paths.link_path(app);
    let temp = link.with_extension(format!("link-{}", uuid::Uuid::new_v4()));
    std::os::unix::fs::symlink(paths.binary_path(app, version), &temp)?;
    if let Err(e) = std::fs::rename(&temp, &link) {
        std::fs::remove_file(&temp).ok();
        return Err(e.into());
    }
    Ok(())
}

#[cfg(not(unix))]
fn link_active(_: &Paths, _: AppId, _: &Version) -> Result<()> {
    Ok(())
}

fn unlink(paths: &Paths, app: AppId) -> Result<()> {
    match std::fs::remove_file(paths.link_path(app)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Downloads, verifies and unpacks `release` for this platform. Returns the
/// record to pass to [`State::record_install`]. Blocking extraction runs on a
/// blocking thread.
pub async fn install_release(
    client: &GitHubClient,
    paths: &Paths,
    release: &ReleaseInfo,
    progress: DownloadProgress,
    cancel: CancellationToken,
) -> Result<InstalledVersion> {
    let target = release::current_target().ok_or(ToolboxError::UnsupportedPlatform)?;
    let (archive, checksum) =
        release
            .assets_for(target)
            .ok_or_else(|| ToolboxError::NoAssetForTarget {
                app: release.app.info().name.to_string(),
                version: release.version.to_string(),
                target: target.to_string(),
            })?;

    let download =
        paths
            .downloads_dir()
            .join(format!("{}.{}.part", archive.name, uuid::Uuid::new_v4()));
    let actual = client
        .download_to(archive, &download, progress, cancel)
        .await?;

    let verified = match checksum {
        Some(checksum) => match client.download_text(checksum).await {
            Ok(text) => install::verify_checksum(&archive.name, &text, &actual),
            Err(e) => Err(e),
        },
        None => {
            tracing::warn!(asset = %archive.name, "release has no .sha256; skipping checksum");
            Ok(())
        }
    };
    if let Err(e) = verified {
        tokio::fs::remove_file(&download).await.ok();
        return Err(e);
    }

    let destination = paths.version_dir(release.app, &release.version);
    let binary = release.app.info().binary;
    let archive_path = download.clone();
    let extracted = tokio::task::spawn_blocking(move || {
        install::install_archive(&archive_path, &destination, binary)
    })
    .await
    .map_err(|e| ToolboxError::Invalid(format!("install task failed: {e}")))?;
    tokio::fs::remove_file(&download).await.ok();
    extracted?;

    Ok(InstalledVersion {
        version: release.version.clone(),
        tag: release.tag.clone(),
        asset: archive.name.clone(),
        size: archive.size,
        installed_at: Utc::now(),
        notes: release.notes.clone(),
    })
}

/// Starts the active version detached from the toolbox.
pub fn launch(state: &State, app: AppId, paths: &Paths) -> Result<()> {
    let version = state
        .active(app)
        .ok_or_else(|| ToolboxError::Invalid(format!("{} is not installed", app.info().name)))?;
    let binary = paths.binary_path(app, version);
    if !binary.is_file() {
        return Err(ToolboxError::MissingBinary(binary.display().to_string()));
    }
    spawn_detached(&binary, &paths.version_dir(app, version))
}

fn spawn_detached(binary: &Path, working_dir: &Path) -> Result<()> {
    let mut command = std::process::Command::new(binary);
    command
        .current_dir(working_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::tests::FakeServer;
    use crate::install::tests::make_archive;
    use crate::release::Asset;
    use sha2::{Digest, Sha256};
    use std::sync::Arc;

    fn paths(dir: &Path) -> Paths {
        Paths {
            data_dir: dir.join("data"),
            bin_dir: dir.join("bin"),
        }
    }

    fn installed(version: &str) -> InstalledVersion {
        InstalledVersion {
            version: Version::parse(version).expect("semver"),
            tag: format!("api-v{version}"),
            asset: "a.tar.gz".into(),
            size: 1,
            installed_at: Utc::now(),
            notes: String::new(),
        }
    }

    fn release(app: AppId, version: &str, prerelease: bool) -> ReleaseInfo {
        ReleaseInfo {
            app,
            version: Version::parse(version).expect("semver"),
            tag: format!("{app}-v{version}"),
            title: String::new(),
            notes: String::new(),
            prerelease,
            published_at: None,
            html_url: String::new(),
            assets: vec![],
        }
    }

    fn fake_install(state: &mut State, paths: &Paths, app: AppId, version: &str) {
        let v = Version::parse(version).expect("semver");
        let dir = paths.version_dir(app, &v);
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join(app.info().binary), b"bin").expect("write");
        state.record_install(app, installed(version));
    }

    #[test]
    fn state_round_trips_and_missing_file_is_default() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        assert_eq!(State::load(&paths).expect("load"), State::default());

        let mut state = State::default();
        state.settings.repo_owner = "me".into();
        state.settings.keep_versions = 5;
        state.record_install(AppId::Api, installed("0.2.0"));
        state.record_releases(vec![release(AppId::Api, "0.3.0", false)]);
        state.save(&paths).expect("save");

        let loaded = State::load(&paths).expect("load");
        assert_eq!(loaded, state);
        let json = std::fs::read_to_string(paths.state_file()).expect("read");
        assert!(!json.to_lowercase().contains("token"));
    }

    #[test]
    fn reports_updates_only_for_installed_apps() {
        let mut state = State::default();
        state.record_releases(vec![
            release(AppId::Api, "0.3.0", false),
            release(AppId::Api, "0.4.0-beta.1", true),
            release(AppId::Git, "1.0.0", false),
        ]);
        state.record_install(AppId::Api, installed("0.2.0"));
        assert_eq!(state.updates(), vec![(AppId::Api, Version::new(0, 3, 0))]);
        state.settings.include_prereleases = true;
        assert_eq!(
            state.updates(),
            vec![(AppId::Api, Version::parse("0.4.0-beta.1").expect("semver"))]
        );
        assert_eq!(state.releases_for(AppId::Api).len(), 2);
    }

    #[test]
    fn activate_links_binary_and_uninstall_guards_active() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let mut state = State::default();
        fake_install(&mut state, &paths, AppId::Api, "0.1.0");
        fake_install(&mut state, &paths, AppId::Api, "0.2.0");
        let v1 = Version::new(0, 1, 0);
        let v2 = Version::new(0, 2, 0);
        assert_eq!(state.active(AppId::Api), Some(&v1));

        state.activate(AppId::Api, &v2, &paths).expect("activate");
        assert_eq!(state.active(AppId::Api), Some(&v2));
        #[cfg(unix)]
        assert_eq!(
            std::fs::read_link(paths.link_path(AppId::Api)).expect("link"),
            paths.binary_path(AppId::Api, &v2)
        );

        assert!(state.uninstall(AppId::Api, &v2, &paths).is_err());
        state
            .uninstall(AppId::Api, &v1, &paths)
            .expect("remove inactive");
        assert!(!paths.version_dir(AppId::Api, &v1).exists());
        state
            .uninstall(AppId::Api, &v2, &paths)
            .expect("remove last");
        assert_eq!(state.active(AppId::Api), None);
        assert!(!paths.link_path(AppId::Api).exists());
        assert!(state.activate(AppId::Api, &v2, &paths).is_err());
    }

    #[test]
    fn prune_keeps_newest_and_active() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let mut state = State::default();
        for v in ["0.1.0", "0.2.0", "0.3.0", "0.4.0", "0.5.0"] {
            fake_install(&mut state, &paths, AppId::Api, v);
        }
        // 0.1.0 is active (first install); keep 3 → keep 0.5, 0.4 and the active 0.1.
        state.settings.keep_versions = 3;
        let candidates = state.prune_candidates(AppId::Api);
        assert_eq!(
            candidates,
            vec![Version::new(0, 3, 0), Version::new(0, 2, 0)]
        );
        let removed = state.prune(AppId::Api, &paths).expect("prune");
        assert_eq!(removed.len(), 2);
        let left: Vec<String> = state
            .app(AppId::Api)
            .installed
            .iter()
            .map(|v| v.version.to_string())
            .collect();
        assert_eq!(left, ["0.5.0", "0.4.0", "0.1.0"]);

        state.settings.keep_versions = 0;
        assert!(state.prune_candidates(AppId::Api).is_empty());
    }

    #[tokio::test]
    async fn installs_release_end_to_end_and_rejects_bad_checksum() {
        let Some(target) = release::current_target() else {
            return;
        };
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let archive = dir.path().join("src.tar.gz");
        make_archive(&archive, &[("ferrumapi", b"#!/bin/sh\n", 0o755)]);
        let bytes = std::fs::read(&archive).expect("read");
        let sha = crate::github::to_hex(&Sha256::digest(&bytes));
        let name = release::archive_name(AppId::Api, &Version::new(0, 2, 0), target);

        let server = FakeServer::start(vec![
            ("/a/good".into(), 200, vec![], bytes.clone()),
            (
                "/a/good.sha".into(),
                200,
                vec![],
                format!("{sha}  {name}\n").into_bytes(),
            ),
            (
                "/a/bad.sha".into(),
                200,
                vec![],
                format!("{}\n", "0".repeat(64)).into_bytes(),
            ),
        ])
        .await;
        let client = GitHubClient::new("o", "r", None)
            .expect("client")
            .with_api_base(&server.base);
        let asset = |path: &str, asset_name: &str| Asset {
            name: asset_name.into(),
            size: bytes.len() as u64,
            url: format!("{}{path}", server.base),
        };

        let mut bad = release(AppId::Api, "0.2.0", false);
        bad.assets = vec![
            asset("/a/good", &name),
            asset("/a/bad.sha", &format!("{name}.sha256")),
        ];
        let result = install_release(
            &client,
            &paths,
            &bad,
            Arc::new(|_, _| {}),
            CancellationToken::new(),
        )
        .await;
        assert!(matches!(result, Err(ToolboxError::ChecksumMismatch { .. })));
        assert!(
            !paths
                .version_dir(AppId::Api, &Version::new(0, 2, 0))
                .exists()
        );

        let mut good = bad.clone();
        good.assets = vec![
            asset("/a/good", &name),
            asset("/a/good.sha", &format!("{name}.sha256")),
        ];
        let record = install_release(
            &client,
            &paths,
            &good,
            Arc::new(|_, _| {}),
            CancellationToken::new(),
        )
        .await
        .expect("install");
        assert_eq!(record.asset, name);
        assert!(
            paths
                .binary_path(AppId::Api, &Version::new(0, 2, 0))
                .is_file()
        );
        let leftover_downloads = std::fs::read_dir(paths.downloads_dir())
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(leftover_downloads, 0);
    }
}
