//! Background updates: a newer version is downloaded and installed beside
//! the active one (checksum verified as for any install), then *staged*
//! until the app isn't running, because switching versions replaces the
//! app the user has open (`~/Applications/Ferrum/<App>.app` on macOS).
//! The launcher switches a staged app as soon as it finds it not running,
//! or after the user confirmed quitting it ("Restart to update").
//!
//! Whether an app runs is read from the process list (`ps` on macOS and
//! Linux, `tasklist` on Windows) by its main executable's name; quitting
//! asks the app to quit the way the OS's own Quit does (Apple Events on
//! macOS, `SIGTERM` on Linux, `WM_CLOSE` through `taskkill` without `/F`
//! on Windows), so it can still ask about unsaved work.

use std::process::{Command, Stdio};

use semver::Version;

use crate::catalog::{AppId, AppInfo};
use crate::platform::Platform;
use crate::store::State;
use crate::{LauncherError, Result};

impl State {
    /// The update downloaded in the background for `app` and not yet
    /// active: still installed and newer than the active version.
    #[must_use]
    pub fn staged_update(&self, app: AppId) -> Option<Version> {
        let state = self.apps.get(&app)?;
        let staged = state.staged.as_ref()?;
        let newer = state.active.as_ref().is_none_or(|active| staged > active);
        (newer && state.is_installed(staged)).then(|| staged.clone())
    }

    /// Records `version` (installed, not active) as waiting for `app` to
    /// quit.
    pub fn stage_update(&mut self, app: AppId, version: Version) {
        self.apps.entry(app).or_default().staged = Some(version);
    }

    /// The apps whose update [`State::app_updates`] offers and that a
    /// background check should download: not staged at that version yet.
    #[must_use]
    pub fn background_downloads(&self) -> Vec<(AppId, Version)> {
        self.app_updates()
            .into_iter()
            .filter(|(app, version)| self.staged_update(*app).as_ref() != Some(version))
            .collect()
    }
}

/// Process ids of `binary` (the app's main executable) in `ps -axo
/// pid=,comm=` output. macOS prints the executable's path; Linux its name,
/// cut to 15 bytes.
#[must_use]
pub fn pids_in_ps(output: &str, binary: &str, own_pid: u32) -> Vec<u32> {
    output
        .lines()
        .filter_map(|line| {
            let (pid, command) = line.trim_start().split_once(char::is_whitespace)?;
            let pid: u32 = pid.parse().ok()?;
            let name = command.trim().rsplit('/').next()?;
            let matches = name == binary
                || (name.len() == 15 && binary.len() > 15 && binary.starts_with(name));
            (matches && pid != own_pid).then_some(pid)
        })
        .collect()
}

/// Process ids in `tasklist /FO CSV /NH` output for `image` (`x.exe`).
#[must_use]
pub fn pids_in_tasklist(output: &str, image: &str) -> Vec<u32> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split("\",\"").map(|f| f.trim_matches('"'));
            let name = fields.next()?;
            let pid = fields.next()?.parse().ok()?;
            name.eq_ignore_ascii_case(image).then_some(pid)
        })
        .collect()
}

/// The running processes of `info`'s app on this machine. Blocks briefly.
pub fn running_pids(platform: Platform, info: &AppInfo) -> Result<Vec<u32>> {
    let own = std::process::id();
    match platform {
        Platform::Windows => {
            let image = platform.executable(&info.binary);
            let output = quiet(Command::new("tasklist").args([
                "/FO",
                "CSV",
                "/NH",
                "/FI",
                &format!("IMAGENAME eq {image}"),
            ]))?;
            Ok(pids_in_tasklist(&output, &image)
                .into_iter()
                .filter(|pid| *pid != own)
                .collect())
        }
        _ => {
            let output = quiet(Command::new("ps").args(["-axo", "pid=,comm="]))?;
            Ok(pids_in_ps(&output, &info.binary, own))
        }
    }
}

/// Asks `info`'s app to quit (its processes `pids`); it may still ask the
/// user about unsaved work, so the caller polls [`running_pids`].
pub fn request_quit(platform: Platform, info: &AppInfo, pids: &[u32]) -> Result<()> {
    match platform {
        Platform::MacOs => {
            let script = format!(
                "tell application \"{}\" to quit",
                info.bundle_name.replace('"', "")
            );
            if quiet(Command::new("osascript").args(["-e", &script])).is_ok() {
                return Ok(());
            }
            signal_term(pids)
        }
        Platform::Windows => {
            for pid in pids {
                quiet(Command::new("taskkill").args(["/PID", &pid.to_string()]))?;
            }
            Ok(())
        }
        Platform::Linux | Platform::Other => signal_term(pids),
    }
}

fn signal_term(pids: &[u32]) -> Result<()> {
    if pids.is_empty() {
        return Ok(());
    }
    let mut command = Command::new("kill");
    command.arg("-TERM").args(pids.iter().map(u32::to_string));
    quiet(&mut command).map(|_| ())
}

/// Runs `command` without a console or input; its stdout on success.
fn quiet(command: &mut Command) -> Result<String> {
    let output = command
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Err(LauncherError::Invalid(format!(
            "{command:?} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{InstalledVersion, Paths};

    fn v(text: &str) -> Version {
        Version::parse(text).expect("version")
    }

    fn installed(version: &str) -> InstalledVersion {
        InstalledVersion {
            version: v(version),
            tag: format!("git-v{version}"),
            asset: "git.tar.gz".into(),
            size: 1,
            installed_at: chrono::Utc::now(),
            notes: String::new(),
        }
    }

    #[test]
    fn finds_an_apps_processes_by_executable_name() {
        let macos = "  101 /Users/me/.ferrum/launcher/apps/git/0.3.0/FerrumGIT.app/Contents/MacOS/ferrumgit\n\
                     \x20 102 /usr/bin/ferrumgit-helper\n\
                     \x20 103 ferrumgit\n\
                     \x20 104 /Applications/Ferrum/FerrumGIT.app/Contents/MacOS/ferrumgit\n";
        assert_eq!(pids_in_ps(macos, "ferrumgit", 104), [101, 103]);
        // Linux cuts `comm` to 15 bytes.
        let linux = "  7 ferrumcontainer\n  8 bash\n";
        assert_eq!(pids_in_ps(linux, "ferrumcontainers", 1), [7]);
        assert!(pids_in_ps(linux, "ferrumcontai", 1).is_empty());
        assert!(pids_in_ps("garbage\n\n", "x", 1).is_empty());

        let tasklist = "\"FerrumGIT.exe\",\"4242\",\"Console\",\"1\",\"80,000 K\"\r\n\
                        \"ferrumgit.exe\",\"4343\",\"Console\",\"1\",\"80,000 K\"\r\n\
                        INFO: No tasks are running which match the specified criteria.\r\n";
        assert_eq!(pids_in_tasklist(tasklist, "ferrumgit.exe"), [4242, 4343]);
    }

    #[test]
    fn a_staged_update_waits_survives_pruning_and_ends_on_activation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = Paths {
            platform: Platform::Linux,
            data_dir: dir.path().join("data"),
            bin_dir: dir.path().join("bin"),
            applications_dir: None,
            xdg_data_dir: None,
            start_menu_dir: None,
        };
        let mut state = State::default();
        state.settings.keep_versions = 1;
        state.record_install(AppId::GIT, installed("0.2.0"));
        state.record_install(AppId::GIT, installed("0.3.0"));
        for version in ["0.2.0", "0.3.0"] {
            std::fs::create_dir_all(paths.version_dir(AppId::GIT, &v(version))).expect("dir");
        }
        assert_eq!(state.active(AppId::GIT), Some(&v("0.2.0")));
        assert_eq!(state.staged_update(AppId::GIT), None);
        // Without staging, keeping one version removes the newer one.
        assert_eq!(state.prune_candidates(AppId::GIT), [v("0.3.0")]);

        state.stage_update(AppId::GIT, v("0.3.0"));
        assert_eq!(state.staged_update(AppId::GIT), Some(v("0.3.0")));
        assert!(state.prune_candidates(AppId::GIT).is_empty());

        state
            .activate(AppId::GIT, &v("0.3.0"), &paths)
            .expect("activate");
        assert_eq!(state.staged_update(AppId::GIT), None);
        assert_eq!(state.app(AppId::GIT).staged, None);
        assert_eq!(state.prune_candidates(AppId::GIT), [v("0.2.0")]);

        // A staged version that was removed is no longer offered.
        state.stage_update(AppId::GIT, v("0.4.0"));
        assert_eq!(state.staged_update(AppId::GIT), None);
    }
}
