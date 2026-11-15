//! First start of Ferrum Launcher where Ferrum Toolbox ran before: the
//! toolbox's data directory (`state.json`, installed versions, downloads)
//! moves to the launcher's, so installed apps, active versions, channels,
//! `channels_revision` and the cached releases survive. The keychain token
//! is copied separately ([`crate::auth::migrate_token`]).
//!
//! Every step is idempotent and nothing is deleted before it exists at the
//! new place: a rename moves data atomically, and when a rename isn't
//! possible the data is copied and the original kept.

use crate::Result;
use crate::catalog::AppId;
use std::path::{Path, PathBuf};

/// Ferrum Toolbox's data directory: `~/Library/Application Support/
/// dev.Ferrum.FerrumToolbox` on macOS, `~/.local/share/ferrumtoolbox` on
/// Linux, `%LOCALAPPDATA%\Ferrum\FerrumToolbox\data` on Windows.
#[must_use]
pub fn legacy_data_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "Ferrum", "FerrumToolbox")
        .map(|project| project.data_local_dir().to_path_buf())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataMigration {
    /// The launcher has its own state already, or there is nothing to move.
    NotNeeded,
    /// The toolbox's files were renamed into the launcher's directory.
    Moved,
    /// At least part had to be copied; the toolbox's copy was kept.
    Copied,
}

impl DataMigration {
    /// Whether files moved, so links into the old directory need fixing.
    #[must_use]
    pub fn happened(self) -> bool {
        self != DataMigration::NotNeeded
    }
}

const STATE_FILE: &str = "state.json";

/// Moves `legacy` (the toolbox's data directory) to `data_dir` unless
/// `data_dir` already has a `state.json`. `state.json` moves last, so an
/// interrupted run is simply repeated on the next start. On Unix the old
/// path becomes a symlink to the new one, so absolute paths into it (CLI
/// links, desktop entries) keep resolving.
pub fn migrate_data_dir(legacy: &Path, data_dir: &Path) -> Result<DataMigration> {
    let outcome = if data_dir.join(STATE_FILE).is_file() || !legacy.join(STATE_FILE).is_file() {
        DataMigration::NotNeeded
    } else if !data_dir.exists() {
        if let Some(parent) = data_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        match std::fs::rename(legacy, data_dir) {
            Ok(()) => DataMigration::Moved,
            Err(e) => {
                tracing::warn!(error = %e, "could not move the toolbox data; copying it");
                let staging =
                    data_dir.with_extension(format!("migrating-{}", uuid::Uuid::new_v4()));
                if let Err(e) = copy_tree(legacy, &staging) {
                    remove_tree(&staging).ok();
                    return Err(e.into());
                }
                std::fs::rename(&staging, data_dir)?;
                DataMigration::Copied
            }
        }
    } else {
        merge_into(legacy, data_dir)?
    };
    if outcome == DataMigration::Moved {
        link_legacy_path(legacy, data_dir);
    }
    rename_launcher_versions(data_dir)?;
    Ok(outcome)
}

/// Moves each entry of `legacy` that `data_dir` lacks, `state.json` last.
/// Entries the launcher already has are left in `legacy`.
fn merge_into(legacy: &Path, data_dir: &Path) -> Result<DataMigration> {
    let mut names: Vec<_> = std::fs::read_dir(legacy)?
        .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
        .collect();
    names.sort_by_key(|name| name == STATE_FILE);
    let mut copied = false;
    for name in names {
        let (from, to) = (legacy.join(&name), data_dir.join(&name));
        if to.exists() || to.is_symlink() {
            tracing::warn!(path = %to.display(), "already in the launcher's data; keeping both");
            continue;
        }
        if std::fs::rename(&from, &to).is_err() {
            let staging = data_dir.join(format!(".migrating-{}", uuid::Uuid::new_v4()));
            if let Err(e) = copy_tree(&from, &staging) {
                remove_tree(&staging).ok();
                return Err(e.into());
            }
            std::fs::rename(&staging, &to)?;
            copied = true;
        }
    }
    // Only succeeds when everything moved.
    if std::fs::remove_dir(legacy).is_ok() {
        link_legacy_path(legacy, data_dir);
    }
    Ok(if copied {
        DataMigration::Copied
    } else {
        DataMigration::Moved
    })
}

/// Versions of the launcher that Toolbox 0.5.0 installed (the bridge
/// release) live in `apps/toolbox/`; the launcher's are in `apps/launcher/`.
fn rename_launcher_versions(data_dir: &Path) -> Result<()> {
    let apps = data_dir.join("apps");
    let legacy = apps.join(AppId::LEGACY_LAUNCHER);
    if !legacy.is_dir() || legacy.is_symlink() {
        return Ok(());
    }
    let current = apps.join(AppId::LAUNCHER.as_str());
    if !current.exists() {
        std::fs::rename(&legacy, &current)?;
        return Ok(());
    }
    for entry in std::fs::read_dir(&legacy)? {
        let entry = entry?;
        let to = current.join(entry.file_name());
        if !to.exists() {
            std::fs::rename(entry.path(), to)?;
        }
    }
    std::fs::remove_dir(&legacy).ok();
    Ok(())
}

#[cfg(unix)]
fn link_legacy_path(legacy: &Path, data_dir: &Path) {
    if legacy.exists() || legacy.is_symlink() {
        return;
    }
    if let Err(e) = std::os::unix::fs::symlink(data_dir, legacy) {
        tracing::warn!(error = %e, "could not leave a link at the toolbox's data directory");
    }
}

#[cfg(not(unix))]
fn link_legacy_path(_: &Path, _: &Path) {}

/// Copies a file, directory or symlink (links are recreated, not followed).
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    let metadata = std::fs::symlink_metadata(from)?;
    if metadata.file_type().is_symlink() {
        let target = std::fs::read_link(from)?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, to)?;
        #[cfg(not(unix))]
        {
            let resolved = from
                .parent()
                .map_or(target.clone(), |dir| dir.join(&target));
            if resolved.is_dir() {
                copy_tree(&resolved, to)?;
            } else {
                std::fs::copy(&resolved, to)?;
            }
        }
    } else if metadata.is_dir() {
        std::fs::create_dir_all(to)?;
        std::fs::set_permissions(to, metadata.permissions())?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        std::fs::copy(from, to)?;
    }
    Ok(())
}

fn remove_tree(path: &Path) -> std::io::Result<()> {
    if path.is_dir() && !path.is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::AppInfo;
    use crate::platform::Platform;
    use crate::release::UpdateChannel;
    use crate::store::{Paths, State};
    use semver::Version;

    /// `state.json` as Ferrum Toolbox 0.5.0 wrote it.
    const TOOLBOX_STATE: &str = r#"{
      "settings": {"repo_owner": "Amygdal-doo", "repo_name": "ferrum-releases",
                   "check_interval_hours": 4, "keep_versions": 2, "channels_revision": 1},
      "apps": {
        "api": {"installed": [{"version": "0.2.0", "tag": "api-v0.2.0", "asset": "a", "size": 1,
                               "installed_at": "2026-11-01T00:00:00Z", "notes": ""}],
                "active": "0.2.0", "channel": "stable"},
        "ide": {"installed": [{"version": "0.1.0", "tag": "ide-v0.1.0", "asset": "c", "size": 1,
                               "installed_at": "2026-11-01T00:00:00Z", "notes": ""}],
                "active": "0.1.0", "channel": "alpha"},
        "toolbox": {"installed": [{"version": "0.6.0", "tag": "toolbox-v0.6.0", "asset": "b", "size": 1,
                                   "installed_at": "2026-11-02T00:00:00Z", "notes": ""}],
                    "active": null, "channel": "alpha"}
      },
      "last_check": "2026-11-02T00:00:00Z",
      "releases": [
        {"app": "toolbox", "version": "0.6.0", "tag": "toolbox-v0.6.0", "title": "", "notes": "",
         "prerelease": false, "published_at": null, "html_url": "", "assets": []},
        {"app": "api", "version": "0.2.0", "tag": "api-v0.2.0", "title": "", "notes": "",
         "prerelease": false, "published_at": null, "html_url": "", "assets": []},
        {"app": "ide", "version": "0.2.0", "tag": "ide-v0.2.0", "title": "", "notes": "",
         "prerelease": false, "published_at": null, "html_url": "", "assets": []}
      ],
      "catalog": {"apps": [{"id": "toolbox", "status": "alpha", "summary": "Old", "highlights": [],
                            "roadmap": [], "homepage": null},
                           {"id": "ide", "status": "alpha", "summary": "Old", "highlights": [],
                            "roadmap": [], "homepage": null}]}
    }"#;

    fn paths(root: &Path) -> Paths {
        Paths {
            platform: Platform::MacOs,
            data_dir: root.join("Launcher"),
            bin_dir: root.join("bin"),
            applications_dir: Some(root.join("Applications/Ferrum")),
            xdg_data_dir: None,
            start_menu_dir: None,
        }
    }

    /// A toolbox data dir with state, an installed FerrumAPI and the
    /// launcher 0.6.0 it installed from the bridge release.
    fn seed_toolbox(legacy: &Path) {
        std::fs::create_dir_all(legacy.join("apps/api/0.2.0")).expect("mkdir");
        std::fs::write(legacy.join("apps/api/0.2.0/ferrumapi"), b"bin").expect("bin");
        std::fs::create_dir_all(legacy.join("apps/toolbox/0.6.0/Ferrum Launcher.app"))
            .expect("mkdir");
        std::fs::create_dir_all(legacy.join("downloads")).expect("mkdir");
        std::fs::write(legacy.join(STATE_FILE), TOOLBOX_STATE).expect("state");
    }

    #[test]
    fn moves_toolbox_data_and_state_reads_as_the_launcher() {
        let dir = tempfile::tempdir().expect("tmp");
        let legacy = dir.path().join("Toolbox");
        let paths = paths(dir.path());
        seed_toolbox(&legacy);

        let outcome = migrate_data_dir(&legacy, &paths.data_dir).expect("migrate");
        assert_eq!(outcome, DataMigration::Moved);
        assert!(paths.state_file().is_file());
        assert!(paths.data_dir.join("apps/api/0.2.0/ferrumapi").is_file());
        assert!(
            paths
                .data_dir
                .join("apps/launcher/0.6.0/Ferrum Launcher.app")
                .is_dir()
        );
        assert!(!paths.data_dir.join("apps/toolbox").exists());
        #[cfg(unix)]
        assert_eq!(std::fs::read_link(&legacy).expect("link"), paths.data_dir);

        let mut state = State::load(&paths).expect("load");
        assert_eq!(state.settings.check_interval_hours, 4);
        assert_eq!(state.settings.keep_versions, 2);
        assert_eq!(state.settings.channels_revision, 1);
        assert_eq!(state.channel(AppId::API), UpdateChannel::Stable);
        assert_eq!(state.active(AppId::API), Some(&Version::new(0, 2, 0)));
        let launcher = state.app(AppId::LAUNCHER);
        assert_eq!(launcher.installed[0].version, Version::new(0, 6, 0));
        assert!(
            paths
                .bundle_path(AppId::LAUNCHER, &Version::new(0, 6, 0))
                .is_some()
        );
        assert!(state.releases.iter().any(|r| r.app == AppId::LAUNCHER));
        assert_eq!(
            state.info(AppId::LAUNCHER).bundle_name,
            "FerrumLauncher",
            "the cached 0.5.0 catalog doesn't break the new metadata"
        );
        // Ferrum IDE was dropped: the installed copy stays launchable and
        // removable as "No longer offered", without updates.
        let ide = AppId::new("ide").expect("id");
        assert!(state.managed_apps().iter().all(|info| info.id != ide));
        assert_eq!(
            state
                .discontinued_apps()
                .iter()
                .map(|info| info.id)
                .collect::<Vec<_>>(),
            vec![ide]
        );
        assert_eq!(state.info(ide).binary, "zed");
        assert_eq!(state.active(ide), Some(&Version::new(0, 1, 0)));
        assert!(state.update_for(ide).is_none());
        assert!(state.updates().iter().all(|(app, _)| *app != ide));
        // Toolbox 0.5.0 knew five apps: FerrumIDE, FerrumEngineer, FerrumKit, FerrumContainers
        // and the apps added after them are new.
        assert_eq!(
            state.discover_apps(),
            vec![
                AppId::EDITOR,
                AppId::VIBE,
                AppId::DOCK,
                AppId::ENV,
                AppId::PORTS,
                AppId::KIT,
                AppId::SYSTEM
            ]
        );
        assert!(state.is_new(AppId::DOCK));
        state.save(&paths).expect("save");
        let saved = std::fs::read_to_string(paths.state_file()).expect("read");
        assert!(!saved.contains("\"toolbox\""), "{saved}");

        // Idempotent: the second start finds the launcher's own state.
        assert_eq!(
            migrate_data_dir(&legacy, &paths.data_dir).expect("again"),
            DataMigration::NotNeeded
        );
        let mut reloaded = State::load(&paths).expect("reload");
        assert!(reloaded.discover_apps().is_empty());
        assert!(reloaded.is_new(AppId::KIT), "seen-state persists");
        assert!(reloaded.is_new(AppId::EDITOR), "seen-state persists");
    }

    #[test]
    fn existing_launcher_state_is_never_replaced() {
        let dir = tempfile::tempdir().expect("tmp");
        let legacy = dir.path().join("Toolbox");
        let paths = paths(dir.path());
        seed_toolbox(&legacy);
        std::fs::create_dir_all(&paths.data_dir).expect("mkdir");
        std::fs::write(paths.state_file(), r#"{"settings":{"keep_versions":9}}"#).expect("state");

        assert_eq!(
            migrate_data_dir(&legacy, &paths.data_dir).expect("migrate"),
            DataMigration::NotNeeded
        );
        assert_eq!(State::load(&paths).expect("load").settings.keep_versions, 9);
        assert!(
            legacy.join(STATE_FILE).is_file(),
            "the toolbox's data is untouched"
        );
        assert!(legacy.join("apps/api/0.2.0/ferrumapi").is_file());
    }

    #[test]
    fn merges_into_a_launcher_dir_without_state_and_keeps_conflicts() {
        let dir = tempfile::tempdir().expect("tmp");
        let legacy = dir.path().join("Toolbox");
        let paths = paths(dir.path());
        seed_toolbox(&legacy);
        // The launcher cached a logo before the migration ran.
        std::fs::create_dir_all(paths.data_dir.join("downloads")).expect("mkdir");
        std::fs::create_dir_all(paths.data_dir.join("logos")).expect("mkdir");

        let outcome = migrate_data_dir(&legacy, &paths.data_dir).expect("migrate");
        assert!(outcome.happened());
        assert!(paths.state_file().is_file());
        assert!(paths.data_dir.join("apps/api/0.2.0/ferrumapi").is_file());
        assert!(paths.data_dir.join("apps/launcher/0.6.0").is_dir());
        // `downloads` existed in both, so the toolbox's copy stays.
        assert!(legacy.join("downloads").is_dir());
        assert!(!legacy.join(STATE_FILE).exists());
    }

    #[test]
    fn copies_trees_with_links_and_modes() {
        let dir = tempfile::tempdir().expect("tmp");
        let from = dir.path().join("from");
        std::fs::create_dir_all(from.join("App.app/Contents/MacOS")).expect("mkdir");
        let binary = from.join("App.app/Contents/MacOS/app");
        std::fs::write(&binary, b"bin").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
            std::os::unix::fs::symlink("App.app/Contents/MacOS/app", from.join("app"))
                .expect("link");
        }
        let to = dir.path().join("to");
        copy_tree(&from, &to).expect("copy");
        assert_eq!(
            std::fs::read(to.join("App.app/Contents/MacOS/app")).expect("read"),
            b"bin"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(to.join("App.app/Contents/MacOS/app"))
                .expect("meta")
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111);
            assert_eq!(
                std::fs::read_link(to.join("app")).expect("link"),
                Path::new("App.app/Contents/MacOS/app")
            );
        }
        assert!(binary.is_file(), "the source is kept");
    }

    #[cfg(unix)]
    #[test]
    fn relinks_cli_entries_into_the_new_data_dir() {
        let dir = tempfile::tempdir().expect("tmp");
        let legacy = dir.path().join("Toolbox");
        let paths = paths(dir.path());
        seed_toolbox(&legacy);
        // Toolbox 0.5.0 linked ~/.ferrum/bin/ferrumapi into its own directory.
        std::fs::create_dir_all(&paths.bin_dir).expect("mkdir");
        std::os::unix::fs::symlink(
            legacy.join("apps/api/0.2.0/ferrumapi"),
            paths.bin_dir.join("ferrumapi"),
        )
        .expect("link");

        migrate_data_dir(&legacy, &paths.data_dir).expect("migrate");
        let state = State::load(&paths).expect("load");
        assert!(state.relink_active(&paths).is_empty());
        let api = AppInfo::builtin(AppId::API).expect("api");
        assert_eq!(
            std::fs::read_link(paths.link_path(&api)).expect("link"),
            paths.data_dir.join("apps/api/0.2.0/ferrumapi")
        );
    }
}
