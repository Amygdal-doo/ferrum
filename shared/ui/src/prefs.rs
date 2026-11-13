//! JSON preference files under `~/.ferrum/` shared by every Ferrum app:
//! `appearance.json`, `onboarding.json`, `notifications.json` and the
//! per-app `<binary>/settings.json` / `<binary>/keymap.json`.
//!
//! Files are written atomically (write + rename). A missing or unreadable
//! file reads as the type's default, so a broken file never stops an app.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// `~/.ferrum`, or `$FERRUM_HOME` when set (tests, portable installs).
pub fn ferrum_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FERRUM_HOME") {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(Path::new(&home).join(".ferrum"))
}

/// `~/.ferrum/<relative>`.
pub fn path(relative: &str) -> Option<PathBuf> {
    Some(ferrum_dir()?.join(relative))
}

/// Reads `path` as JSON; the default when it is missing or invalid.
pub fn read<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Writes `value` to `path` as pretty JSON, creating parent folders.
pub fn write<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, json)?;
    std::fs::rename(&partial, path)
}

/// Last modification time, to notice changes made by another app.
pub fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// The running executable's file name (`ferrumterm`, `ferrumgit`, …).
pub fn current_binary() -> Option<String> {
    std::env::current_exe()
        .ok()?
        .file_stem()
        .map(|n| n.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Sample {
        name: String,
    }

    #[test]
    fn round_trips_and_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("nested/sample.json");
        assert_eq!(read::<Sample>(&file), Sample::default());
        let value = Sample { name: "x".into() };
        write(&file, &value).expect("write");
        assert_eq!(read::<Sample>(&file), value);
        std::fs::write(&file, "{not json").expect("corrupt");
        assert_eq!(read::<Sample>(&file), Sample::default());
    }
}
