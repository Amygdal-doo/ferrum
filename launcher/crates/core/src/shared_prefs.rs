//! `~/.ferrum/preferences.json`: the launcher's choices that the apps
//! themselves read when they start (`ferrum_ui` does it for every app).
//! Today that is which apps run without a Dock icon, by binary name.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SharedPrefs {
    /// Binaries (`ferrumeditor`, `ferrumgit`, …) that hide their Dock icon.
    pub hide_from_dock: BTreeSet<String>,
}

/// `~/.ferrum/preferences.json` (the folder that holds `bin/`).
#[must_use]
pub fn path_for(bin_dir: &Path) -> Option<PathBuf> {
    Some(bin_dir.parent()?.join("preferences.json"))
}

pub fn write(path: &Path, prefs: &SharedPrefs) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(prefs).map_err(std::io::Error::other)?;
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, json)?;
    std::fs::rename(&partial, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_what_ferrum_ui_reads() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = path_for(&dir.path().join(".ferrum/bin")).expect("path");
        assert_eq!(path, dir.path().join(".ferrum/preferences.json"));
        let mut prefs = SharedPrefs::default();
        prefs.hide_from_dock.insert("ferrumeditor".into());
        write(&path, &prefs).expect("write");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("\"hide_from_dock\"") && text.contains("\"ferrumeditor\""));
    }
}
