//! Local state: settings, installed versions, the active version per app and
//! the cached release list. Persisted as `state.json` (never holds the token).

use crate::catalog::{AppId, Catalog, CatalogEntry};
use crate::github::{DownloadProgress, GitHubClient};
use crate::platform::{self, ArchiveKind, Platform};
use crate::release::{self, Channel, ReleaseInfo, UpdateChannel};
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
    /// Which OS conventions to follow (always [`Platform::current`] outside tests).
    pub platform: Platform,
    /// `state.json`, `apps/<id>/<version>/` and `downloads/`.
    pub data_dir: PathBuf,
    /// CLI entry points for the active binaries (`~/.ferrum/bin`): symlinks,
    /// or `.cmd` shims on Windows.
    pub bin_dir: PathBuf,
    /// Where the active version's `.app` is published (`~/Applications/Ferrum`
    /// on macOS); `None` on other platforms.
    pub applications_dir: Option<PathBuf>,
    /// Linux: `~/.local/share` (desktop entries go in `applications/`, icons in
    /// `icons/hicolor/256x256/apps/`).
    pub xdg_data_dir: Option<PathBuf>,
    /// Windows: the Start Menu folder that gets a shortcut per active app.
    pub start_menu_dir: Option<PathBuf>,
}

impl Paths {
    /// Platform defaults: the OS local app-data dir and `~/.ferrum/bin`.
    #[must_use]
    pub fn default_for_user() -> Option<Self> {
        let project = directories::ProjectDirs::from("dev", "Ferrum", "FerrumToolbox")?;
        let home = directories::BaseDirs::new()?;
        let platform = Platform::current();
        Some(Self {
            platform,
            data_dir: project.data_local_dir().to_path_buf(),
            bin_dir: home.home_dir().join(".ferrum").join("bin"),
            applications_dir: (platform == Platform::MacOs)
                .then(|| home.home_dir().join("Applications").join("Ferrum")),
            xdg_data_dir: (platform == Platform::Linux)
                .then(|| home.data_local_dir().to_path_buf()),
            start_menu_dir: (platform == Platform::Windows).then(|| {
                home.config_dir()
                    .join("Microsoft")
                    .join("Windows")
                    .join("Start Menu")
                    .join("Programs")
                    .join("Ferrum")
            }),
        })
    }

    /// `~/.local/share/applications/ferrum-<id>.desktop` (Linux).
    #[must_use]
    pub fn desktop_entry_path(&self, app: AppId) -> Option<PathBuf> {
        self.xdg_data_dir.as_ref().map(|dir| {
            dir.join("applications")
                .join(platform::desktop_entry_name(app))
        })
    }

    /// `~/.local/share/icons/hicolor/256x256/apps/ferrum-<id>.png` (Linux).
    #[must_use]
    pub fn icon_path(&self, app: AppId) -> Option<PathBuf> {
        self.xdg_data_dir.as_ref().map(|dir| {
            dir.join("icons")
                .join("hicolor")
                .join("256x256")
                .join("apps")
                .join(format!("{}.png", platform::icon_name(app)))
        })
    }

    /// `<Start Menu>/Programs/Ferrum/<name>.lnk` (Windows).
    #[must_use]
    pub fn shortcut_path(&self, app: AppId) -> Option<PathBuf> {
        self.start_menu_dir
            .as_ref()
            .map(|dir| dir.join(format!("{}.lnk", app.info().name)))
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

    /// The main executable of an installed version (plain or inside its `.app`).
    #[must_use]
    pub fn binary_path(&self, app: AppId, version: &Version) -> PathBuf {
        let dir = self.version_dir(app, version);
        install::locate_binary(&dir, app.info().binary)
            .unwrap_or_else(|| dir.join(app.info().binary))
    }

    /// The `.app` bundle of an installed version, if it shipped as one.
    #[must_use]
    pub fn bundle_path(&self, app: AppId, version: &Version) -> Option<PathBuf> {
        install::locate_bundle(&self.version_dir(app, version))
    }

    /// `~/Applications/Ferrum/<bundle_name>.app` for the active version.
    #[must_use]
    pub fn application_path(&self, app: AppId) -> Option<PathBuf> {
        self.applications_dir
            .as_ref()
            .map(|dir| dir.join(format!("{}.app", app.info().bundle_name)))
    }

    #[must_use]
    pub fn link_path(&self, app: AppId) -> PathBuf {
        self.bin_dir
            .join(self.platform.cli_entry(app.info().binary))
    }

    fn binaries(app: AppId) -> impl Iterator<Item = &'static str> {
        std::iter::once(app.info().binary).chain(app.info().extra_binaries.iter().copied())
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
    /// The global toggle that per-app update channels replaced; only read so
    /// old state files can be migrated.
    #[serde(rename = "include_prereleases", skip_serializing)]
    pub legacy_include_prereleases: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            repo_owner: "Amygdal-doo".into(),
            repo_name: "ferrum".into(),
            check_interval_hours: 6,
            keep_versions: 3,
            legacy_include_prereleases: false,
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
    /// Which releases updates follow.
    pub channel: UpdateChannel,
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
    /// Last `catalog.json` fetched from the repo (completed from the embedded copy).
    pub catalog: Option<Catalog>,
}

impl State {
    /// Loads `state.json`; a missing file yields defaults.
    pub fn load(paths: &Paths) -> Result<Self> {
        let mut state: Self = match std::fs::read(paths.state_file()) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e.into()),
        };
        state.migrate();
        Ok(state)
    }

    /// "Include prereleases" became the Beta channel for every app.
    fn migrate(&mut self) {
        if std::mem::take(&mut self.settings.legacy_include_prereleases) {
            for app in AppId::ALL {
                self.apps.entry(app).or_default().channel = UpdateChannel::Beta;
            }
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

    /// Every release of `app` on every channel, newest version first.
    #[must_use]
    pub fn releases_for(&self, app: AppId) -> Vec<&ReleaseInfo> {
        let mut releases: Vec<_> = self
            .releases
            .iter()
            .filter(|release| release.app == app)
            .collect();
        releases.sort_by(|a, b| b.version.cmp(&a.version));
        releases
    }

    /// Every release of every app, most recently published first.
    #[must_use]
    pub fn timeline(&self) -> Vec<&ReleaseInfo> {
        let mut releases: Vec<_> = self.releases.iter().collect();
        releases.sort_by(|a, b| {
            b.published_at
                .cmp(&a.published_at)
                .then_with(|| b.version.cmp(&a.version))
        });
        releases
    }

    #[must_use]
    pub fn channel(&self, app: AppId) -> UpdateChannel {
        self.apps
            .get(&app)
            .map(|state| state.channel)
            .unwrap_or_default()
    }

    pub fn set_channel(&mut self, app: AppId, channel: UpdateChannel) {
        self.apps.entry(app).or_default().channel = channel;
    }

    /// Newest release the app's update channel follows.
    #[must_use]
    pub fn latest(&self, app: AppId) -> Option<&ReleaseInfo> {
        release::latest(&self.releases, app, self.channel(app))
    }

    /// Newest release published exactly on `channel`.
    #[must_use]
    pub fn latest_in(&self, app: AppId, channel: Channel) -> Option<&ReleaseInfo> {
        release::latest_in(&self.releases, app, channel)
    }

    #[must_use]
    pub fn last_released(&self, app: AppId) -> Option<DateTime<Utc>> {
        self.releases
            .iter()
            .filter(|release| release.app == app)
            .filter_map(|release| release.published_at)
            .max()
    }

    #[must_use]
    pub fn total_downloads(&self, app: AppId) -> u64 {
        self.releases
            .iter()
            .filter(|release| release.app == app)
            .map(ReleaseInfo::downloads)
            .sum()
    }

    /// The app's catalog entry: fetched copy first, then the embedded one.
    #[must_use]
    pub fn catalog_entry(&self, app: AppId) -> Option<&CatalogEntry> {
        self.catalog
            .as_ref()
            .and_then(|catalog| catalog.entry(app))
            .or_else(|| Catalog::embedded().entry(app))
    }

    pub fn record_catalog(&mut self, catalog: Catalog) {
        self.catalog = Some(catalog.or_fill_from(Catalog::embedded()));
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

/// Points the CLI entries at `version` and integrates it with the desktop:
/// the `.app` on macOS, a `.desktop` entry on Linux, a Start Menu shortcut
/// on Windows.
fn link_active(paths: &Paths, app: AppId, version: &Version) -> Result<()> {
    let dir = paths.version_dir(app, version);
    for binary in Paths::binaries(app) {
        let entry = paths.bin_dir.join(paths.platform.cli_entry(binary));
        match install::locate_binary(&dir, binary) {
            Some(target) if paths.platform == Platform::Windows => {
                write_atomically(&entry, platform::cmd_shim(&target).as_bytes())?
            }
            Some(target) => symlink_atomically(&target, &entry)?,
            None => remove_if_exists(&entry)?,
        }
    }
    if let Some(application) = paths.application_path(app) {
        match paths.bundle_path(app, version) {
            Some(bundle) => publish_bundle(&bundle, &application)?,
            None => remove_if_exists(&application)?,
        }
    }
    let executable = install::locate_binary(&dir, app.info().binary);
    if let (Some(entry), Some(icon)) = (paths.desktop_entry_path(app), paths.icon_path(app)) {
        match &executable {
            Some(executable) => {
                write_atomically(&icon, platform::icon_png(app))?;
                write_atomically(&entry, platform::desktop_entry(app, executable).as_bytes())?;
            }
            None => {
                remove_if_exists(&entry)?;
                remove_if_exists(&icon)?;
            }
        }
    }
    if let Some(shortcut) = paths.shortcut_path(app) {
        match &executable {
            Some(executable) => {
                if let Err(e) = create_shortcut(&shortcut, executable, &dir, app) {
                    tracing::warn!(app = %app, error = %e, "could not create Start Menu shortcut");
                }
            }
            None => remove_if_exists(&shortcut)?,
        }
    }
    Ok(())
}

fn write_atomically(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    std::fs::write(&temp, contents)?;
    if let Err(e) = std::fs::rename(&temp, path) {
        std::fs::remove_file(&temp).ok();
        return Err(e.into());
    }
    Ok(())
}

/// Creates a `.lnk` through the Windows Script Host. Paths travel in
/// environment variables so no quoting can break the PowerShell command.
#[cfg(windows)]
fn create_shortcut(
    shortcut: &Path,
    executable: &Path,
    working_dir: &Path,
    app: AppId,
) -> Result<()> {
    if let Some(parent) = shortcut.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let icon = working_dir.join(format!("{app}.ico"));
    let icon = if icon.is_file() {
        icon
    } else {
        executable.to_path_buf()
    };
    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$s = (New-Object -ComObject WScript.Shell).CreateShortcut($env:FERRUM_LNK); \
             $s.TargetPath = $env:FERRUM_TARGET; $s.WorkingDirectory = $env:FERRUM_DIR; \
             $s.IconLocation = $env:FERRUM_ICON; $s.Description = $env:FERRUM_DESC; $s.Save()",
        ])
        .env("FERRUM_LNK", shortcut)
        .env("FERRUM_TARGET", executable)
        .env("FERRUM_DIR", working_dir)
        .env("FERRUM_ICON", icon)
        .env("FERRUM_DESC", app.info().description)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(ToolboxError::Invalid(format!(
            "powershell exited with {status}"
        )))
    }
}

#[cfg(not(windows))]
fn create_shortcut(_: &Path, _: &Path, _: &Path, _: AppId) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn symlink_atomically(target: &Path, link: &Path) -> Result<()> {
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = link.with_extension(format!("link-{}", uuid::Uuid::new_v4()));
    std::os::unix::fs::symlink(target, &temp)?;
    if let Err(e) = std::fs::rename(&temp, link) {
        std::fs::remove_file(&temp).ok();
        return Err(e.into());
    }
    Ok(())
}

#[cfg(not(unix))]
fn symlink_atomically(_: &Path, _: &Path) -> Result<()> {
    Ok(())
}

/// Copies the bundle into place. Launchpad and Spotlight skip symlinked apps,
/// so this is a real copy; `cp -c` makes it an APFS clone (no extra disk).
fn publish_bundle(bundle: &Path, application: &Path) -> Result<()> {
    let parent = application
        .parent()
        .ok_or_else(|| ToolboxError::Invalid("applications path has no parent".into()))?;
    std::fs::create_dir_all(parent)?;
    let staging = parent.join(format!(".ferrum-{}.app", uuid::Uuid::new_v4()));
    let copied = ["-cR", "-R"].into_iter().any(|flags| {
        std::process::Command::new("cp")
            .arg(flags)
            .arg(bundle)
            .arg(&staging)
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
            || {
                std::fs::remove_dir_all(&staging).ok();
                false
            }
    });
    if !copied {
        return Err(ToolboxError::Invalid(format!(
            "could not copy {} to {}",
            bundle.display(),
            parent.display()
        )));
    }
    let result = if application.exists() || application.is_symlink() {
        let old = parent.join(format!(".ferrum-old-{}.app", uuid::Uuid::new_v4()));
        std::fs::rename(application, &old)
            .and_then(|()| std::fs::rename(&staging, application))
            .and_then(|()| remove_path(&old))
    } else {
        std::fs::rename(&staging, application)
    };
    if result.is_err() {
        std::fs::remove_dir_all(&staging).ok();
    }
    Ok(result?)
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    if path.is_dir() && !path.is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match remove_path(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn unlink(paths: &Paths, app: AppId) -> Result<()> {
    for binary in Paths::binaries(app) {
        remove_if_exists(&paths.bin_dir.join(paths.platform.cli_entry(binary)))?;
    }
    let integrations = [
        paths.application_path(app),
        paths.desktop_entry_path(app),
        paths.icon_path(app),
        paths.shortcut_path(app),
    ];
    for path in integrations.into_iter().flatten() {
        remove_if_exists(&path)?;
    }
    Ok(())
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
    install_release_for_target(client, paths, release, target, progress, cancel).await
}

/// [`install_release`] for an explicit target triple.
pub async fn install_release_for_target(
    client: &GitHubClient,
    paths: &Paths,
    release: &ReleaseInfo,
    target: &str,
    progress: DownloadProgress,
    cancel: CancellationToken,
) -> Result<InstalledVersion> {
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

    let kind = ArchiveKind::from_asset_name(&archive.name)
        .ok_or_else(|| ToolboxError::Invalid(format!("unsupported archive {}", archive.name)))?;
    let destination = paths.version_dir(release.app, &release.version);
    let binary = release.app.info().binary;
    let archive_path = download.clone();
    let extracted = tokio::task::spawn_blocking(move || {
        install::install_archive(&archive_path, kind, &destination, binary)
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

/// Starts the active version detached from the toolbox. On macOS a bundled
/// version opens through Launch Services (`open`), like from Finder.
pub fn launch(state: &State, app: AppId, paths: &Paths) -> Result<()> {
    let version = state
        .active(app)
        .ok_or_else(|| ToolboxError::Invalid(format!("{} is not installed", app.info().name)))?;
    if paths.platform == Platform::MacOs {
        let bundle = paths
            .application_path(app)
            .filter(|path| path.is_dir())
            .or_else(|| paths.bundle_path(app, version));
        if let Some(bundle) = bundle {
            let status = std::process::Command::new("open")
                .arg(&bundle)
                .stdin(std::process::Stdio::null())
                .status()?;
            return if status.success() {
                Ok(())
            } else {
                Err(ToolboxError::Invalid(format!(
                    "`open {}` failed ({status})",
                    bundle.display()
                )))
            };
        }
    }
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
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
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
            platform: Platform::MacOs,
            data_dir: dir.join("data"),
            bin_dir: dir.join("bin"),
            applications_dir: Some(dir.join("Applications/Ferrum")),
            xdg_data_dir: None,
            start_menu_dir: None,
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
        state.set_channel(AppId::Api, UpdateChannel::Beta);
        assert_eq!(
            state.updates(),
            vec![(AppId::Api, Version::parse("0.4.0-beta.1").expect("semver"))]
        );
        assert_eq!(state.releases_for(AppId::Api).len(), 2);
        assert_eq!(
            state
                .latest_in(AppId::Api, Channel::Stable)
                .map(|r| r.version.clone()),
            Some(Version::new(0, 3, 0))
        );
    }

    #[test]
    fn migrates_include_prereleases_to_beta_channels() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        std::fs::create_dir_all(&paths.data_dir).expect("mkdir");
        std::fs::write(
            paths.state_file(),
            r#"{"settings":{"repo_owner":"o","repo_name":"r","include_prereleases":true},
                "apps":{"data":{"installed":[],"active":null}}}"#,
        )
        .expect("write");
        let state = State::load(&paths).expect("load");
        assert!(
            AppId::ALL
                .iter()
                .all(|app| state.channel(*app) == UpdateChannel::Beta)
        );
        assert!(!state.settings.legacy_include_prereleases);
        state.save(&paths).expect("save");
        let json = std::fs::read_to_string(paths.state_file()).expect("read");
        assert!(!json.contains("include_prereleases"));
        assert!(json.contains(r#""channel": "beta""#));

        std::fs::write(paths.state_file(), r#"{"settings":{"repo_owner":"o"}}"#).expect("write");
        let fresh = State::load(&paths).expect("load");
        assert_eq!(fresh.channel(AppId::Data), UpdateChannel::Stable);
    }

    #[test]
    fn summarizes_releases_and_catalog() {
        let mut state = State::default();
        let mut older = release(AppId::Data, "0.1.0", false);
        older.published_at = DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z")
            .ok()
            .map(|t| t.with_timezone(&Utc));
        older.assets = vec![Asset {
            name: "FerrumData-0.1.0-aarch64.dmg".into(),
            size: 1,
            url: String::new(),
            download_count: 3,
        }];
        let mut newer = release(AppId::Data, "0.2.0-beta.1", true);
        newer.published_at = DateTime::parse_from_rfc3339("2026-11-05T00:00:00Z")
            .ok()
            .map(|t| t.with_timezone(&Utc));
        newer.assets = vec![Asset {
            name: "data.tar.gz".into(),
            size: 1,
            url: String::new(),
            download_count: 2,
        }];
        let api = release(AppId::Api, "1.0.0", false);
        state.record_releases(vec![older, api, newer]);
        assert_eq!(state.total_downloads(AppId::Data), 5);
        assert_eq!(
            state.last_released(AppId::Data),
            state.releases[2].published_at
        );
        let timeline: Vec<_> = state.timeline().iter().map(|r| r.tag.clone()).collect();
        assert_eq!(timeline[0], "data-v0.2.0-beta.1");
        assert_eq!(timeline[1], "data-v0.1.0");

        assert!(state.catalog_entry(AppId::Term).is_some());
        state.record_catalog(
            Catalog::parse(r#"{"apps":[{"id":"term","status":"stable","summary":"Done"}]}"#)
                .expect("parse"),
        );
        assert_eq!(
            state.catalog_entry(AppId::Term).map(|e| e.status),
            Some(crate::AppStatus::Stable)
        );
        assert!(state.catalog_entry(AppId::Ide).is_some());
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
    fn windows_activation_writes_cmd_shims_and_uninstall_removes_them() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = Paths {
            platform: Platform::Windows,
            applications_dir: None,
            start_menu_dir: None,
            ..paths(dir.path())
        };
        let mut state = State::default();
        let version = Version::new(0, 3, 0);
        let version_dir = paths.version_dir(AppId::Term, &version);
        std::fs::create_dir_all(&version_dir).expect("mkdir");
        std::fs::write(version_dir.join("ferrumterm.exe"), b"MZ").expect("write");
        std::fs::write(version_dir.join("fterm.exe"), b"MZ").expect("write");
        state.record_install(AppId::Term, installed("0.3.0"));

        state
            .activate(AppId::Term, &version, &paths)
            .expect("activate");
        let shim = std::fs::read_to_string(paths.bin_dir.join("fterm.cmd")).expect("shim");
        assert_eq!(shim, platform::cmd_shim(&version_dir.join("fterm.exe")));
        assert_eq!(
            paths.link_path(AppId::Term),
            paths.bin_dir.join("ferrumterm.cmd")
        );
        assert!(paths.link_path(AppId::Term).is_file());

        state
            .uninstall(AppId::Term, &version, &paths)
            .expect("uninstall");
        assert!(!paths.bin_dir.join("fterm.cmd").exists());
        assert!(!paths.link_path(AppId::Term).exists());
    }

    #[test]
    fn linux_activation_writes_desktop_entry_and_icon() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = Paths {
            platform: Platform::Linux,
            applications_dir: None,
            xdg_data_dir: Some(dir.path().join("share")),
            ..paths(dir.path())
        };
        let mut state = State::default();
        fake_install(&mut state, &paths, AppId::Data, "1.0.0");
        let version = Version::new(1, 0, 0);
        state
            .activate(AppId::Data, &version, &paths)
            .expect("activate");

        let entry_path = dir.path().join("share/applications/ferrum-data.desktop");
        let entry = std::fs::read_to_string(&entry_path).expect("desktop entry");
        assert!(entry.contains(&format!(
            "Exec=\"{}\"",
            paths.binary_path(AppId::Data, &version).display()
        )));
        let icon = dir
            .path()
            .join("share/icons/hicolor/256x256/apps/ferrum-data.png");
        assert!(std::fs::read(&icon).expect("icon").starts_with(b"\x89PNG"));

        state
            .uninstall(AppId::Data, &version, &paths)
            .expect("uninstall");
        assert!(!entry_path.exists());
        assert!(!icon.exists());
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

    fn fake_bundle_install(state: &mut State, paths: &Paths, app: AppId, version: &str) {
        let v = Version::parse(version).expect("semver");
        let macos = paths
            .version_dir(app, &v)
            .join(format!("{}.app", app.info().bundle_name))
            .join("Contents/MacOS");
        std::fs::create_dir_all(&macos).expect("mkdir");
        for binary in Paths::binaries(app) {
            std::fs::write(macos.join(binary), version.as_bytes()).expect("write");
        }
        state.record_install(app, installed(version));
    }

    #[test]
    fn activating_a_bundle_publishes_the_app_and_links_every_binary() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let mut state = State::default();
        fake_bundle_install(&mut state, &paths, AppId::Term, "0.1.0");
        fake_bundle_install(&mut state, &paths, AppId::Term, "0.2.0");
        let v1 = Version::new(0, 1, 0);
        let v2 = Version::new(0, 2, 0);
        let application = paths.application_path(AppId::Term).expect("app path");
        assert!(application.ends_with("Applications/Ferrum/FerrumTerm.app"));

        state.activate(AppId::Term, &v2, &paths).expect("activate");
        let published = application.join("Contents/MacOS/ferrumterm");
        assert_eq!(std::fs::read(&published).expect("copy"), b"0.2.0");
        assert!(!application.is_symlink(), "Launchpad needs a real bundle");
        #[cfg(unix)]
        {
            let bundle = paths.bundle_path(AppId::Term, &v2).expect("bundle");
            assert_eq!(
                std::fs::read_link(paths.bin_dir.join("ferrumterm")).expect("link"),
                bundle.join("Contents/MacOS/ferrumterm")
            );
            assert_eq!(
                std::fs::read_link(paths.bin_dir.join("fterm")).expect("extra link"),
                bundle.join("Contents/MacOS/fterm")
            );
        }

        // Rollback swaps the published copy.
        state.activate(AppId::Term, &v1, &paths).expect("rollback");
        assert_eq!(std::fs::read(&published).expect("copy"), b"0.1.0");
        let leftovers: Vec<_> = std::fs::read_dir(application.parent().expect("parent"))
            .expect("read")
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(leftovers.len(), 1, "no staging copies left behind");

        state
            .uninstall(AppId::Term, &v2, &paths)
            .expect("remove inactive");
        assert!(application.is_dir());
        state
            .uninstall(AppId::Term, &v1, &paths)
            .expect("remove last");
        assert!(!application.exists());
        assert!(!paths.bin_dir.join("fterm").exists());
        assert!(!paths.bin_dir.join("ferrumterm").exists());
    }

    #[test]
    fn activating_a_plain_version_removes_a_stale_app_copy() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let mut state = State::default();
        fake_bundle_install(&mut state, &paths, AppId::Data, "0.1.0");
        fake_install(&mut state, &paths, AppId::Data, "0.2.0");
        state
            .activate(AppId::Data, &Version::new(0, 1, 0), &paths)
            .expect("bundle");
        let application = paths.application_path(AppId::Data).expect("app path");
        assert!(application.is_dir());
        state
            .activate(AppId::Data, &Version::new(0, 2, 0), &paths)
            .expect("plain");
        assert!(!application.exists());
        assert_eq!(
            paths.binary_path(AppId::Data, &Version::new(0, 2, 0)),
            paths
                .version_dir(AppId::Data, &Version::new(0, 2, 0))
                .join("ferrumdata")
        );
    }

    #[tokio::test]
    async fn installs_app_bundle_release_end_to_end() {
        let target = "aarch64-apple-darwin";
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let archive = dir.path().join("bundle.tar.gz");
        make_archive(
            &archive,
            &[
                ("FerrumData.app/Contents/Info.plist", b"<plist/>", 0o644),
                (
                    "FerrumData.app/Contents/MacOS/ferrumdata",
                    b"#!/bin/sh\n",
                    0o755,
                ),
            ],
        );
        let bytes = std::fs::read(&archive).expect("read");
        let sha = crate::github::to_hex(&Sha256::digest(&bytes));
        let version = Version::new(0, 3, 0);
        let name = release::bundle_archive_name(AppId::Data, &version, target);
        let server = FakeServer::start(vec![
            ("/a/bundle".into(), 200, vec![], bytes.clone()),
            (
                "/a/bundle.sha".into(),
                200,
                vec![],
                format!("{sha}  {name}\n").into_bytes(),
            ),
        ])
        .await;
        let client = GitHubClient::new("o", "r", None)
            .expect("client")
            .with_api_base(&server.base);
        let mut data = release(AppId::Data, "0.3.0", false);
        data.assets = vec![
            Asset {
                name: name.clone(),
                size: bytes.len() as u64,
                url: format!("{}/a/bundle", server.base),
                download_count: 0,
            },
            Asset {
                name: format!("{name}.sha256"),
                size: 80,
                url: format!("{}/a/bundle.sha", server.base),
                download_count: 0,
            },
        ];
        let record = install_release_for_target(
            &client,
            &paths,
            &data,
            target,
            Arc::new(|_, _| {}),
            CancellationToken::new(),
        )
        .await
        .expect("install");
        assert_eq!(record.asset, name);
        let mut state = State::default();
        state.record_install(AppId::Data, record);
        state
            .activate(AppId::Data, &version, &paths)
            .expect("activate");
        let application = paths.application_path(AppId::Data).expect("app path");
        assert!(application.join("Contents/MacOS/ferrumdata").is_file());
        #[cfg(unix)]
        assert!(
            std::fs::canonicalize(paths.link_path(AppId::Data))
                .expect("link resolves")
                .ends_with("FerrumData.app/Contents/MacOS/ferrumdata")
        );
    }

    /// Opt-in check against a real bundle archive and the real per-user paths:
    /// `FERRUM_E2E_APP_ARCHIVE=<data .app.tar.gz> HOME=<temp> cargo test -p
    /// ferrumtoolbox-core real_bundle -- --ignored`.
    #[tokio::test]
    #[ignore = "needs FERRUM_E2E_APP_ARCHIVE and a throwaway HOME"]
    async fn real_bundle_installs_into_user_paths() {
        let archive = std::env::var("FERRUM_E2E_APP_ARCHIVE").expect("FERRUM_E2E_APP_ARCHIVE");
        let paths = Paths::default_for_user().expect("paths");
        let bytes = std::fs::read(&archive).expect("archive");
        let sha = crate::github::to_hex(&Sha256::digest(&bytes));
        let target = release::current_target().expect("target");
        let version = Version::new(0, 0, 1);
        let name = release::bundle_archive_name(AppId::Data, &version, target);
        let server = FakeServer::start(vec![
            ("/e2e/archive".into(), 200, vec![], bytes.clone()),
            (
                "/e2e/sha".into(),
                200,
                vec![],
                format!("{sha}  {name}\n").into_bytes(),
            ),
        ])
        .await;
        let client = GitHubClient::new("o", "r", None)
            .expect("client")
            .with_api_base(&server.base);
        let mut data = release(AppId::Data, "0.0.1", false);
        data.assets = vec![
            Asset {
                name: name.clone(),
                size: bytes.len() as u64,
                url: format!("{}/e2e/archive", server.base),
                download_count: 0,
            },
            Asset {
                name: format!("{name}.sha256"),
                size: 80,
                url: format!("{}/e2e/sha", server.base),
                download_count: 0,
            },
        ];
        let record = install_release(
            &client,
            &paths,
            &data,
            Arc::new(|_, _| {}),
            CancellationToken::new(),
        )
        .await
        .expect("install");
        let mut state = State::load(&paths).expect("load");
        state.record_install(AppId::Data, record);
        state
            .activate(AppId::Data, &version, &paths)
            .expect("activate");
        state.save(&paths).expect("save");
        let application = paths.application_path(AppId::Data).expect("macOS");
        println!("application: {}", application.display());
        println!(
            "link: {} -> {}",
            paths.link_path(AppId::Data).display(),
            std::fs::canonicalize(paths.link_path(AppId::Data))
                .expect("resolves")
                .display()
        );
        assert!(application.join("Contents/MacOS/ferrumdata").is_file());
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
            download_count: 0,
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
