//! Local state: settings, installed versions, the active version per app and
//! the cached release list. Persisted as `state.json` (never holds the token).

use crate::catalog::{AppId, AppInfo, Catalog, CatalogEntry};
use crate::github::{DownloadProgress, GitHubClient};
use crate::platform::{self, ArchiveKind, Platform};
use crate::release::{self, Channel, ReleaseInfo, UpdateChannel};
use crate::{LauncherError, Result, install};
use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

/// Where the launcher keeps its files.
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
        let project = directories::ProjectDirs::from("dev", "Ferrum", "FerrumLauncher")?;
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
    pub fn shortcut_path(&self, info: &AppInfo) -> Option<PathBuf> {
        self.start_menu_dir
            .as_ref()
            .map(|dir| dir.join(format!("{}.lnk", info.name)))
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
    pub fn binary_path(&self, info: &AppInfo, version: &Version) -> PathBuf {
        let dir = self.version_dir(info.id, version);
        install::locate_binary(&dir, &info.binary).unwrap_or_else(|| dir.join(&info.binary))
    }

    /// The `.app` bundle of an installed version, if it shipped as one.
    #[must_use]
    pub fn bundle_path(&self, app: AppId, version: &Version) -> Option<PathBuf> {
        install::locate_bundle(&self.version_dir(app, version))
    }

    /// `~/Applications/Ferrum/<bundle_name>.app` for the active version.
    #[must_use]
    pub fn application_path(&self, info: &AppInfo) -> Option<PathBuf> {
        self.applications_dir
            .as_ref()
            .map(|dir| dir.join(format!("{}.app", info.bundle_name)))
    }

    /// The `.app` copies and Start Menu shortcuts named after the app's
    /// [former names](AppInfo::former_names), whether they exist or not.
    #[must_use]
    pub fn former_integrations(&self, info: &AppInfo) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for former in &info.former_names {
            if let Some(dir) = &self.applications_dir
                && *former != info.bundle_name
            {
                found.push(dir.join(format!("{former}.app")));
            }
            if let Some(dir) = &self.start_menu_dir
                && *former != info.name
            {
                found.push(dir.join(format!("{former}.lnk")));
            }
        }
        found
    }

    #[must_use]
    pub fn link_path(&self, info: &AppInfo) -> PathBuf {
        self.bin_dir.join(self.platform.cli_entry(&info.binary))
    }

    /// Where the app's catalog logo is cached. The file name carries a hash
    /// of the URL, so a changed URL is downloaded again.
    #[must_use]
    pub fn logo_path(&self, info: &AppInfo) -> Option<PathBuf> {
        use sha2::{Digest, Sha256};
        let url = info.logo.as_ref()?;
        let hash = crate::github::to_hex(&Sha256::digest(url.as_bytes()));
        Some(
            self.data_dir
                .join("logos")
                .join(format!("{}-{}.png", info.id, &hash[..12])),
        )
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
    /// Bumped by one-time moves of every app's update channel, so a later
    /// choice on an app's page isn't overridden again. Missing in older state
    /// files, which therefore read as 0 rather than the current revision.
    #[serde(default)]
    pub channels_revision: u32,
    /// Bumped by one-time moves of the suggested shell commands (see
    /// [`crate::shell::rename_defaults`]). Missing in older state files,
    /// which read as 0 and get every move.
    #[serde(default)]
    pub commands_revision: u32,
    /// Apps the user has been told about. `None` until the first start,
    /// which records every app known then without announcing any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen_apps: Option<BTreeSet<AppId>>,
    /// Apps announced as new; their overview row shows a "New" badge until
    /// the user opens or installs them.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub new_apps: BTreeSet<AppId>,
    /// Whether the keychain was checked for the GitHub sign-in Ferrum
    /// Toolbox saved (see [`crate::auth::migrate_token`]); checked once so a
    /// denied keychain prompt isn't repeated on every start.
    #[serde(default)]
    pub legacy_keychain_checked: bool,
    /// Per-app choices: shell command, Dock icon, global shortcut.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub app_prefs: BTreeMap<AppId, AppPrefs>,
    /// Show the launcher in the macOS menu bar (and keep it running there
    /// when its window is closed, so shortcuts keep working).
    #[serde(default)]
    pub menu_bar: bool,
}

/// What the user chose for one app on its page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppPrefs {
    /// The shell command that opens it (`fee`); `None` when turned off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Run without a Dock icon (the app reads this when it starts).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hide_from_dock: bool,
    /// Global shortcut that opens it, e.g. `cmd+alt+e`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
}

/// The public repo customers download from: releases only, no source, so
/// the launcher works without a GitHub sign-in.
pub const PUBLIC_REPO: (&str, &str) = ("Amygdal-doo", "ferrum-releases");

/// Where releases were published before [`PUBLIC_REPO`] (private).
const LEGACY_REPO: (&str, &str) = ("Amygdal-doo", "ferrum");

/// 1: every app moved to the Alpha channel, where all Ferrum releases are.
const CHANNELS_REVISION: u32 = 1;

/// 1: FerrumIDE `fee` → `fei`, FerrumEngineer `fev` → `fen`, FerrumVariables
/// `fen` → `fev` (after FerrumAgentic's `fex` → `fev`, FerrumEnv's `fev` → `fen`).
const COMMANDS_REVISION: u32 = 1;

impl Settings {
    /// Whether the configured repo is [`PUBLIC_REPO`], which needs no token.
    #[must_use]
    pub fn uses_public_repo(&self) -> bool {
        (self.repo_owner.as_str(), self.repo_name.as_str()) == PUBLIC_REPO
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            repo_owner: PUBLIC_REPO.0.into(),
            repo_name: PUBLIC_REPO.1.into(),
            check_interval_hours: 6,
            keep_versions: 3,
            legacy_include_prereleases: false,
            channels_revision: CHANNELS_REVISION,
            commands_revision: COMMANDS_REVISION,
            seen_apps: None,
            new_apps: BTreeSet::new(),
            legacy_keychain_checked: false,
            app_prefs: BTreeMap::new(),
            menu_bar: false,
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

/// `State::format` of files the launcher writes. Ferrum Toolbox's files
/// have no `format` and read as 0.
const STATE_FORMAT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// 0 for a file Ferrum Toolbox wrote, [`STATE_FORMAT`] for the launcher's.
    #[serde(default)]
    pub format: u32,
    pub settings: Settings,
    pub apps: BTreeMap<AppId, AppState>,
    pub last_check: Option<DateTime<Utc>>,
    pub releases: Vec<ReleaseInfo>,
    /// Last `catalog.json` fetched from the repo (completed from the embedded copy).
    pub catalog: Option<Catalog>,
    /// Version of the launcher binary that is running. Not persisted: a
    /// launcher opened from a DMG install counts as installed even though
    /// this store never installed it.
    #[serde(skip)]
    pub running_launcher: Option<Version>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            format: STATE_FORMAT,
            settings: Settings::default(),
            apps: BTreeMap::new(),
            last_check: None,
            releases: Vec::new(),
            catalog: None,
            running_launcher: None,
        }
    }
}

impl State {
    /// Loads `state.json`; a missing file yields defaults.
    pub fn load(paths: &Paths) -> Result<Self> {
        let mut state: Self = match std::fs::read(paths.state_file()) {
            Ok(bytes) => {
                let mut state: Self = serde_json::from_slice(&bytes)?;
                // Written by Ferrum Toolbox 0.5.0 or older, which listed
                // fewer apps: the ones added since get announced.
                if state.format == 0 && state.settings.seen_apps.is_none() {
                    state.settings.seen_apps = Some(AppId::PRE_LAUNCHER.into_iter().collect());
                }
                state.format = STATE_FORMAT;
                state
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e.into()),
        };
        state.migrate();
        Ok(state)
    }

    /// "Include prereleases" became the Beta channel for every app, then every
    /// app moved to Alpha; releases moved from the private source repo to
    /// [`PUBLIC_REPO`].
    fn migrate(&mut self) {
        if (
            self.settings.repo_owner.as_str(),
            self.settings.repo_name.as_str(),
        ) == LEGACY_REPO
        {
            self.settings.repo_name = PUBLIC_REPO.1.into();
        }
        if std::mem::take(&mut self.settings.legacy_include_prereleases) {
            for app in AppId::BUILTIN {
                self.apps.entry(app).or_default().channel = UpdateChannel::Beta;
            }
        }
        // FerrumAgentic became FerrumVibe (FeV), whose command was `fev`, so
        // FerrumEnv's `fev` became `fen`. The old install itself is
        // removed by `retire_agentic`.
        if let Some(old) = self.settings.app_prefs.remove(&AppId::LEGACY_VIBE) {
            let vibe = self.settings.app_prefs.entry(AppId::VIBE).or_default();
            vibe.hide_from_dock |= old.hide_from_dock;
            vibe.hotkey = vibe.hotkey.take().or(old.hotkey);
            if old.command.is_some() {
                vibe.command = Some("fev".into());
            }
        }
        if self.settings.commands_revision < COMMANDS_REVISION {
            if let Some(env) = self.settings.app_prefs.get_mut(&AppId::ENV)
                && env.command.as_deref() == Some("fev")
            {
                env.command = Some("fen".into());
            }
            // Then FerrumIDE (FeI), FerrumEngineer (FeN) and FerrumVariables
            // (FeV): `fee` → `fei`, and `fen` / `fev` swap. The scripts
            // follow in `sync_shell_commands`.
            for (app, old, new) in crate::shell::rename_defaults(&mut self.settings.app_prefs) {
                tracing::info!(%app, old, new, "moving a shell command to its new name");
            }
            self.settings.commands_revision = COMMANDS_REVISION;
        }
        if let Some(seen) = &mut self.settings.seen_apps
            && seen.remove(&AppId::LEGACY_VIBE)
        {
            seen.insert(AppId::VIBE);
        }
        if let Some(old) = self.apps.get(&AppId::LEGACY_VIBE) {
            let channel = old.channel;
            self.apps.entry(AppId::VIBE).or_default().channel = channel;
        }
        if self.settings.channels_revision < CHANNELS_REVISION {
            for app in AppId::BUILTIN {
                self.apps.entry(app).or_default().channel = UpdateChannel::Alpha;
            }
            self.settings.channels_revision = CHANNELS_REVISION;
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
        let recorded = self.apps.get(&app).and_then(|state| state.active.as_ref());
        if app == AppId::LAUNCHER {
            return recorded.max(self.running_launcher.as_ref());
        }
        recorded
    }

    /// The catalog in use: the fetched copy, else the embedded one.
    #[must_use]
    pub fn catalog(&self) -> &Catalog {
        self.catalog.as_ref().unwrap_or_else(|| Catalog::embedded())
    }

    /// Metadata for `app`: built-in values overridden by the catalog, the
    /// install layout of a retired app, or generic names for an id nothing
    /// describes.
    #[must_use]
    pub fn info(&self, app: AppId) -> AppInfo {
        self.catalog()
            .info(app)
            .or_else(|| Catalog::embedded().info(app))
            .or_else(|| AppInfo::retired(app))
            .unwrap_or_else(|| AppInfo::generic(app))
    }

    /// Whether the suite still offers `app`: the launcher, or an app the
    /// catalog or the built-in table describes. Retired apps (Ferrum IDE)
    /// and apps whose catalog entry was removed are not offered.
    #[must_use]
    pub fn is_offered(&self, app: AppId) -> bool {
        app == AppId::LAUNCHER || self.managed_apps().iter().any(|info| info.id == app)
    }

    /// Apps installed earlier that are no longer offered, in overview order.
    /// The overview lists them as "No longer offered": they get no updates,
    /// but Launch and Uninstall keep working.
    #[must_use]
    pub fn discontinued_apps(&self) -> Vec<AppInfo> {
        let offered: BTreeSet<AppId> = self.managed_apps().iter().map(|info| info.id).collect();
        let mut apps: Vec<AppInfo> = self
            .apps
            .iter()
            .filter(|(app, state)| {
                **app != AppId::LAUNCHER && !offered.contains(*app) && !state.installed.is_empty()
            })
            .map(|(app, _)| self.info(*app))
            .collect();
        apps.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.name.cmp(&b.name)));
        apps
    }

    /// The apps the overview lists, in order: every app the catalog
    /// describes except the launcher itself (it updates from the toolbar).
    #[must_use]
    pub fn managed_apps(&self) -> Vec<AppInfo> {
        self.catalog()
            .apps()
            .into_iter()
            .filter(|info| info.id != AppId::LAUNCHER)
            .collect()
    }

    /// Records apps that appeared since the last call (new catalog entries,
    /// or new built-ins after a launcher update) and returns them so the UI
    /// can announce them once. The first call on a fresh install records
    /// every app without announcing any.
    pub fn discover_apps(&mut self) -> Vec<AppId> {
        let available: Vec<AppId> = self.managed_apps().iter().map(|info| info.id).collect();
        let Some(seen) = &mut self.settings.seen_apps else {
            self.settings.seen_apps = Some(available.into_iter().collect());
            return Vec::new();
        };
        let fresh: Vec<AppId> = available
            .into_iter()
            .filter(|app| !seen.contains(app))
            .collect();
        seen.extend(fresh.iter().copied());
        self.settings.new_apps.extend(fresh.iter().copied());
        fresh
    }

    /// Whether `app`'s overview row shows the "New" badge.
    #[must_use]
    pub fn is_new(&self, app: AppId) -> bool {
        self.settings.new_apps.contains(&app)
    }

    /// Clears the "New" badge; `true` if it was shown.
    pub fn mark_seen(&mut self, app: AppId) -> bool {
        self.settings.new_apps.remove(&app)
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
        self.catalog()
            .entry(app)
            .or_else(|| Catalog::embedded().entry(app))
    }

    pub fn record_catalog(&mut self, catalog: Catalog) {
        self.catalog = Some(catalog.or_fill_from(Catalog::embedded()));
    }

    /// The newer release if `app` is installed, still offered and behind
    /// the latest.
    #[must_use]
    pub fn update_for(&self, app: AppId) -> Option<&ReleaseInfo> {
        let active = self.active(app)?;
        if !self.is_offered(app) {
            return None;
        }
        self.latest(app).filter(|latest| &latest.version > active)
    }

    /// Every app with a newer release than its active version: the listed
    /// apps and the launcher. Discontinued apps get no updates.
    #[must_use]
    pub fn updates(&self) -> Vec<(AppId, Version)> {
        let apps: BTreeSet<AppId> = self
            .managed_apps()
            .iter()
            .map(|info| info.id)
            .chain(self.apps.keys().copied())
            .chain([AppId::LAUNCHER])
            .collect();
        let mut apps: Vec<AppId> = apps.into_iter().collect();
        apps.sort_by_key(|app| (self.info(*app).order, *app));
        apps.into_iter()
            .filter_map(|app| self.update_for(app).map(|r| (app, r.version.clone())))
            .collect()
    }

    /// [`State::updates`] for the managed apps, without the launcher itself.
    #[must_use]
    pub fn app_updates(&self) -> Vec<(AppId, Version)> {
        self.updates()
            .into_iter()
            .filter(|(app, _)| *app != AppId::LAUNCHER)
            .collect()
    }

    /// Replaces the release list. A launcher version published both as
    /// `launcher-v*` and as the `toolbox-v*` bridge is kept once.
    pub fn record_releases(&mut self, releases: Vec<ReleaseInfo>) {
        self.releases = release::dedupe(releases);
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
        let info = self.info(app);
        let state = self.apps.entry(app).or_default();
        if !state.is_installed(version) {
            return Err(LauncherError::Invalid(format!(
                "{} {version} is not installed",
                info.name
            )));
        }
        state.active = Some(version.clone());
        link_active(paths, &info, version)
    }

    /// Points the CLI links, desktop entries and Start Menu shortcuts of
    /// every active app at its version again, e.g. after the data directory
    /// moved. Published `.app` copies are left alone (they don't refer to
    /// the data directory). Returns the apps that failed.
    pub fn relink_active(&self, paths: &Paths) -> Vec<(AppId, LauncherError)> {
        self.apps
            .iter()
            .filter(|(app, _)| **app != AppId::LAUNCHER)
            .filter_map(|(app, state)| Some((*app, state.active.clone()?)))
            .filter_map(|(app, version)| {
                let info = self.info(app);
                link_cli(paths, &info, &version)
                    .and_then(|()| integrate_desktop(paths, &info, &version))
                    .err()
                    .map(|e| (app, e))
            })
            .collect()
    }

    /// Apps renamed since their active version was published (FerrumDock
    /// became FerrumContainers): publishes the active version under the
    /// current name and removes the `.app` copy or Start Menu shortcut with
    /// the old one. Runs at every start, so it also tidies up after a
    /// launcher that didn't know the rename (0.8.1 updated the app into
    /// `FerrumContainers.app` and left `FerrumDock.app`); it does nothing
    /// unless an old copy is still there. Returns the apps that failed.
    pub fn retire_former_names(&self, paths: &Paths) -> Vec<(AppId, LauncherError)> {
        self.apps
            .iter()
            .filter(|(app, _)| **app != AppId::LAUNCHER)
            .filter_map(|(app, state)| Some((*app, state.active.clone()?)))
            .filter_map(|(app, version)| {
                let info = self.info(app);
                let stale = paths
                    .former_integrations(&info)
                    .iter()
                    .any(|path| path.exists() || path.is_symlink());
                if !stale {
                    return None;
                }
                tracing::info!(app = %app, name = %info.name, "replacing the copy under the app's former name");
                link_active(paths, &info, &version).err().map(|e| (app, e))
            })
            .collect()
    }

    /// FerrumAgentic was renamed FerrumVibe (now FerrumEngineer) with a new
    /// id, binary and bundle, so its install can't be updated in place: this
    /// removes it (versions, `FerrumAgentic.app`, the `ferrumagentic` link
    /// and the old `fex` / `fev` command scripts that opened it) and leaves
    /// FerrumEngineer to be installed. Returns whether anything changed.
    pub fn retire_agentic(&mut self, paths: &Paths) -> bool {
        let had_install = self.apps.remove(&AppId::LEGACY_VIBE).is_some();
        let mut changed = had_install;
        let remove = |path: PathBuf| {
            if path.is_dir() && !path.is_symlink() {
                std::fs::remove_dir_all(&path).is_ok()
            } else {
                (path.exists() || path.is_symlink()) && std::fs::remove_file(&path).is_ok()
            }
        };
        changed |= remove(
            paths
                .data_dir
                .join("apps")
                .join(AppId::LEGACY_VIBE.as_str()),
        );
        if let Some(dir) = &paths.applications_dir {
            changed |= remove(dir.join("FerrumAgentic.app"));
        }
        changed |= remove(paths.bin_dir.join("ferrumagentic"));
        // Command scripts that opened FerrumAgentic. The others are brought
        // in line with the settings by `sync_shell_commands`.
        for name in ["fex", "fev"] {
            let script = paths.bin_dir.join(name);
            let stale = std::fs::read_to_string(&script).is_ok_and(|s| s.contains("ferrumagentic"));
            if stale {
                changed |= remove(script);
            }
        }
        changed
    }

    /// Writes the enabled shell commands' scripts and removes the launcher's
    /// scripts no app uses any more (see [`crate::shell::sync`]), e.g. after
    /// the renames moved `fee`, `fen` and `fev`. Returns whether anything
    /// changed, so the state (with any command moved by [`State::load`], or
    /// turned off because its name is someone else's file) is saved.
    pub fn sync_shell_commands(&mut self, paths: &Paths) -> bool {
        let mut prefs = std::mem::take(&mut self.settings.app_prefs);
        let synced = crate::shell::sync(paths, &mut prefs, |app| self.info(app));
        self.settings.app_prefs = prefs;
        synced.files_changed || !synced.turned_off.is_empty()
    }

    /// Removes an installed version. The active one can only be removed when
    /// it is the last installed version.
    pub fn uninstall(&mut self, app: AppId, version: &Version, paths: &Paths) -> Result<()> {
        let info = self.info(app);
        let state = self.apps.entry(app).or_default();
        if !state.is_installed(version) {
            return Err(LauncherError::Invalid(format!(
                "{} {version} is not installed",
                info.name
            )));
        }
        let is_active = state.active.as_ref() == Some(version);
        if is_active && state.installed.len() > 1 {
            return Err(LauncherError::Invalid(format!(
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
            unlink(paths, &info)?;
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
fn link_active(paths: &Paths, info: &AppInfo, version: &Version) -> Result<()> {
    link_cli(paths, info, version)?;
    if let Some(application) = paths.application_path(info) {
        match paths.bundle_path(info.id, version) {
            Some(bundle) => publish_bundle(&bundle, &application)?,
            None => remove_if_exists(&application)?,
        }
    }
    integrate_desktop(paths, info, version)?;
    // Only once the copy under the current name is in place.
    for former in paths.former_integrations(info) {
        remove_if_exists(&former)?;
    }
    Ok(())
}

/// `~/.ferrum/bin/<binary>` for every binary the version ships.
fn link_cli(paths: &Paths, info: &AppInfo, version: &Version) -> Result<()> {
    let dir = paths.version_dir(info.id, version);
    for binary in info.binaries() {
        let entry = paths.bin_dir.join(paths.platform.cli_entry(binary));
        match install::locate_binary(&dir, binary) {
            Some(target) if paths.platform == Platform::Windows => {
                write_atomically(&entry, platform::cmd_shim(&target).as_bytes())?
            }
            Some(target) => symlink_atomically(&target, &entry)?,
            None => remove_if_exists(&entry)?,
        }
    }
    Ok(())
}

/// The Linux desktop entry and icon, or the Windows Start Menu shortcut.
fn integrate_desktop(paths: &Paths, info: &AppInfo, version: &Version) -> Result<()> {
    let dir = paths.version_dir(info.id, version);
    let executable = install::locate_binary(&dir, &info.binary);
    if let (Some(entry), Some(icon)) = (paths.desktop_entry_path(info.id), paths.icon_path(info.id))
    {
        match &executable {
            Some(executable) => {
                let png = platform::icon_png(info.id).map(<[u8]>::to_vec).or_else(|| {
                    paths
                        .logo_path(info)
                        .and_then(|logo| std::fs::read(logo).ok())
                });
                match png {
                    Some(png) => write_atomically(&icon, &png)?,
                    None => remove_if_exists(&icon)?,
                }
                write_atomically(&entry, platform::desktop_entry(info, executable).as_bytes())?;
            }
            None => {
                remove_if_exists(&entry)?;
                remove_if_exists(&icon)?;
            }
        }
    }
    if let Some(shortcut) = paths.shortcut_path(info) {
        match &executable {
            Some(executable) => {
                if let Err(e) = create_shortcut(&shortcut, executable, &dir, info) {
                    tracing::warn!(app = %info.id, error = %e, "could not create Start Menu shortcut");
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
    info: &AppInfo,
) -> Result<()> {
    if let Some(parent) = shortcut.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let icon = working_dir.join(format!("{}.ico", info.id));
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
        .env("FERRUM_DESC", &info.description)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(LauncherError::Invalid(format!(
            "powershell exited with {status}"
        )))
    }
}

#[cfg(not(windows))]
fn create_shortcut(_: &Path, _: &Path, _: &Path, _: &AppInfo) -> Result<()> {
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
        .ok_or_else(|| LauncherError::Invalid("applications path has no parent".into()))?;
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
        return Err(LauncherError::Invalid(format!(
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

fn unlink(paths: &Paths, info: &AppInfo) -> Result<()> {
    for binary in info.binaries() {
        remove_if_exists(&paths.bin_dir.join(paths.platform.cli_entry(binary)))?;
    }
    let integrations = [
        paths.application_path(info),
        paths.desktop_entry_path(info.id),
        paths.icon_path(info.id),
        paths.shortcut_path(info),
    ];
    for path in integrations
        .into_iter()
        .flatten()
        .chain(paths.former_integrations(info))
    {
        remove_if_exists(&path)?;
    }
    Ok(())
}

/// Downloads, verifies and unpacks `release` of the app `info` describes
/// for this platform. Returns the record to pass to
/// [`State::record_install`]. Blocking extraction runs on a blocking thread.
pub async fn install_release(
    client: &GitHubClient,
    paths: &Paths,
    info: &AppInfo,
    release: &ReleaseInfo,
    progress: DownloadProgress,
    cancel: CancellationToken,
) -> Result<InstalledVersion> {
    let target = release::current_target().ok_or(LauncherError::UnsupportedPlatform)?;
    install_release_for_target(client, paths, info, release, target, progress, cancel).await
}

/// [`install_release`] for an explicit target triple.
pub async fn install_release_for_target(
    client: &GitHubClient,
    paths: &Paths,
    info: &AppInfo,
    release: &ReleaseInfo,
    target: &str,
    progress: DownloadProgress,
    cancel: CancellationToken,
) -> Result<InstalledVersion> {
    let (archive, checksum) =
        release
            .assets_for(target)
            .ok_or_else(|| LauncherError::NoAssetForTarget {
                app: info.name.clone(),
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
        .ok_or_else(|| LauncherError::Invalid(format!("unsupported archive {}", archive.name)))?;
    let destination = paths.version_dir(release.app, &release.version);
    let binary = info.binary.clone();
    let archive_path = download.clone();
    let extracted = tokio::task::spawn_blocking(move || {
        install::install_archive(&archive_path, kind, &destination, &binary)
    })
    .await
    .map_err(|e| LauncherError::Invalid(format!("install task failed: {e}")))?;
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

/// Downloads the app's catalog logo into the cache unless it is there
/// already. Returns the cached file, or `None` when the app has no logo URL.
pub async fn cache_logo(
    client: &GitHubClient,
    paths: &Paths,
    info: &AppInfo,
) -> Result<Option<PathBuf>> {
    let (Some(url), Some(path)) = (info.logo.as_deref(), paths.logo_path(info)) else {
        return Ok(None);
    };
    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Ok(Some(path));
    }
    let png = client.fetch_logo(url).await?;
    let target = path.clone();
    tokio::task::spawn_blocking(move || write_atomically(&target, &png))
        .await
        .map_err(|e| LauncherError::Invalid(format!("logo task failed: {e}")))??;
    Ok(Some(path))
}

/// Starts the active version detached from the launcher. On macOS a bundled
/// version opens through Launch Services (`open`), like from Finder.
pub fn launch(state: &State, app: AppId, paths: &Paths) -> Result<()> {
    let info = state.info(app);
    let version = state
        .active(app)
        .ok_or_else(|| LauncherError::Invalid(format!("{} is not installed", info.name)))?;
    if paths.platform == Platform::MacOs {
        let bundle = paths
            .application_path(&info)
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
                Err(LauncherError::Invalid(format!(
                    "`open {}` failed ({status})",
                    bundle.display()
                )))
            };
        }
    }
    let binary = paths.binary_path(&info, version);
    if !binary.is_file() {
        return Err(LauncherError::MissingBinary(binary.display().to_string()));
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

    fn info(app: AppId) -> AppInfo {
        AppInfo::builtin(app).expect("built-in app")
    }

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
        std::fs::write(dir.join(&info(app).binary), b"bin").expect("write");
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
        state.record_install(AppId::API, installed("0.2.0"));
        state.record_releases(vec![release(AppId::API, "0.3.0", false)]);
        state.save(&paths).expect("save");

        let loaded = State::load(&paths).expect("load");
        assert_eq!(loaded, state);
        let json = std::fs::read_to_string(paths.state_file()).expect("read");
        assert!(!json.to_lowercase().contains("token"));
    }

    #[test]
    fn running_launcher_counts_as_installed() {
        let mut state = State::default();
        state.record_releases(vec![
            release(AppId::LAUNCHER, "0.1.0", false),
            release(AppId::LAUNCHER, "0.2.0", false),
        ]);
        assert_eq!(state.active(AppId::LAUNCHER), None);

        state.running_launcher = Some(Version::new(0, 2, 0));
        assert_eq!(state.active(AppId::LAUNCHER), Some(&Version::new(0, 2, 0)));
        assert!(state.updates().is_empty());

        state.running_launcher = Some(Version::new(0, 1, 0));
        assert_eq!(
            state.updates(),
            vec![(AppId::LAUNCHER, Version::new(0, 2, 0))]
        );
        assert!(state.app_updates().is_empty());

        // A newer copy installed through the store wins over an older running one.
        state.running_launcher = Some(Version::new(0, 1, 0));
        state.record_install(AppId::LAUNCHER, installed("0.2.0"));
        assert_eq!(state.active(AppId::LAUNCHER), Some(&Version::new(0, 2, 0)));

        // Only the launcher itself is affected.
        assert_eq!(state.active(AppId::DATA), None);
    }

    fn tagged(tag: &str) -> ReleaseInfo {
        let (app, version) = release::parse_tag(tag).expect("tag");
        ReleaseInfo {
            tag: tag.into(),
            ..release(app, &version.to_string(), false)
        }
    }

    #[test]
    fn launcher_updates_consider_both_tags_and_never_downgrade() {
        let mut state = State {
            running_launcher: Some(Version::new(0, 6, 0)),
            ..State::default()
        };
        // The bridge (toolbox-v0.6.0) is the version already running, and
        // older toolbox-v* releases are older still.
        state.record_releases(vec![
            tagged("toolbox-v0.5.0"),
            tagged("toolbox-v0.6.0"),
            tagged("launcher-v0.6.0"),
        ]);
        assert!(state.updates().is_empty());
        assert_eq!(state.releases_for(AppId::LAUNCHER).len(), 2);
        state.record_releases(vec![tagged("toolbox-v0.6.0")]);
        assert!(state.updates().is_empty());

        state.record_releases(vec![
            tagged("toolbox-v0.6.0"),
            tagged("launcher-v0.6.0"),
            tagged("launcher-v0.7.0"),
        ]);
        assert_eq!(
            state.updates(),
            vec![(AppId::LAUNCHER, Version::new(0, 7, 0))]
        );
        assert_eq!(
            state.update_for(AppId::LAUNCHER).map(|r| r.tag.as_str()),
            Some("launcher-v0.7.0")
        );
        assert!(state.app_updates().is_empty());

        // A newer toolbox-v* tag counts too: the newest wins.
        state.record_releases(vec![tagged("launcher-v0.6.0"), tagged("toolbox-v0.6.1")]);
        assert_eq!(
            state.update_for(AppId::LAUNCHER).map(|r| r.tag.as_str()),
            Some("toolbox-v0.6.1")
        );
    }

    #[test]
    fn new_apps_are_announced_once_and_badged_until_seen() {
        let mut state = State::default();
        // A fresh install knows every app already.
        assert!(state.discover_apps().is_empty());
        assert!(state.settings.new_apps.is_empty());

        let forge = AppId::new("forge").expect("id");
        state.record_catalog(
            Catalog::parse(
                r#"{"apps":[{"id":"forge","status":"alpha","summary":"Builds.",
                    "name":"FerrumForge","binary":"ferrumforge","bundle_name":"FerrumForge"}]}"#,
            )
            .expect("catalog"),
        );
        assert!(state.managed_apps().iter().any(|info| info.id == forge));
        assert!(
            state
                .managed_apps()
                .iter()
                .all(|info| info.id != AppId::LAUNCHER)
        );
        assert_eq!(state.discover_apps(), vec![forge]);
        assert!(state.is_new(forge));
        assert!(state.discover_apps().is_empty(), "announced once");

        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        state.save(&paths).expect("save");
        let mut loaded = State::load(&paths).expect("load");
        assert!(loaded.is_new(forge), "the badge survives a restart");
        assert!(loaded.mark_seen(forge));
        assert!(!loaded.is_new(forge));
        assert!(!loaded.mark_seen(forge));
        assert_eq!(loaded.info(forge).binary, "ferrumforge");
    }

    #[test]
    fn unknown_catalog_apps_install_activate_and_launch_like_builtins() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let mut state = State::default();
        state.record_catalog(
            Catalog::parse(
                r#"{"apps":[{"id":"forge","status":"alpha","summary":"Builds.",
                    "name":"FerrumForge","binary":"ferrumforge","extra_binaries":["fforge"],
                    "bundle_name":"FerrumForge"}]}"#,
            )
            .expect("catalog"),
        );
        let forge = AppId::new("forge").expect("id");
        let info = state.info(forge);
        let version = Version::new(1, 0, 0);
        let macos = paths
            .version_dir(forge, &version)
            .join("FerrumForge.app/Contents/MacOS");
        std::fs::create_dir_all(&macos).expect("mkdir");
        std::fs::write(macos.join("ferrumforge"), b"bin").expect("write");
        std::fs::write(macos.join("fforge"), b"bin").expect("write");
        state.record_install(forge, installed("1.0.0"));
        state.activate(forge, &version, &paths).expect("activate");
        assert!(
            paths
                .application_path(&info)
                .expect("app")
                .ends_with("FerrumForge.app")
        );
        assert!(paths.application_path(&info).expect("app").is_dir());
        assert!(paths.bin_dir.join("fforge").exists());
        state.record_releases(vec![tagged("forge-v1.1.0")]);
        assert_eq!(state.app_updates(), vec![(forge, Version::new(1, 1, 0))]);
        state.uninstall(forge, &version, &paths).expect("uninstall");
        assert!(!paths.bin_dir.join("ferrumforge").exists());
    }

    #[test]
    fn ferrumagentic_becomes_ferrumvibe() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let mut state: State = serde_json::from_str(
            r#"{"settings":{"channels_revision":99,"app_prefs":{"agentic":{"command":"fex","hide_from_dock":true},
                "env":{"command":"fev"}},"seen_apps":["agentic","env"]},
                "apps":{"agentic":{"installed":[],"active":null,"channel":"beta"}}}"#,
        )
        .expect("state");
        state.migrate();
        // fex → fev (FerrumVibe) → fen (FerrumEngineer); FerrumEnv's fev →
        // fen → fev (FerrumVariables).
        assert_eq!(
            state.settings.app_prefs[&AppId::VIBE].command.as_deref(),
            Some("fen")
        );
        assert!(state.settings.app_prefs[&AppId::VIBE].hide_from_dock);
        assert_eq!(
            state.settings.app_prefs[&AppId::ENV].command.as_deref(),
            Some("fev")
        );
        assert!(
            state
                .settings
                .seen_apps
                .as_ref()
                .expect("seen")
                .contains(&AppId::VIBE)
        );
        assert_eq!(state.apps[&AppId::VIBE].channel, UpdateChannel::Beta);

        let old = dir.path().join("data/apps/agentic/0.4.2-alpha.1");
        std::fs::create_dir_all(&old).expect("old install");
        std::fs::create_dir_all(&paths.bin_dir).expect("bin");
        std::fs::write(paths.bin_dir.join("fev"), "exec ferrumenv").expect("fev");
        std::fs::write(paths.bin_dir.join("fex"), "exec ferrumagentic").expect("fex");
        std::fs::write(paths.bin_dir.join("fea"), "exec ferrumapi").expect("fea");
        assert!(state.retire_agentic(&paths));
        assert!(!state.apps.contains_key(&AppId::LEGACY_VIBE));
        assert!(!paths.bin_dir.join("fex").exists());
        assert!(
            paths.bin_dir.join("fev").exists(),
            "FerrumVariables' command now"
        );
        assert!(paths.bin_dir.join("fea").exists());
        assert!(!state.retire_agentic(&paths), "nothing left to do");
    }

    #[cfg(unix)]
    #[test]
    fn renamed_apps_shell_commands_move_once() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        std::fs::create_dir_all(&paths.bin_dir).expect("bin");
        // A state file from before FerrumIDE, FerrumEngineer and
        // FerrumVariables, with its scripts.
        std::fs::create_dir_all(&paths.data_dir).expect("data");
        std::fs::write(
            paths.state_file(),
            r#"{"format":1,"settings":{"channels_revision":1,"app_prefs":{
                "editor":{"command":"fee"},"vibe":{"command":"fev","hide_from_dock":true},
                "env":{"command":"fen"},"git":{"command":"feg"}}}}"#,
        )
        .expect("state");
        let old = |app: AppId, command: &str| {
            let info = Catalog::embedded().info(app).expect("app");
            let entry = paths.bin_dir.join(&info.binary);
            std::fs::write(
                paths.bin_dir.join(command),
                crate::shell::script(&info, command, &entry),
            )
            .expect("script");
        };
        old(AppId::EDITOR, "fee");
        old(AppId::VIBE, "fev");
        old(AppId::ENV, "fen");
        old(AppId::GIT, "feg");
        let mut state = State::load(&paths).expect("load");
        let command = |state: &State, app: AppId| {
            state.settings.app_prefs[&app]
                .command
                .clone()
                .unwrap_or_default()
        };
        assert_eq!(command(&state, AppId::EDITOR), "fei");
        assert_eq!(command(&state, AppId::VIBE), "fen");
        assert_eq!(command(&state, AppId::ENV), "fev");
        assert_eq!(command(&state, AppId::GIT), "feg");
        assert!(state.settings.app_prefs[&AppId::VIBE].hide_from_dock);
        assert!(state.sync_shell_commands(&paths), "scripts moved");
        let script = |name: &str| std::fs::read_to_string(paths.bin_dir.join(name)).ok();
        assert!(script("fee").is_none());
        assert!(script("fei").expect("fei").contains("/ferrumeditor'"));
        assert!(script("fen").expect("fen").contains("/ferrumvibe'"));
        assert!(script("fev").expect("fev").contains("/ferrumenv'"));
        state.save(&paths).expect("save");
        // Later the user gives FerrumVariables `fen` back (Engineer's is off):
        // the move isn't made again.
        state
            .settings
            .app_prefs
            .get_mut(&AppId::VIBE)
            .expect("vibe")
            .command = None;
        state
            .settings
            .app_prefs
            .get_mut(&AppId::ENV)
            .expect("env")
            .command = Some("fen".into());
        state.save(&paths).expect("save");
        let mut state = State::load(&paths).expect("load");
        assert_eq!(command(&state, AppId::ENV), "fen");
        assert!(state.sync_shell_commands(&paths));
        assert!(script("fen").expect("fen").contains("/ferrumenv'"));
        assert!(script("fev").is_none(), "no app has fev now");
        assert!(!state.sync_shell_commands(&paths), "nothing left to do");
        // A new install starts at the current revision.
        assert_eq!(
            State::default().settings.commands_revision,
            COMMANDS_REVISION
        );
    }

    #[test]
    fn installed_retired_app_is_discontinued_but_launchable_and_removable() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let ide = AppId::new("ide").expect("id");
        // A cached catalog from before Ferrum IDE was dropped.
        let mut state = State {
            catalog: Some(
                Catalog::parse(
                    r#"{"apps":[{"id":"ide","status":"alpha","summary":"Editor","name":"FerrumIDE",
                        "binary":"zed","bundle_name":"Ferrum IDE"}]}"#,
                )
                .expect("catalog"),
            ),
            ..State::default()
        };
        let version = Version::new(0, 3, 0);
        let macos = paths
            .version_dir(ide, &version)
            .join("Ferrum IDE.app/Contents/MacOS");
        std::fs::create_dir_all(&macos).expect("mkdir");
        std::fs::write(macos.join("zed"), b"bin").expect("write");
        state.record_install(ide, installed("0.3.0"));
        state.activate(ide, &version, &paths).expect("activate");
        assert!(paths.bin_dir.join("zed").exists());

        assert!(state.managed_apps().iter().all(|info| info.id != ide));
        assert!(!state.is_offered(ide));
        let discontinued = state.discontinued_apps();
        assert_eq!(discontinued.len(), 1);
        assert_eq!(discontinued[0].name, "Ferrum IDE");
        assert_eq!(state.info(ide).bundle_name, "Ferrum IDE");
        assert!(state.discover_apps().is_empty(), "never announced");

        // Releases of it are still published, but nothing offers an update.
        state.record_releases(vec![tagged("ide-v0.4.0")]);
        assert!(state.update_for(ide).is_none());
        assert!(state.updates().is_empty());

        state.uninstall(ide, &version, &paths).expect("uninstall");
        assert!(!paths.bin_dir.join("zed").exists());
        assert!(!paths.version_dir(ide, &version).exists());
        assert!(
            state.discontinued_apps().is_empty(),
            "gone once uninstalled"
        );
    }

    #[test]
    fn reports_updates_only_for_installed_apps() {
        let mut state = State::default();
        state.record_releases(vec![
            release(AppId::API, "0.3.0", false),
            release(AppId::API, "0.4.0-beta.1", true),
            release(AppId::GIT, "1.0.0", false),
        ]);
        state.record_install(AppId::API, installed("0.2.0"));
        state.set_channel(AppId::API, UpdateChannel::Stable);
        assert_eq!(state.updates(), vec![(AppId::API, Version::new(0, 3, 0))]);
        state.set_channel(AppId::API, UpdateChannel::Beta);
        assert_eq!(
            state.updates(),
            vec![(AppId::API, Version::parse("0.4.0-beta.1").expect("semver"))]
        );
        assert_eq!(state.releases_for(AppId::API).len(), 2);
        assert_eq!(
            state
                .latest_in(AppId::API, Channel::Stable)
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
            AppId::BUILTIN
                .iter()
                .all(|app| state.channel(*app) == UpdateChannel::Alpha)
        );
        assert!(!state.settings.legacy_include_prereleases);
        state.save(&paths).expect("save");
        let json = std::fs::read_to_string(paths.state_file()).expect("read");
        assert!(!json.contains("include_prereleases"));
        assert!(json.contains(r#""channel": "alpha""#));

        std::fs::write(paths.state_file(), r#"{"settings":{"repo_owner":"o"}}"#).expect("write");
        let fresh = State::load(&paths).expect("load");
        assert_eq!(fresh.channel(AppId::DATA), UpdateChannel::Alpha);
    }

    #[test]
    fn moves_every_app_to_alpha_once() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        std::fs::create_dir_all(&paths.data_dir).expect("mkdir");
        std::fs::write(
            paths.state_file(),
            r#"{"settings":{"repo_owner":"o","repo_name":"r"},
                "apps":{"api":{"channel":"stable"},"git":{"channel":"beta"}}}"#,
        )
        .expect("write");
        let mut state = State::load(&paths).expect("load");
        assert_eq!(state.channel(AppId::API), UpdateChannel::Alpha);
        assert_eq!(state.channel(AppId::GIT), UpdateChannel::Alpha);
        assert_eq!(state.channel(AppId::LAUNCHER), UpdateChannel::Alpha);

        state.set_channel(AppId::API, UpdateChannel::Stable);
        state.save(&paths).expect("save");
        let reloaded = State::load(&paths).expect("load");
        assert_eq!(reloaded.channel(AppId::API), UpdateChannel::Stable);
    }

    #[test]
    fn moves_the_private_repo_setting_to_the_public_repo() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        std::fs::create_dir_all(&paths.data_dir).expect("mkdir");
        std::fs::write(
            paths.state_file(),
            r#"{"settings":{"repo_owner":"Amygdal-doo","repo_name":"ferrum"}}"#,
        )
        .expect("write");
        let state = State::load(&paths).expect("load");
        assert!(state.settings.uses_public_repo());

        std::fs::write(
            paths.state_file(),
            r#"{"settings":{"repo_owner":"me","repo_name":"ferrum"}}"#,
        )
        .expect("write");
        let custom = State::load(&paths).expect("load");
        assert_eq!(custom.settings.repo_name, "ferrum");
        assert!(!custom.settings.uses_public_repo());
        assert!(Settings::default().uses_public_repo());
    }

    #[test]
    fn summarizes_releases_and_catalog() {
        let mut state = State::default();
        let mut older = release(AppId::DATA, "0.1.0", false);
        older.published_at = DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z")
            .ok()
            .map(|t| t.with_timezone(&Utc));
        older.assets = vec![Asset {
            name: "FerrumData-0.1.0-aarch64.dmg".into(),
            size: 1,
            url: String::new(),
            download_count: 3,
        }];
        let mut newer = release(AppId::DATA, "0.2.0-beta.1", true);
        newer.published_at = DateTime::parse_from_rfc3339("2026-11-05T00:00:00Z")
            .ok()
            .map(|t| t.with_timezone(&Utc));
        newer.assets = vec![Asset {
            name: "data.tar.gz".into(),
            size: 1,
            url: String::new(),
            download_count: 2,
        }];
        let api = release(AppId::API, "1.0.0", false);
        state.record_releases(vec![older, api, newer]);
        assert_eq!(state.total_downloads(AppId::DATA), 5);
        assert_eq!(
            state.last_released(AppId::DATA),
            state.releases[2].published_at
        );
        let timeline: Vec<_> = state.timeline().iter().map(|r| r.tag.clone()).collect();
        assert_eq!(timeline[0], "data-v0.2.0-beta.1");
        assert_eq!(timeline[1], "data-v0.1.0");

        assert!(state.catalog_entry(AppId::TERM).is_some());
        state.record_catalog(
            Catalog::parse(r#"{"apps":[{"id":"term","status":"stable","summary":"Done"}]}"#)
                .expect("parse"),
        );
        assert_eq!(
            state.catalog_entry(AppId::TERM).map(|e| e.status),
            Some(crate::AppStatus::Stable)
        );
        assert!(state.catalog_entry(AppId::DATA).is_some());
    }

    #[test]
    fn activate_links_binary_and_uninstall_guards_active() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let mut state = State::default();
        fake_install(&mut state, &paths, AppId::API, "0.1.0");
        fake_install(&mut state, &paths, AppId::API, "0.2.0");
        let v1 = Version::new(0, 1, 0);
        let v2 = Version::new(0, 2, 0);
        assert_eq!(state.active(AppId::API), Some(&v1));

        state.activate(AppId::API, &v2, &paths).expect("activate");
        assert_eq!(state.active(AppId::API), Some(&v2));
        #[cfg(unix)]
        assert_eq!(
            std::fs::read_link(paths.link_path(&info(AppId::API))).expect("link"),
            paths.binary_path(&info(AppId::API), &v2)
        );

        assert!(state.uninstall(AppId::API, &v2, &paths).is_err());
        state
            .uninstall(AppId::API, &v1, &paths)
            .expect("remove inactive");
        assert!(!paths.version_dir(AppId::API, &v1).exists());
        state
            .uninstall(AppId::API, &v2, &paths)
            .expect("remove last");
        assert_eq!(state.active(AppId::API), None);
        assert!(!paths.link_path(&info(AppId::API)).exists());
        assert!(state.activate(AppId::API, &v2, &paths).is_err());
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
        let version_dir = paths.version_dir(AppId::TERM, &version);
        std::fs::create_dir_all(&version_dir).expect("mkdir");
        std::fs::write(version_dir.join("ferrumterm.exe"), b"MZ").expect("write");
        std::fs::write(version_dir.join("fterm.exe"), b"MZ").expect("write");
        state.record_install(AppId::TERM, installed("0.3.0"));

        state
            .activate(AppId::TERM, &version, &paths)
            .expect("activate");
        let shim = std::fs::read_to_string(paths.bin_dir.join("fterm.cmd")).expect("shim");
        assert_eq!(shim, platform::cmd_shim(&version_dir.join("fterm.exe")));
        assert_eq!(
            paths.link_path(&info(AppId::TERM)),
            paths.bin_dir.join("ferrumterm.cmd")
        );
        assert!(paths.link_path(&info(AppId::TERM)).is_file());

        state
            .uninstall(AppId::TERM, &version, &paths)
            .expect("uninstall");
        assert!(!paths.bin_dir.join("fterm.cmd").exists());
        assert!(!paths.link_path(&info(AppId::TERM)).exists());
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
        fake_install(&mut state, &paths, AppId::DATA, "1.0.0");
        let version = Version::new(1, 0, 0);
        state
            .activate(AppId::DATA, &version, &paths)
            .expect("activate");

        let entry_path = dir.path().join("share/applications/ferrum-data.desktop");
        let entry = std::fs::read_to_string(&entry_path).expect("desktop entry");
        assert!(entry.contains(&format!(
            "Exec=\"{}\"",
            paths.binary_path(&info(AppId::DATA), &version).display()
        )));
        let icon = dir
            .path()
            .join("share/icons/hicolor/256x256/apps/ferrum-data.png");
        assert!(std::fs::read(&icon).expect("icon").starts_with(b"\x89PNG"));

        state
            .uninstall(AppId::DATA, &version, &paths)
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
            fake_install(&mut state, &paths, AppId::API, v);
        }
        // 0.1.0 is active (first install); keep 3 → keep 0.5, 0.4 and the active 0.1.
        state.settings.keep_versions = 3;
        let candidates = state.prune_candidates(AppId::API);
        assert_eq!(
            candidates,
            vec![Version::new(0, 3, 0), Version::new(0, 2, 0)]
        );
        let removed = state.prune(AppId::API, &paths).expect("prune");
        assert_eq!(removed.len(), 2);
        let left: Vec<String> = state
            .app(AppId::API)
            .installed
            .iter()
            .map(|v| v.version.to_string())
            .collect();
        assert_eq!(left, ["0.5.0", "0.4.0", "0.1.0"]);

        state.settings.keep_versions = 0;
        assert!(state.prune_candidates(AppId::API).is_empty());
    }

    fn fake_bundle_install(state: &mut State, paths: &Paths, app: AppId, version: &str) {
        let v = Version::parse(version).expect("semver");
        let macos = paths
            .version_dir(app, &v)
            .join(format!("{}.app", info(app).bundle_name))
            .join("Contents/MacOS");
        std::fs::create_dir_all(&macos).expect("mkdir");
        for binary in info(app).binaries() {
            std::fs::write(macos.join(binary), version.as_bytes()).expect("write");
        }
        state.record_install(app, installed(version));
    }

    #[test]
    fn activating_a_bundle_publishes_the_app_and_links_every_binary() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let mut state = State::default();
        fake_bundle_install(&mut state, &paths, AppId::TERM, "0.1.0");
        fake_bundle_install(&mut state, &paths, AppId::TERM, "0.2.0");
        let v1 = Version::new(0, 1, 0);
        let v2 = Version::new(0, 2, 0);
        let application = paths
            .application_path(&info(AppId::TERM))
            .expect("app path");
        assert!(application.ends_with("Applications/Ferrum/FerrumTerm.app"));

        state.activate(AppId::TERM, &v2, &paths).expect("activate");
        let published = application.join("Contents/MacOS/ferrumterm");
        assert_eq!(std::fs::read(&published).expect("copy"), b"0.2.0");
        assert!(!application.is_symlink(), "Launchpad needs a real bundle");
        #[cfg(unix)]
        {
            let bundle = paths.bundle_path(AppId::TERM, &v2).expect("bundle");
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
        state.activate(AppId::TERM, &v1, &paths).expect("rollback");
        assert_eq!(std::fs::read(&published).expect("copy"), b"0.1.0");
        let leftovers: Vec<_> = std::fs::read_dir(application.parent().expect("parent"))
            .expect("read")
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(leftovers.len(), 1, "no staging copies left behind");

        state
            .uninstall(AppId::TERM, &v2, &paths)
            .expect("remove inactive");
        assert!(application.is_dir());
        state
            .uninstall(AppId::TERM, &v1, &paths)
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
        fake_bundle_install(&mut state, &paths, AppId::DATA, "0.1.0");
        fake_install(&mut state, &paths, AppId::DATA, "0.2.0");
        state
            .activate(AppId::DATA, &Version::new(0, 1, 0), &paths)
            .expect("bundle");
        let application = paths
            .application_path(&info(AppId::DATA))
            .expect("app path");
        assert!(application.is_dir());
        state
            .activate(AppId::DATA, &Version::new(0, 2, 0), &paths)
            .expect("plain");
        assert!(!application.exists());
        assert_eq!(
            paths.binary_path(&info(AppId::DATA), &Version::new(0, 2, 0)),
            paths
                .version_dir(AppId::DATA, &Version::new(0, 2, 0))
                .join("ferrumdata")
        );
    }

    /// A FerrumDock release as published before the rename: its archive
    /// holds `FerrumDock.app`.
    fn fake_dock_bundle(state: &mut State, paths: &Paths, version: &str, bundle_name: &str) {
        let v = Version::parse(version).expect("semver");
        let macos = paths
            .version_dir(AppId::DOCK, &v)
            .join(format!("{bundle_name}.app"))
            .join("Contents/MacOS");
        std::fs::create_dir_all(&macos).expect("mkdir");
        std::fs::write(macos.join("ferrumdock"), version.as_bytes()).expect("write");
        state.record_install(AppId::DOCK, installed(version));
    }

    /// What Launcher 0.8.1 makes of the catalog that renames the app: the
    /// new name and bundle name, and no `former_names` (it doesn't know
    /// the field).
    fn dock_as_0_8_1_reads_it() -> AppInfo {
        AppInfo {
            former_names: Vec::new(),
            ..info(AppId::DOCK)
        }
    }

    #[test]
    fn ferrumdock_is_now_ferrumcontainers_with_the_same_id_binary_and_data() {
        let dock = info(AppId::DOCK);
        assert_eq!(dock.id.as_str(), "dock");
        assert_eq!(dock.name, "FerrumContainers");
        assert_eq!(dock.bundle_name, "FerrumContainers");
        assert_eq!(dock.binary, "ferrumdock");
        assert_eq!(dock.former_names, ["FerrumDock"]);
        let paths = paths(Path::new("/u"));
        assert_eq!(
            paths.version_dir(AppId::DOCK, &Version::new(0, 2, 0)),
            Path::new("/u/data/apps/dock/0.2.0")
        );
        assert_eq!(
            paths.former_integrations(&dock),
            [PathBuf::from("/u/Applications/Ferrum/FerrumDock.app")]
        );
        assert!(paths.former_integrations(&info(AppId::API)).is_empty());
    }

    #[test]
    fn renamed_app_moves_to_its_new_bundle_name_and_keeps_its_settings() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let mut state = State::default();
        let v2 = Version::new(0, 2, 0);
        let v3 = Version::new(0, 3, 0);
        // FerrumDock 0.2.0, activated by a launcher that called it FerrumDock.
        fake_dock_bundle(&mut state, &paths, "0.2.0", "FerrumDock");
        let old = AppInfo {
            name: "FerrumDock".into(),
            bundle_name: "FerrumDock".into(),
            former_names: Vec::new(),
            ..info(AppId::DOCK)
        };
        link_active(&paths, &old, &v2).expect("published as FerrumDock");
        let applications = paths.applications_dir.clone().expect("dir");
        let old_copy = applications.join("FerrumDock.app");
        let new_copy = applications.join("FerrumContainers.app");
        assert!(old_copy.is_dir());
        state.settings.app_prefs.insert(
            AppId::DOCK,
            AppPrefs {
                command: Some("fec".into()),
                hide_from_dock: true,
                hotkey: Some("cmd+alt+c".into()),
            },
        );
        state.save(&paths).expect("save");

        // This launcher starts: the copy moves to the new name.
        let mut state = State::load(&paths).expect("load");
        assert!(state.retire_former_names(&paths).is_empty());
        assert!(!old_copy.exists());
        assert_eq!(
            std::fs::read(new_copy.join("Contents/MacOS/ferrumdock")).expect("copy"),
            b"0.2.0",
            "the installed version, published under the new name"
        );
        assert_eq!(
            paths.application_path(&state.info(AppId::DOCK)),
            Some(new_copy.clone())
        );
        #[cfg(unix)]
        assert_eq!(
            std::fs::read_link(paths.link_path(&info(AppId::DOCK))).expect("cli"),
            paths
                .version_dir(AppId::DOCK, &v2)
                .join("FerrumDock.app/Contents/MacOS/ferrumdock"),
            "the CLI link (and the shell command running it) keeps working"
        );
        // Nothing left to do on the next start.
        assert!(state.retire_former_names(&paths).is_empty());
        assert!(new_copy.is_dir());

        // The update ships FerrumContainers.app.
        fake_dock_bundle(&mut state, &paths, "0.3.0", "FerrumContainers");
        state.activate(AppId::DOCK, &v3, &paths).expect("update");
        assert_eq!(
            std::fs::read(new_copy.join("Contents/MacOS/ferrumdock")).expect("copy"),
            b"0.3.0"
        );
        assert!(!old_copy.exists());
        let published: Vec<_> = std::fs::read_dir(&applications)
            .expect("read")
            .filter_map(|e| e.ok().map(|e| e.file_name()))
            .collect();
        assert_eq!(published, ["FerrumContainers.app"]);
        // Rolling back to the version shipped as FerrumDock.app keeps the
        // new name.
        state.activate(AppId::DOCK, &v2, &paths).expect("rollback");
        assert_eq!(
            std::fs::read(new_copy.join("Contents/MacOS/ferrumdock")).expect("copy"),
            b"0.2.0"
        );
        assert!(!old_copy.exists());

        // Shell command, Dock icon and shortcut are keyed by the id (and
        // the Dock choice by the binary), which didn't change.
        state.save(&paths).expect("save");
        let state = State::load(&paths).expect("reload");
        let prefs = &state.settings.app_prefs[&AppId::DOCK];
        assert_eq!(prefs.command.as_deref(), Some("fec"));
        assert!(prefs.hide_from_dock);
        assert_eq!(prefs.hotkey.as_deref(), Some("cmd+alt+c"));
        assert_eq!(state.info(AppId::DOCK).binary, "ferrumdock");
    }

    #[test]
    fn copy_left_by_launcher_0_8_1_is_removed_on_the_next_start() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let mut state = State::default();
        fake_dock_bundle(&mut state, &paths, "0.2.0", "FerrumDock");
        let applications = paths.applications_dir.clone().expect("dir");
        let old_copy = applications.join("FerrumDock.app");
        let new_copy = applications.join("FerrumContainers.app");
        std::fs::create_dir_all(old_copy.join("Contents/MacOS")).expect("old copy");

        // 0.8.1 launches the not-yet-updated app from its install folder:
        // no FerrumContainers.app yet, so it falls back to the bundle.
        let info_0_8_1 = dock_as_0_8_1_reads_it();
        assert!(!new_copy.exists());
        assert!(
            paths
                .bundle_path(AppId::DOCK, &Version::new(0, 2, 0))
                .is_some_and(|bundle| bundle.ends_with("FerrumDock.app"))
        );
        // 0.8.1 updates it: FerrumContainers.app is published, and
        // FerrumDock.app (the 0.2.0 copy) is left behind.
        fake_dock_bundle(&mut state, &paths, "0.3.0", "FerrumContainers");
        state.apps.get_mut(&AppId::DOCK).expect("dock").active = Some(Version::new(0, 3, 0));
        link_active(&paths, &info_0_8_1, &Version::new(0, 3, 0)).expect("0.8.1 update");
        assert!(new_copy.is_dir());
        assert!(old_copy.is_dir(), "0.8.1 doesn't know the former name");

        // This launcher's next start removes it.
        assert!(state.retire_former_names(&paths).is_empty());
        assert!(!old_copy.exists());
        assert_eq!(
            std::fs::read(new_copy.join("Contents/MacOS/ferrumdock")).expect("copy"),
            b"0.3.0"
        );

        // Uninstalling removes either name.
        std::fs::create_dir_all(&old_copy).expect("old copy again");
        state
            .uninstall(AppId::DOCK, &Version::new(0, 2, 0), &paths)
            .expect("inactive");
        state
            .uninstall(AppId::DOCK, &Version::new(0, 3, 0), &paths)
            .expect("last");
        assert!(!new_copy.exists());
        assert!(!old_copy.exists());
    }

    #[test]
    fn a_catalog_still_naming_the_app_ferrumdock_removes_nothing() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let mut state = State::default();
        // The remote catalog from before the rename.
        state.record_catalog(
            Catalog::parse(
                r#"{"apps":[{"id":"dock","status":"alpha","summary":"S","name":"FerrumDock",
                    "binary":"ferrumdock","bundle_name":"FerrumDock"}]}"#,
            )
            .expect("parse"),
        );
        let dock = state.info(AppId::DOCK);
        assert_eq!(dock.bundle_name, "FerrumDock");
        assert!(paths.former_integrations(&dock).is_empty());
        fake_dock_bundle(&mut state, &paths, "0.2.0", "FerrumDock");
        state
            .activate(AppId::DOCK, &Version::new(0, 2, 0), &paths)
            .expect("activate");
        let copy = paths.application_path(&dock).expect("path");
        assert!(copy.ends_with("FerrumDock.app") && copy.is_dir());
        assert!(state.retire_former_names(&paths).is_empty());
        assert!(copy.is_dir(), "the current name is never removed");
    }

    #[test]
    fn windows_shortcut_under_the_former_name_is_replaced() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = Paths {
            platform: Platform::Windows,
            applications_dir: None,
            start_menu_dir: Some(dir.path().join("Start Menu/Ferrum")),
            ..paths(dir.path())
        };
        let mut state = State::default();
        let version = Version::new(0, 2, 0);
        let version_dir = paths.version_dir(AppId::DOCK, &version);
        std::fs::create_dir_all(&version_dir).expect("mkdir");
        std::fs::write(version_dir.join("ferrumdock.exe"), b"MZ").expect("write");
        state.record_install(AppId::DOCK, installed("0.2.0"));
        let start_menu = paths.start_menu_dir.clone().expect("dir");
        std::fs::create_dir_all(&start_menu).expect("mkdir");
        let old_shortcut = start_menu.join("FerrumDock.lnk");
        std::fs::write(&old_shortcut, b"lnk").expect("old shortcut");
        assert_eq!(
            paths.shortcut_path(&info(AppId::DOCK)),
            Some(start_menu.join("FerrumContainers.lnk"))
        );

        assert!(state.retire_former_names(&paths).is_empty());
        assert!(!old_shortcut.exists());
        assert!(paths.bin_dir.join("ferrumdock.cmd").is_file());
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
        let name = release::bundle_archive_name(AppId::DATA, &version, target);
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
        let mut data = release(AppId::DATA, "0.3.0", false);
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
            &info(AppId::DATA),
            &data,
            target,
            Arc::new(|_, _| {}),
            CancellationToken::new(),
        )
        .await
        .expect("install");
        assert_eq!(record.asset, name);
        let mut state = State::default();
        state.record_install(AppId::DATA, record);
        state
            .activate(AppId::DATA, &version, &paths)
            .expect("activate");
        let application = paths
            .application_path(&info(AppId::DATA))
            .expect("app path");
        assert!(application.join("Contents/MacOS/ferrumdata").is_file());
        #[cfg(unix)]
        assert!(
            std::fs::canonicalize(paths.link_path(&info(AppId::DATA)))
                .expect("link resolves")
                .ends_with("FerrumData.app/Contents/MacOS/ferrumdata")
        );
    }

    /// Opt-in check against a real bundle archive and the real per-user paths:
    /// `FERRUM_E2E_APP_ARCHIVE=<data .app.tar.gz> HOME=<temp> cargo test -p
    /// ferrumlauncher-core real_bundle -- --ignored`.
    #[tokio::test]
    #[ignore = "needs FERRUM_E2E_APP_ARCHIVE and a throwaway HOME"]
    async fn real_bundle_installs_into_user_paths() {
        let archive = std::env::var("FERRUM_E2E_APP_ARCHIVE").expect("FERRUM_E2E_APP_ARCHIVE");
        let paths = Paths::default_for_user().expect("paths");
        let bytes = std::fs::read(&archive).expect("archive");
        let sha = crate::github::to_hex(&Sha256::digest(&bytes));
        let target = release::current_target().expect("target");
        let version = Version::new(0, 0, 1);
        let name = release::bundle_archive_name(AppId::DATA, &version, target);
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
        let mut data = release(AppId::DATA, "0.0.1", false);
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
            &info(AppId::DATA),
            &data,
            Arc::new(|_, _| {}),
            CancellationToken::new(),
        )
        .await
        .expect("install");
        let mut state = State::load(&paths).expect("load");
        state.record_install(AppId::DATA, record);
        state
            .activate(AppId::DATA, &version, &paths)
            .expect("activate");
        state.save(&paths).expect("save");
        let application = paths.application_path(&info(AppId::DATA)).expect("macOS");
        println!("application: {}", application.display());
        println!(
            "link: {} -> {}",
            paths.link_path(&info(AppId::DATA)).display(),
            std::fs::canonicalize(paths.link_path(&info(AppId::DATA)))
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
        let name = release::archive_name(AppId::API, &Version::new(0, 2, 0), target);

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

        let mut bad = release(AppId::API, "0.2.0", false);
        bad.assets = vec![
            asset("/a/good", &name),
            asset("/a/bad.sha", &format!("{name}.sha256")),
        ];
        let result = install_release(
            &client,
            &paths,
            &info(AppId::API),
            &bad,
            Arc::new(|_, _| {}),
            CancellationToken::new(),
        )
        .await;
        assert!(matches!(
            result,
            Err(LauncherError::ChecksumMismatch { .. })
        ));
        assert!(
            !paths
                .version_dir(AppId::API, &Version::new(0, 2, 0))
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
            &info(AppId::API),
            &good,
            Arc::new(|_, _| {}),
            CancellationToken::new(),
        )
        .await
        .expect("install");
        assert_eq!(record.asset, name);
        assert!(
            paths
                .binary_path(&info(AppId::API), &Version::new(0, 2, 0))
                .is_file()
        );
        let leftover_downloads = std::fs::read_dir(paths.downloads_dir())
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(leftover_downloads, 0);
    }
}
