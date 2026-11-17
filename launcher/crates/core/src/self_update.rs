//! Replacing the running launcher with a newer version it just installed.
//!
//! A running bundle can't overwrite itself, so the swap is done by a small
//! detached shell script that waits for this process to exit, moves the new
//! bundle into place (keeping the old one until the move succeeded) and opens
//! it again. Only macOS bundles are swapped.
//!
//! On Windows and Linux a launcher installed from its installer updates
//! through the new release's installer instead: [`download_installer`]
//! fetches it (checksum verified when a `.sha256` is published) and
//! [`open_installer`] starts it: the per-user Inno Setup wizard on Windows
//! (it closes the running launcher itself, `CloseApplications=yes`), the
//! desktop's package installer for the `.deb` on Linux. Copies run from an
//! archive (or `cargo run`) fall back to the release page.

use crate::github::{DownloadProgress, GitHubClient};
use crate::platform::Platform;
use crate::release::ReleaseInfo;
use crate::store::Paths;
use crate::{LauncherError, Result, install};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

/// Whether the launcher at `executable` was put there by its installer, so
/// the next installer replaces it: the `.deb`'s `/opt/ferrum/launcher/` on
/// Linux, Inno Setup's `%LOCALAPPDATA%\Programs\Ferrum\` on Windows.
#[must_use]
pub fn installed_by_installer(
    platform: Platform,
    executable: &Path,
    local_app_data: Option<&Path>,
) -> bool {
    match platform {
        Platform::Linux => executable.starts_with("/opt/ferrum/launcher"),
        Platform::Windows => local_app_data
            .is_some_and(|base| executable.starts_with(base.join("Programs").join("Ferrum"))),
        Platform::MacOs | Platform::Other => false,
    }
}

/// [`installed_by_installer`] for this process.
#[must_use]
pub fn running_from_installer() -> bool {
    let Ok(executable) = std::env::current_exe() else {
        return false;
    };
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    installed_by_installer(Platform::current(), &executable, local.as_deref())
}

/// Downloads `release`'s installer for `platform` into the downloads
/// folder under its own name and verifies it. Returns its path.
pub async fn download_installer(
    client: &GitHubClient,
    paths: &Paths,
    release: &ReleaseInfo,
    platform: Platform,
    progress: DownloadProgress,
    cancel: CancellationToken,
) -> Result<PathBuf> {
    let (installer, checksum) = release.launcher_installer(platform).ok_or_else(|| {
        LauncherError::Invalid(format!(
            "launcher {} has no installer for this platform",
            release.version
        ))
    })?;
    let destination = paths.downloads_dir().join(&installer.name);
    let actual = client
        .download_to(installer, &destination, progress, cancel)
        .await?;
    let verified = match checksum {
        Some(checksum) => match client.download_text(checksum).await {
            Ok(text) => install::verify_checksum(&installer.name, &text, &actual),
            Err(e) => Err(e),
        },
        None => {
            tracing::warn!(asset = %installer.name, "installer has no .sha256; not verified");
            Ok(())
        }
    };
    if let Err(e) = verified {
        tokio::fs::remove_file(&destination).await.ok();
        return Err(e);
    }
    Ok(destination)
}

/// Starts the downloaded installer detached. On Windows the caller should
/// quit right after, so the setup can replace the launcher's files.
pub fn open_installer(platform: Platform, installer: &Path) -> Result<()> {
    let mut command = match platform {
        // `/SP-` skips "This will install…"; the wizard still shows.
        Platform::Windows => {
            let mut command = std::process::Command::new(installer);
            command.arg("/SP-");
            command
        }
        Platform::Linux => {
            let mut command = std::process::Command::new("xdg-open");
            command.arg(installer);
            command
        }
        Platform::MacOs | Platform::Other => {
            return Err(LauncherError::Invalid(
                "installers are only used on Windows and Linux".into(),
            ));
        }
    };
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command.spawn()?;
    Ok(())
}

/// The `.app` bundle the current executable runs from, if any. `cargo run`
/// builds aren't bundled and return `None`.
#[must_use]
pub fn running_bundle() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?.canonicalize().ok()?;
    bundle_containing(&executable)
}

pub(crate) fn bundle_containing(executable: &Path) -> Option<PathBuf> {
    executable
        .ancestors()
        .skip(1)
        .find(|dir| dir.extension().is_some_and(|ext| ext == "app"))
        .map(Path::to_path_buf)
}

/// Whether the folder holding `bundle` accepts a replacement without admin
/// rights (an admin user's `/Applications` does; a standard user's doesn't).
#[must_use]
pub fn can_replace(bundle: &Path) -> bool {
    let Some(parent) = bundle.parent() else {
        return false;
    };
    let probe = parent.join(format!(".ferrum-launcher-probe-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            if let Err(e) = std::fs::remove_file(&probe) {
                tracing::warn!(error = %e, path = %probe.display(), "could not remove write probe");
            }
            true
        }
        Err(_) => false,
    }
}

/// The swap script for `new_bundle` → `target`, run after `pid` exits.
fn swap_script(new_bundle: &Path, target: &Path, pid: u32) -> String {
    let quote = |path: &Path| shell_quote(&path.display().to_string());
    let staged = target.with_extension(format!("app.new-{pid}"));
    let backup = target.with_extension(format!("app.old-{pid}"));
    format!(
        r#"set -u
while kill -0 {pid} 2>/dev/null; do sleep 0.2; done
rm -rf {staged}
ditto {new} {staged} || {{ open {target}; exit 1; }}
mv {target} {backup} || {{ rm -rf {staged}; open {target}; exit 1; }}
if mv {staged} {target}; then
  rm -rf {backup}
else
  mv {backup} {target}
fi
open {target}
"#,
        new = quote(new_bundle),
        staged = quote(&staged),
        target = quote(target),
        backup = quote(&backup),
    )
}

pub(crate) fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Starts the detached swap of `target` (the running bundle) for
/// `new_bundle`. The caller must quit right after so the script can proceed.
pub fn replace_and_relaunch(new_bundle: &Path, target: &Path) -> Result<()> {
    if !new_bundle.is_dir() {
        return Err(LauncherError::Invalid(format!(
            "the new launcher bundle is missing: {}",
            new_bundle.display()
        )));
    }
    if !can_replace(target) {
        return Err(LauncherError::Invalid(format!(
            "{} is not writable, so the launcher can't update itself there",
            target.parent().unwrap_or(target).display()
        )));
    }
    let mut command = std::process::Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(swap_script(new_bundle, target, std::process::id()))
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

    #[test]
    fn finds_the_enclosing_bundle() {
        let exe = Path::new("/Applications/FerrumLauncher.app/Contents/MacOS/ferrum-launcher");
        assert_eq!(
            bundle_containing(exe),
            Some(PathBuf::from("/Applications/FerrumLauncher.app"))
        );
        assert_eq!(
            bundle_containing(Path::new("/repo/target/release/ferrum-launcher")),
            None
        );
    }

    #[test]
    fn only_installer_copies_update_through_the_installer() {
        let local = Path::new("C:/Users/me/AppData/Local");
        assert!(installed_by_installer(
            Platform::Windows,
            &local.join("Programs/Ferrum/FerrumLauncher/ferrum-launcher.exe"),
            Some(local),
        ));
        assert!(!installed_by_installer(
            Platform::Windows,
            Path::new("C:/Users/me/Downloads/launcher/ferrum-launcher.exe"),
            Some(local),
        ));
        assert!(!installed_by_installer(
            Platform::Windows,
            Path::new("C:/x/ferrum-launcher.exe"),
            None
        ));
        assert!(installed_by_installer(
            Platform::Linux,
            Path::new("/opt/ferrum/launcher/ferrum-launcher"),
            None
        ));
        assert!(!installed_by_installer(
            Platform::Linux,
            Path::new("/home/me/ferrum/ferrum-launcher"),
            None
        ));
        assert!(!installed_by_installer(
            Platform::MacOs,
            Path::new("/opt/ferrum/launcher/ferrum-launcher"),
            None
        ));
    }

    #[test]
    fn quotes_paths_with_spaces_and_quotes() {
        assert_eq!(shell_quote("/a b/it's.app"), r"'/a b/it'\''s.app'");
    }

    #[test]
    fn script_waits_then_swaps_with_rollback() {
        let script = swap_script(
            Path::new("/data/launcher/0.7.0/FerrumLauncher.app"),
            Path::new("/Applications/FerrumLauncher.app"),
            42,
        );
        let wait = script.find("kill -0 42").expect("waits for the pid");
        let copy = script.find("ditto").expect("copies the new bundle");
        assert!(wait < copy);
        assert!(script.contains("'/Applications/FerrumLauncher.app.old-42'"));
        assert!(script.contains(
            "mv '/Applications/FerrumLauncher.app.old-42' '/Applications/FerrumLauncher.app'"
        ));
        assert!(
            script
                .trim_end()
                .ends_with("open '/Applications/FerrumLauncher.app'")
        );
    }

    #[test]
    fn swaps_a_bundle_end_to_end() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("Old.app");
        let new = dir.path().join("src/New.app");
        std::fs::create_dir_all(target.join("Contents")).expect("old");
        std::fs::write(target.join("Contents/version"), "old").expect("old file");
        std::fs::create_dir_all(new.join("Contents")).expect("new");
        std::fs::write(new.join("Contents/version"), "new").expect("new file");
        assert!(can_replace(&target));

        // A pid that has already exited, so the script doesn't wait.
        let exited = std::process::Command::new("true")
            .spawn()
            .and_then(|mut child| child.wait().map(|_| child.id()))
            .expect("spawn true");
        let script = swap_script(&new, &target, exited).replace("open ", ": ");
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .status()
            .expect("run script");
        assert!(status.success());
        assert_eq!(
            std::fs::read_to_string(target.join("Contents/version")).expect("read"),
            "new"
        );
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .expect("list")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".app."))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
}
