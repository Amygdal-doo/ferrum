//! The command scripts in `~/.ferrum/bin`: one per app (`fei`, `feg`, …)
//! and the `ferrum` umbrella (see [`crate::umbrella`]). Every script starts
//! with [`MARKER`] on its second line; only such files are ever replaced or
//! removed, so a user's own `~/.ferrum/bin/fei` is never touched.

use std::path::{Path, PathBuf};

use crate::apps::App;

/// First line after the shebang of every script written here (its wording
/// predates the shared crate and is kept: older scripts are recognised by it).
pub const MARKER: &str = "# Ferrum Launcher shell command";

/// Single-quotes `path` for `sh`.
pub(crate) fn quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

/// Exits with a hint when the app's CLI link is missing (not installed).
pub(crate) fn missing_check(app: &App, command: &str, target: &str) -> String {
    format!(
        "[ -x {target} ] || {{ echo \"{command}: {name} is not installed. Install it in FerrumLauncher.\" >&2; exit 1; }}\n",
        name = app.name
    )
}

/// The lines that start `app` through its CLI link `target` (quoted), with
/// the script's arguments. FerrumTerm's first opens a session in the folder
/// given (or the current one); FerrumContainers' runs its command line.
pub(crate) fn launch(app: &App, command: &str, target: &str) -> String {
    match app.id {
        "term" => format!(
            "dir=$(cd \"${{1:-.}}\" 2>/dev/null && pwd) || {{ echo \"{command}: no such folder: $1\" >&2; exit 1; }}\n\
             real=$(readlink {target} || echo {target})\n\
             fterm=\"$(dirname \"$real\")/fterm\"\n\
             [ -x \"$fterm\" ] && \"$fterm\" new \"$(basename \"$dir\")\" -c \"$dir\" >/dev/null 2>&1\n\
             cd \"$dir\" && nohup {target} >/dev/null 2>&1 &\n"
        ),
        // `fec` is FerrumContainers' command line (VM start/stop, compose,
        // docker); it opens the app itself for `fec` and `fec <folder>`.
        "dock" => format!(
            "real=$(readlink {target} || echo {target})\n\
             cli=\"$(dirname \"$real\")/ferrumdock-cli\"\n\
             [ -x \"$cli\" ] && exec \"$cli\" \"$@\"\n\
             nohup {target} \"$@\" >/dev/null 2>&1 &\n"
        ),
        _ => format!("nohup {target} \"$@\" >/dev/null 2>&1 &\n"),
    }
}

/// The script for `app`'s `command`. It runs the app's CLI link `entry`
/// (which always points at the active version) in the background, passing
/// the arguments through.
#[must_use]
pub fn app_script(app: &App, command: &str, entry: &Path) -> String {
    let target = quote(entry);
    format!(
        "#!/bin/sh\n{MARKER} for {name} (change it in {name}'s Settings › System or in FerrumLauncher).\n{missing}{launch}",
        name = app.name,
        missing = missing_check(app, command, &target),
        launch = launch(app, command, &target),
    )
}

/// Whether `path` is a script written here.
#[must_use]
pub fn is_ours(path: &Path) -> bool {
    let small = path
        .symlink_metadata()
        .is_ok_and(|m| m.is_file() && m.len() < 64 * 1024);
    small
        && std::fs::read_to_string(path)
            .is_ok_and(|text| text.lines().nth(1).is_some_and(|l| l.starts_with(MARKER)))
}

/// Writes `text` as the executable `bin_dir/name`. Refuses to replace a
/// file it didn't write. Returns whether the file changed.
pub fn write(bin_dir: &Path, name: &str, text: &str) -> Result<bool, String> {
    if cfg!(windows) {
        return Err("Shell commands are not available on Windows yet".into());
    }
    if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
        return Err(format!("`{name}` can't be a command"));
    }
    std::fs::create_dir_all(bin_dir).map_err(|e| e.to_string())?;
    let file = bin_dir.join(name);
    if (file.exists() || file.is_symlink()) && !is_ours(&file) {
        return Err(format!(
            "{} already exists and wasn't made by Ferrum",
            file.display()
        ));
    }
    if std::fs::read_to_string(&file).is_ok_and(|current| current == text) {
        return Ok(false);
    }
    std::fs::write(&file, text).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    Ok(true)
}

/// Removes `bin_dir/name` if it is ours. Returns whether it was removed.
pub fn remove(bin_dir: &Path, name: &str) -> Result<bool, String> {
    let file = bin_dir.join(name);
    if is_ours(&file) {
        std::fs::remove_file(&file).map_err(|e| e.to_string())?;
        return Ok(true);
    }
    Ok(false)
}

/// What [`sync`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Synced {
    /// A script was written, rewritten or removed.
    pub files_changed: bool,
    /// Scripts that could not be written (their name is someone else's
    /// file), with the reason.
    pub failed: Vec<(String, String)>,
}

/// Brings `bin_dir` in line with `wanted` (name → script text): each is
/// written if it differs, and every other script written here is removed,
/// so a name that moved to another app never opens the old one. Files that
/// aren't ours (CLI links, the user's scripts) are left alone.
pub fn sync(bin_dir: &Path, wanted: &[(String, String)]) -> Synced {
    let mut synced = Synced::default();
    if cfg!(windows) {
        return synced;
    }
    if let Ok(entries) = std::fs::read_dir(bin_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !wanted.iter().any(|(w, _)| *w == name) && is_ours(&entry.path()) {
                synced.files_changed |= std::fs::remove_file(entry.path()).is_ok();
            }
        }
    }
    for (name, text) in wanted {
        match write(bin_dir, name, text) {
            Ok(changed) => synced.files_changed |= changed,
            Err(error) => synced.failed.push((name.clone(), error)),
        }
    }
    synced
}

/// Another program named `name` on `path_env` (outside `bin_dir`), which
/// the command would hide or be hidden by.
#[must_use]
pub fn conflict(name: &str, bin_dir: &Path, path_env: &str) -> Option<PathBuf> {
    std::env::split_paths(path_env)
        .filter(|dir| dir != bin_dir)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// The command names found in `bin_dir`'s scripts, per app id: what is on
/// when no preferences say so yet (written by a launcher before the shared
/// preferences).
#[must_use]
pub fn discover(bin_dir: &Path) -> Vec<(&'static str, String)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(bin_dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == crate::umbrella::NAME || !is_ours(&entry.path()) {
            continue;
        }
        let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
        if let Some(app) = crate::apps::APPS
            .iter()
            .find(|app| text.contains(&format!("/{}'", app.binary)))
        {
            found.push((app.id, name));
        }
    }
    found.sort();
    found
}
