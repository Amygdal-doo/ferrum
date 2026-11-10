//! The launcher started under its old name: from `Ferrum Toolbox.app` (after
//! Toolbox 0.5.0 updated itself through the `toolbox-v*` bridge release) or
//! as a binary named `ferrum-toolbox`. It renames itself to Ferrum Launcher
//! in the same folder, fixes the CLI link and desktop entry, and (for a
//! bundle) relaunches from the new path.
//!
//! A running bundle is renamed by the same kind of detached script as the
//! self-update swap: it waits for this process to exit, moves the bundle
//! (keeping any existing `Ferrum Launcher.app` until the move succeeded) and
//! opens it. When the move fails it reopens the old bundle with
//! [`RENAME_FAILED_ARG`], so the launcher doesn't try again in a loop.

use crate::catalog::AppInfo;
use crate::platform::{self, Platform};
use crate::self_update::{bundle_containing, can_replace, shell_quote};
use crate::store::Paths;
use crate::{LauncherError, Result};
use std::path::{Path, PathBuf};

/// Bundle name of Ferrum Toolbox 0.5.0 and older (without `.app`).
pub const LEGACY_BUNDLE_NAME: &str = "Ferrum Toolbox";
/// Binary name of Ferrum Toolbox 0.5.0 and older.
pub const LEGACY_BINARY: &str = "ferrum-toolbox";
/// Passed to a relaunch after a failed rename.
pub const RENAME_FAILED_ARG: &str = "--ferrum-rename-failed";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rename {
    /// `…/Ferrum Toolbox.app` → `…/Ferrum Launcher.app`.
    Bundle { from: PathBuf, to: PathBuf },
    /// `…/ferrum-toolbox` → `…/ferrum-launcher` (not in a bundle).
    Binary { from: PathBuf, to: PathBuf },
}

impl Rename {
    /// The folder that must be writable.
    #[must_use]
    pub fn folder(&self) -> &Path {
        let (Rename::Bundle { from, .. } | Rename::Binary { from, .. }) = self;
        from.parent().unwrap_or(from)
    }

    /// The old name, for messages.
    #[must_use]
    pub fn old_name(&self) -> String {
        let (Rename::Bundle { from, .. } | Rename::Binary { from, .. }) = self;
        from.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// The new name, for messages.
    #[must_use]
    pub fn new_name(&self) -> String {
        let (Rename::Bundle { to, .. } | Rename::Binary { to, .. }) = self;
        to.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Where the launcher's executable is once the rename is done.
    #[must_use]
    pub fn executable_after(&self, info: &AppInfo) -> PathBuf {
        match self {
            Rename::Bundle { to, .. } => to.join("Contents").join("MacOS").join(&info.binary),
            Rename::Binary { to, .. } => to.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Not running under the old name.
    Nothing,
    /// The rename script runs once this process exits: quit now.
    Relaunch,
    /// A plain binary was renamed in place; keep running.
    Renamed,
    /// The rename isn't possible; keep running and show this one-line notice.
    Blocked(String),
}

/// Renames the running launcher if it still has Ferrum Toolbox's name:
/// fixes the CLI links and desktop entry, then either starts the bundle
/// rename (the caller must exit on [`Outcome::Relaunch`]) or renames a plain
/// binary in place. `rename_failed` is set when a previous attempt
/// relaunched with [`RENAME_FAILED_ARG`].
pub fn run(paths: &Paths, info: &AppInfo, rename_failed: bool) -> Outcome {
    let Some(executable) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.canonicalize().ok())
    else {
        return Outcome::Nothing;
    };
    let Some(rename) = plan(&executable, paths.platform, info) else {
        return Outcome::Nothing;
    };
    let blocked = |reason: String, executable: &Path| {
        fix_links(paths, info, executable);
        Outcome::Blocked(format!(
            "Couldn't rename {} to {}: {reason}.",
            rename.old_name(),
            rename.new_name()
        ))
    };
    if rename_failed {
        return blocked("moving it failed".into(), &executable);
    }
    if !can_rename(&rename) {
        return blocked(
            format!("{} isn't writable", rename.folder().display()),
            &executable,
        );
    }
    let result = match &rename {
        Rename::Bundle { from, to } => rename_bundle_and_relaunch(from, to).map(|()| Outcome::Relaunch),
        Rename::Binary { from, to } => rename_binary(from, to).map(|()| Outcome::Renamed),
    };
    match result {
        Ok(outcome) => {
            tracing::info!(from = %rename.old_name(), to = %rename.new_name(), "renaming Ferrum Toolbox to Ferrum Launcher");
            fix_links(paths, info, &rename.executable_after(info));
            outcome
        }
        Err(e) => blocked(e.to_string(), &executable),
    }
}

/// CLI links and the Linux desktop entry; failures are logged, not fatal.
fn fix_links(paths: &Paths, info: &AppInfo, executable: &Path) {
    if let Err(e) = fix_cli_links(&paths.bin_dir, paths.platform, info, executable) {
        tracing::warn!(error = %e, "could not update the ferrum-toolbox CLI link");
    }
    if let Some(xdg) = &paths.xdg_data_dir
        && let Err(e) = fix_desktop_entry(xdg, info, executable)
    {
        tracing::warn!(error = %e, "could not update the ferrum-toolbox desktop entry");
    }
}

/// What needs renaming when `executable` (canonical path of the running
/// binary) belongs to the old name; `None` otherwise.
#[must_use]
pub fn plan(executable: &Path, platform: Platform, info: &AppInfo) -> Option<Rename> {
    if let Some(bundle) = bundle_containing(executable) {
        let legacy = format!("{LEGACY_BUNDLE_NAME}.app");
        return (bundle.file_name()? == legacy.as_str()).then(|| Rename::Bundle {
            to: bundle.with_file_name(format!("{}.app", info.bundle_name)),
            from: bundle,
        });
    }
    let stem = executable.file_stem()?;
    (stem == LEGACY_BINARY).then(|| Rename::Binary {
        from: executable.to_path_buf(),
        to: executable.with_file_name(platform.executable(&info.binary)),
    })
}

/// Whether the rename can happen without admin rights.
#[must_use]
pub fn can_rename(rename: &Rename) -> bool {
    let (Rename::Bundle { from, .. } | Rename::Binary { from, .. }) = rename;
    can_replace(from)
}

/// The script that renames `from` to `to` once `pid` exited, then opens it.
fn rename_script(from: &Path, to: &Path, pid: u32) -> String {
    let quote = |path: &Path| shell_quote(&path.display().to_string());
    let backup = to.with_extension(format!("app.old-{pid}"));
    format!(
        r#"set -u
while kill -0 {pid} 2>/dev/null; do sleep 0.2; done
if [ -e {to} ]; then
  mv {to} {backup} || {{ open {from} --args {failed}; exit 1; }}
fi
if mv {from} {to}; then
  rm -rf {backup}
  open {to}
else
  if [ -e {backup} ]; then mv {backup} {to}; fi
  open {from} --args {failed}
  exit 1
fi
"#,
        from = quote(from),
        to = quote(to),
        backup = quote(&backup),
        failed = RENAME_FAILED_ARG,
    )
}

/// Starts the detached rename of the running bundle. The caller must quit
/// right after so the script can proceed.
pub fn rename_bundle_and_relaunch(from: &Path, to: &Path) -> Result<()> {
    if !can_replace(from) {
        return Err(LauncherError::Invalid(format!(
            "{} is not writable",
            from.parent().unwrap_or(from).display()
        )));
    }
    let mut command = std::process::Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(rename_script(from, to, std::process::id()))
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

/// Renames a plain `ferrum-toolbox` binary (the running one is fine: the
/// process keeps its file). On Unix the old name stays as a link to the new
/// one for this release.
pub fn rename_binary(from: &Path, to: &Path) -> Result<()> {
    std::fs::rename(from, to)?;
    #[cfg(unix)]
    if let Some(name) = to.file_name()
        && let Err(e) = std::os::unix::fs::symlink(name, from)
    {
        tracing::warn!(error = %e, "could not keep the ferrum-toolbox name as a link");
    }
    Ok(())
}

/// Points `<bin_dir>/ferrum-launcher` and, for one release,
/// `<bin_dir>/ferrum-toolbox` at `executable` when Ferrum Toolbox had a CLI
/// entry there. Returns whether anything changed.
pub fn fix_cli_links(
    bin_dir: &Path,
    platform: Platform,
    info: &AppInfo,
    executable: &Path,
) -> Result<bool> {
    let legacy = bin_dir.join(platform.cli_entry(LEGACY_BINARY));
    if !legacy.exists() && !legacy.is_symlink() {
        return Ok(false);
    }
    for entry in [legacy, bin_dir.join(platform.cli_entry(&info.binary))] {
        if platform == Platform::Windows {
            replace_file(&entry, platform::cmd_shim(executable).as_bytes())?;
        } else {
            replace_symlink(executable, &entry)?;
        }
    }
    Ok(true)
}

/// Linux: replaces a per-user `ferrum-toolbox.desktop` (and its icon) with
/// the launcher's. Returns whether there was one.
pub fn fix_desktop_entry(xdg_data_dir: &Path, info: &AppInfo, executable: &Path) -> Result<bool> {
    let applications = xdg_data_dir.join("applications");
    let icons = xdg_data_dir.join("icons/hicolor/256x256/apps");
    let legacy = applications.join(format!("{LEGACY_BINARY}.desktop"));
    if !legacy.is_file() {
        return Ok(false);
    }
    if let Some(png) = platform::icon_png(info.id) {
        replace_file(
            &icons.join(format!("{}.png", platform::icon_name(info.id))),
            png,
        )?;
    }
    replace_file(
        &applications.join(platform::desktop_entry_name(info.id)),
        platform::desktop_entry(info, executable).as_bytes(),
    )?;
    std::fs::remove_file(&legacy)?;
    let legacy_icon = icons.join(format!("{LEGACY_BINARY}.png"));
    if legacy_icon.is_file() {
        std::fs::remove_file(legacy_icon)?;
    }
    Ok(true)
}

fn replace_file(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    std::fs::write(&temp, contents)?;
    std::fs::rename(&temp, path).inspect_err(|_| {
        std::fs::remove_file(&temp).ok();
    })?;
    Ok(())
}

#[cfg(unix)]
fn replace_symlink(target: &Path, link: &Path) -> Result<()> {
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = link.with_extension(format!("link-{}", uuid::Uuid::new_v4()));
    std::os::unix::fs::symlink(target, &temp)?;
    std::fs::rename(&temp, link).inspect_err(|_| {
        std::fs::remove_file(&temp).ok();
    })?;
    Ok(())
}

#[cfg(not(unix))]
fn replace_symlink(_: &Path, _: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::AppId;

    fn launcher() -> AppInfo {
        AppInfo::builtin(AppId::LAUNCHER).expect("launcher")
    }

    #[test]
    fn plans_bundle_and_binary_renames() {
        let info = launcher();
        let exe = Path::new("/Applications/Ferrum Toolbox.app/Contents/MacOS/ferrum-launcher");
        let rename = plan(exe, Platform::MacOs, &info).expect("bundle rename");
        assert_eq!(
            rename,
            Rename::Bundle {
                from: "/Applications/Ferrum Toolbox.app".into(),
                to: "/Applications/Ferrum Launcher.app".into(),
            }
        );
        assert_eq!(rename.folder(), Path::new("/Applications"));
        assert_eq!(rename.old_name(), "Ferrum Toolbox.app");
        assert_eq!(rename.new_name(), "Ferrum Launcher.app");
        assert_eq!(
            rename.executable_after(&info),
            Path::new("/Applications/Ferrum Launcher.app/Contents/MacOS/ferrum-launcher")
        );

        // Already renamed, or a dev build: nothing to do.
        let current = Path::new("/Applications/Ferrum Launcher.app/Contents/MacOS/ferrum-launcher");
        assert_eq!(plan(current, Platform::MacOs, &info), None);
        assert_eq!(
            plan(Path::new("/repo/target/debug/ferrum-launcher"), Platform::MacOs, &info),
            None
        );

        let binary = plan(Path::new("/home/me/bin/ferrum-toolbox"), Platform::Linux, &info)
            .expect("binary rename");
        assert_eq!(
            binary,
            Rename::Binary {
                from: "/home/me/bin/ferrum-toolbox".into(),
                to: "/home/me/bin/ferrum-launcher".into(),
            }
        );
        let windows = plan(Path::new("C:/Ferrum/ferrum-toolbox.exe"), Platform::Windows, &info);
        assert!(matches!(windows, Some(Rename::Binary { to, .. }) if to.ends_with("ferrum-launcher.exe")));
    }

    #[test]
    fn rename_script_waits_moves_and_reopens() {
        let script = rename_script(
            Path::new("/Applications/Ferrum Toolbox.app"),
            Path::new("/Applications/Ferrum Launcher.app"),
            42,
        );
        let wait = script.find("kill -0 42").expect("waits for the pid");
        let mv = script
            .find("mv '/Applications/Ferrum Toolbox.app' '/Applications/Ferrum Launcher.app'")
            .expect("moves the bundle");
        assert!(wait < mv);
        assert!(script.contains("open '/Applications/Ferrum Launcher.app'"));
        assert!(script.contains(&format!(
            "open '/Applications/Ferrum Toolbox.app' --args {RENAME_FAILED_ARG}"
        )));
    }

    fn exited_pid() -> u32 {
        std::process::Command::new("true")
            .spawn()
            .and_then(|mut child| child.wait().map(|_| child.id()))
            .expect("spawn true")
    }

    fn run(script: String) -> bool {
        // `open` would launch an app; the tests only check the files.
        std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(script.replace("open ", ": "))
            .status()
            .expect("run script")
            .success()
    }

    #[test]
    fn renames_a_bundle_end_to_end_replacing_an_older_copy() {
        let dir = tempfile::tempdir().expect("tmp");
        let from = dir.path().join("Ferrum Toolbox.app");
        let to = dir.path().join("Ferrum Launcher.app");
        std::fs::create_dir_all(from.join("Contents")).expect("from");
        std::fs::write(from.join("Contents/version"), "0.6.0").expect("write");
        std::fs::create_dir_all(to.join("Contents")).expect("to");
        std::fs::write(to.join("Contents/version"), "stale").expect("write");
        assert!(can_rename(&Rename::Bundle {
            from: from.clone(),
            to: to.clone()
        }));

        assert!(run(rename_script(&from, &to, exited_pid())));
        assert!(!from.exists());
        assert_eq!(
            std::fs::read_to_string(to.join("Contents/version")).expect("read"),
            "0.6.0"
        );
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .expect("list")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".app."))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn failed_rename_restores_the_existing_copy() {
        let dir = tempfile::tempdir().expect("tmp");
        let from = dir.path().join("Ferrum Toolbox.app");
        let to = dir.path().join("Ferrum Launcher.app");
        std::fs::create_dir_all(to.join("Contents")).expect("to");
        // `from` is missing, so the move fails.
        assert!(!run(rename_script(&from, &to, exited_pid())));
        assert!(to.join("Contents").is_dir(), "the existing copy is put back");
    }

    #[cfg(unix)]
    #[test]
    fn renames_a_plain_binary_and_keeps_the_old_name_as_a_link() {
        let dir = tempfile::tempdir().expect("tmp");
        let from = dir.path().join("ferrum-toolbox");
        let to = dir.path().join("ferrum-launcher");
        std::fs::write(&from, b"bin").expect("write");
        rename_binary(&from, &to).expect("rename");
        assert_eq!(std::fs::read(&to).expect("read"), b"bin");
        assert_eq!(std::fs::read_link(&from).expect("link"), Path::new("ferrum-launcher"));
    }

    #[cfg(unix)]
    #[test]
    fn fixes_cli_links_only_when_the_toolbox_had_one() {
        let dir = tempfile::tempdir().expect("tmp");
        let bin = dir.path().join("bin");
        let info = launcher();
        let exe = dir.path().join("Ferrum Launcher.app/Contents/MacOS/ferrum-launcher");
        assert!(!fix_cli_links(&bin, Platform::MacOs, &info, &exe).expect("no links"));
        assert!(!bin.join("ferrum-launcher").exists());

        std::fs::create_dir_all(&bin).expect("mkdir");
        std::os::unix::fs::symlink("/old/Ferrum Toolbox.app/Contents/MacOS/ferrum-toolbox", bin.join("ferrum-toolbox"))
            .expect("old link");
        assert!(fix_cli_links(&bin, Platform::MacOs, &info, &exe).expect("fix"));
        assert_eq!(std::fs::read_link(bin.join("ferrum-launcher")).expect("new"), exe);
        assert_eq!(std::fs::read_link(bin.join("ferrum-toolbox")).expect("kept"), exe);
    }

    #[test]
    fn windows_cli_shims_point_at_the_new_exe() {
        let dir = tempfile::tempdir().expect("tmp");
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).expect("mkdir");
        std::fs::write(bin.join("ferrum-toolbox.cmd"), "old").expect("old shim");
        let exe = dir.path().join("ferrum-launcher.exe");
        assert!(fix_cli_links(&bin, Platform::Windows, &launcher(), &exe).expect("fix"));
        let shim = std::fs::read_to_string(bin.join("ferrum-launcher.cmd")).expect("shim");
        assert_eq!(shim, platform::cmd_shim(&exe));
        assert_eq!(
            std::fs::read_to_string(bin.join("ferrum-toolbox.cmd")).expect("kept"),
            shim
        );
    }

    #[test]
    fn replaces_the_toolbox_desktop_entry() {
        let dir = tempfile::tempdir().expect("tmp");
        let share = dir.path().join("share");
        let info = launcher();
        let exe = dir.path().join("ferrum-launcher");
        assert!(!fix_desktop_entry(&share, &info, &exe).expect("none"));

        std::fs::create_dir_all(share.join("applications")).expect("mkdir");
        std::fs::write(share.join("applications/ferrum-toolbox.desktop"), "old").expect("old");
        assert!(fix_desktop_entry(&share, &info, &exe).expect("fix"));
        assert!(!share.join("applications/ferrum-toolbox.desktop").exists());
        let entry = std::fs::read_to_string(share.join("applications/ferrum-launcher.desktop"))
            .expect("new entry");
        assert!(entry.contains("Name=Ferrum Launcher\n"));
        assert!(entry.contains(&format!("Exec=\"{}\" %U", exe.display())));
        assert!(share.join("icons/hicolor/256x256/apps/ferrum-launcher.png").is_file());
    }
}
