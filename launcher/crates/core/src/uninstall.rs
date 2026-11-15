//! Uninstalling an app completely: every installed version, the published
//! `.app` (or desktop entry / Start Menu shortcut) and the CLI links, and on
//! request the app's settings and data in `~/.ferrum`.

use crate::catalog::{AppId, AppInfo};
use crate::store::{Paths, State};
use crate::{LauncherError, Result};
use semver::Version;
use std::path::{Path, PathBuf};

/// What [`State::uninstall_app`] removes, for the confirmation dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    /// Every installed version, newest first.
    pub versions: Vec<Version>,
    /// `<data_dir>/apps/<id>`: the folder holding the installed versions.
    pub versions_dir: PathBuf,
    /// The copy in `~/Applications/Ferrum` (macOS), when there is one.
    pub application: Option<PathBuf>,
    /// The app's settings and data folders in `~/.ferrum` that exist; only
    /// removed when asked.
    pub data_dirs: Vec<PathBuf>,
}

/// `~/.ferrum/<id>` and `~/.ferrum/<binary>` (apps keep their settings under
/// one or the other), whether they exist or not. Never `~/.ferrum` itself or
/// the shared `bin` folder.
#[must_use]
pub fn data_dirs(ferrum_dir: &Path, info: &AppInfo) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for name in [info.id.as_str(), info.binary.as_str()] {
        let usable = !name.is_empty()
            && name != "bin"
            && !name.starts_with('.')
            && !name.contains(['/', '\\']);
        let dir = ferrum_dir.join(name);
        if usable && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// `~/.ferrum`: the folder that holds the CLI links' `bin`.
#[must_use]
pub fn ferrum_dir(paths: &Paths) -> Option<&Path> {
    paths.bin_dir.parent()
}

impl State {
    /// What uninstalling `app` would remove.
    #[must_use]
    pub fn removal(&self, app: AppId, paths: &Paths) -> Removal {
        let info = self.info(app);
        let mut versions: Vec<Version> = self
            .app(app)
            .installed
            .iter()
            .map(|installed| installed.version.clone())
            .collect();
        versions.sort_by(|a, b| b.cmp(a));
        Removal {
            versions,
            versions_dir: paths.data_dir.join("apps").join(app.as_str()),
            application: paths.application_path(&info).filter(|path| path.exists()),
            data_dirs: ferrum_dir(paths)
                .map(|dir| data_dirs(dir, &info))
                .unwrap_or_default()
                .into_iter()
                .filter(|dir| dir.is_dir())
                .collect(),
        }
    }

    /// Removes every installed version of `app` and its desktop integration;
    /// with `remove_data`, also its settings and data folders in `~/.ferrum`.
    /// The launcher can't uninstall itself.
    pub fn uninstall_app(&mut self, app: AppId, remove_data: bool, paths: &Paths) -> Result<()> {
        if app == AppId::LAUNCHER {
            return Err(LauncherError::Invalid(
                "the launcher can't uninstall itself".into(),
            ));
        }
        let removal = self.removal(app, paths);
        let active = self.active(app).cloned();
        // The active version last: removing it unlinks the app.
        let order = removal
            .versions
            .iter()
            .filter(|version| Some(*version) != active.as_ref())
            .chain(active.as_ref());
        for version in order {
            self.uninstall(app, version, paths)?;
        }
        if removal.versions_dir.exists() {
            std::fs::remove_dir_all(&removal.versions_dir)?;
        }
        if remove_data {
            for dir in &removal.data_dirs {
                std::fs::remove_dir_all(dir)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Platform;
    use crate::store::InstalledVersion;
    use chrono::Utc;

    fn paths(dir: &Path) -> Paths {
        Paths {
            platform: Platform::MacOs,
            data_dir: dir.join("data"),
            bin_dir: dir.join(".ferrum/bin"),
            applications_dir: Some(dir.join("Applications/Ferrum")),
            xdg_data_dir: None,
            start_menu_dir: None,
        }
    }

    fn install(state: &mut State, paths: &Paths, app: AppId, version: &str) {
        let version = Version::parse(version).expect("semver");
        let dir = paths.version_dir(app, &version);
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join(&state.info(app).binary), b"bin").expect("write");
        state.record_install(
            app,
            InstalledVersion {
                version,
                tag: format!("{app}-v"),
                asset: "a.tar.gz".into(),
                size: 1,
                installed_at: Utc::now(),
                notes: String::new(),
            },
        );
    }

    #[test]
    fn data_dirs_are_the_id_and_binary_folders() {
        let ferrum = Path::new("/home/me/.ferrum");
        let editor = AppInfo::builtin(AppId::EDITOR).expect("editor");
        assert_eq!(
            data_dirs(ferrum, &editor),
            vec![ferrum.join("editor"), ferrum.join("ferrumeditor")]
        );
        // The same name once.
        let generic = AppInfo::generic(AppId::new("forge").expect("id"));
        assert_eq!(data_dirs(ferrum, &generic), vec![ferrum.join("forge")]);
        // Never the shared CLI folder or anything outside ~/.ferrum.
        let mut odd = generic.clone();
        odd.binary = "bin".into();
        assert_eq!(data_dirs(ferrum, &odd), vec![ferrum.join("forge")]);
        odd.binary = "../x".into();
        assert_eq!(data_dirs(ferrum, &odd), vec![ferrum.join("forge")]);
    }

    #[test]
    fn removal_lists_versions_bundle_and_existing_data() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let mut state = State::default();
        install(&mut state, &paths, AppId::API, "0.1.0");
        install(&mut state, &paths, AppId::API, "0.2.0");
        let ferrum = dir.path().join(".ferrum");
        std::fs::create_dir_all(ferrum.join("ferrumapi")).expect("data");

        let removal = state.removal(AppId::API, &paths);
        assert_eq!(
            removal.versions,
            vec![
                Version::parse("0.2.0").expect("semver"),
                Version::parse("0.1.0").expect("semver")
            ]
        );
        assert_eq!(removal.versions_dir, paths.data_dir.join("apps/api"));
        assert_eq!(removal.application, None);
        // `~/.ferrum/api` doesn't exist, so only the binary's folder.
        assert_eq!(removal.data_dirs, vec![ferrum.join("ferrumapi")]);
    }

    #[test]
    fn uninstalls_every_version_and_keeps_data_unless_asked() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let mut state = State::default();
        install(&mut state, &paths, AppId::GIT, "0.1.0");
        install(&mut state, &paths, AppId::GIT, "0.2.0");
        state
            .activate(
                AppId::GIT,
                &Version::parse("0.2.0").expect("semver"),
                &paths,
            )
            .expect("activate");
        let link = paths.link_path(&state.info(AppId::GIT));
        assert!(link.is_symlink());
        let settings = dir.path().join(".ferrum/ferrumgit");
        std::fs::create_dir_all(&settings).expect("settings");
        std::fs::write(settings.join("settings.json"), "{}").expect("write");
        let shared = dir.path().join(".ferrum/appearance.json");
        std::fs::write(&shared, "{}").expect("write");

        state
            .uninstall_app(AppId::GIT, false, &paths)
            .expect("uninstall");
        assert!(state.app(AppId::GIT).installed.is_empty());
        assert_eq!(state.active(AppId::GIT), None);
        assert!(!paths.data_dir.join("apps/git").exists());
        assert!(!link.exists() && !link.is_symlink());
        assert!(settings.join("settings.json").exists(), "data is kept");

        install(&mut state, &paths, AppId::GIT, "0.2.0");
        state
            .uninstall_app(AppId::GIT, true, &paths)
            .expect("uninstall with data");
        assert!(!settings.exists());
        assert!(shared.exists(), "shared files stay");
        assert!(paths.bin_dir.parent().is_some_and(Path::exists));
    }

    #[test]
    fn the_launcher_is_not_uninstalled() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        let mut state = State::default();
        install(&mut state, &paths, AppId::LAUNCHER, "0.9.0");
        assert!(state.uninstall_app(AppId::LAUNCHER, false, &paths).is_err());
        assert_eq!(state.app(AppId::LAUNCHER).installed.len(), 1);
    }
}
