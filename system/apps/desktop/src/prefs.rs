//! The window's small preferences, in `~/.ferrum/ferrumsystem/ui.json`:
//! whether the demo has been shown, whether the sidebar and the right
//! panel are open, and the project shown last. A missing or broken file
//! reads as the defaults (a first run).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    /// The demo has been shown once (at the first start, or from the menu).
    pub demo_seen: bool,
    pub sidebar_open: bool,
    /// The right panel (overview, APIs, data, …).
    pub panel_open: bool,
    /// The project folder open last, reopened at the next start.
    pub last_project: Option<PathBuf>,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            demo_seen: false,
            sidebar_open: true,
            panel_open: true,
            last_project: None,
        }
    }
}

/// `~/.ferrum/ferrumsystem/ui.json`.
pub fn default_file() -> Option<PathBuf> {
    ferrum_ui::prefs::path("ferrumsystem/ui.json")
}

/// The saved prefs; the defaults without a file.
pub fn load(file: Option<&Path>) -> UiPrefs {
    file.map(ferrum_ui::prefs::read).unwrap_or_default()
}

pub fn save(file: Option<&Path>, prefs: &UiPrefs) -> std::io::Result<()> {
    match file {
        Some(file) => ferrum_ui::prefs::write(file, prefs),
        None => Ok(()),
    }
}

/// The project to open at start: the one asked for, else the last one
/// (while its folder still exists).
pub fn start_project(asked: Option<PathBuf>, prefs: &UiPrefs) -> Option<PathBuf> {
    asked.or_else(|| prefs.last_project.clone().filter(|p| p.is_dir()))
}

/// What the window shows first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstView {
    /// The project's own diagram.
    Project,
    /// The demo: only at the very first start.
    Demo,
    /// "No diagram": New diagram / Open… / Open demo.
    Empty,
}

pub fn first_view(prefs: &UiPrefs, project_has_system: bool) -> FirstView {
    if project_has_system {
        FirstView::Project
    } else if !prefs.demo_seen {
        FirstView::Demo
    } else {
        FirstView::Empty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_is_shown_only_until_it_has_been_seen() {
        let fresh = UiPrefs::default();
        assert_eq!(first_view(&fresh, false), FirstView::Demo);
        let seen = UiPrefs {
            demo_seen: true,
            ..UiPrefs::default()
        };
        assert_eq!(first_view(&seen, false), FirstView::Empty);
    }

    #[test]
    fn a_project_diagram_wins_over_the_demo() {
        assert_eq!(first_view(&UiPrefs::default(), true), FirstView::Project);
        let seen = UiPrefs {
            demo_seen: true,
            ..UiPrefs::default()
        };
        assert_eq!(first_view(&seen, true), FirstView::Project);
    }

    #[test]
    fn the_last_project_is_reopened_unless_another_is_asked_for() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        std::fs::create_dir_all(&a).expect("mkdir");
        let prefs = UiPrefs {
            last_project: Some(a.clone()),
            ..UiPrefs::default()
        };
        assert_eq!(start_project(None, &prefs), Some(a.clone()));
        assert_eq!(start_project(Some(b.clone()), &prefs), Some(b));
        // A deleted folder is not reopened.
        std::fs::remove_dir(&a).expect("rmdir");
        assert_eq!(start_project(None, &prefs), None);
    }

    #[test]
    fn prefs_round_trip_and_default_when_missing_or_broken() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("ferrumsystem").join("ui.json");
        assert_eq!(load(Some(&file)), UiPrefs::default());
        assert_eq!(load(None), UiPrefs::default());
        let prefs = UiPrefs {
            demo_seen: true,
            sidebar_open: false,
            panel_open: false,
            last_project: Some(dir.path().to_path_buf()),
        };
        save(Some(&file), &prefs).expect("save");
        assert_eq!(load(Some(&file)), prefs);
        std::fs::write(&file, "not json").expect("write");
        assert_eq!(load(Some(&file)), UiPrefs::default());
        // Fields added later read as their defaults.
        std::fs::write(&file, r#"{"demo_seen":true}"#).expect("write");
        let partial = load(Some(&file));
        assert!(partial.demo_seen && partial.sidebar_open && partial.panel_open);
    }
}
