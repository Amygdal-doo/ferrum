//! `~/.ferrum/preferences.json`: the integration choices the launcher and
//! every app share — which apps run without a Dock icon (by binary name)
//! and the shell commands (see [`ferrum_shell::prefs`]). It is the source
//! of truth for both; the launcher's `state.json` keeps a mirror in
//! `AppPrefs`, refreshed from it (see `State::sync_shell_commands`).

use std::path::{Path, PathBuf};

pub use ferrum_shell::SharedPrefs;

/// `~/.ferrum/preferences.json` (the folder that holds `bin/`).
#[must_use]
pub fn path_for(bin_dir: &Path) -> Option<PathBuf> {
    Some(ferrum_shell::prefs::path(bin_dir.parent()?))
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
        prefs.save(&path).expect("write");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("\"hide_from_dock\"") && text.contains("\"ferrumeditor\""));
    }
}
