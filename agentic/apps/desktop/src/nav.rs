//! Where the main card is: the conversation or one of the sidebar's pages,
//! with back / forward between the pages and threads visited; and the
//! sidebar's saved state (`~/.ferrum/agentic/ui.json`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// What the main card shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Page {
    /// The project's chat.
    #[default]
    Chat,
    Projects,
    Discussions,
    Search,
    Knowledge,
    Sites,
    Automations,
    Extensions,
    Usage,
}

impl Page {
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Page::Chat => "Chat",
            Page::Projects => "Projects",
            Page::Discussions => "Discussions",
            Page::Search => "Search",
            Page::Knowledge => "Knowledge Center",
            Page::Sites => "Sites",
            Page::Automations => "Automations",
            Page::Extensions => "Extensions",
            Page::Usage => "Usage",
        }
    }
}

/// A place back / forward return to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    Page(Page),
    Thread { project: PathBuf, id: String },
}

/// Back / forward stacks around the current location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct History {
    pub current: Location,
    back: Vec<Location>,
    forward: Vec<Location>,
}

const MAX_HISTORY: usize = 100;

impl Default for History {
    fn default() -> Self {
        Self {
            current: Location::Page(Page::Chat),
            back: Vec::new(),
            forward: Vec::new(),
        }
    }
}

impl History {
    /// Goes to `to`; returns false when already there.
    pub fn visit(&mut self, to: Location) -> bool {
        if to == self.current {
            return false;
        }
        let from = std::mem::replace(&mut self.current, to);
        self.back.push(from);
        if self.back.len() > MAX_HISTORY {
            self.back.remove(0);
        }
        self.forward.clear();
        true
    }

    #[must_use]
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    #[must_use]
    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    pub fn back(&mut self) -> Option<Location> {
        let to = self.back.pop()?;
        let from = std::mem::replace(&mut self.current, to.clone());
        self.forward.push(from);
        Some(to)
    }

    pub fn forward(&mut self) -> Option<Location> {
        let to = self.forward.pop()?;
        let from = std::mem::replace(&mut self.current, to.clone());
        self.back.push(from);
        Some(to)
    }
}

pub const DEFAULT_SIDEBAR_WIDTH: f32 = 264.;
pub const MIN_SIDEBAR_WIDTH: f32 = 200.;
pub const MAX_SIDEBAR_WIDTH: f32 = 440.;

/// The sidebar as the user left it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiState {
    #[serde(default = "yes")]
    pub sidebar_open: bool,
    #[serde(default = "default_width")]
    pub sidebar_width: f32,
    /// Folders collapsed in the sidebar (the current project starts open,
    /// others closed; this lists the exceptions the user made).
    #[serde(default)]
    pub toggled_folders: Vec<PathBuf>,
}

fn yes() -> bool {
    true
}

fn default_width() -> f32 {
    DEFAULT_SIDEBAR_WIDTH
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            sidebar_open: true,
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            toggled_folders: Vec::new(),
        }
    }
}

impl UiState {
    #[must_use]
    pub fn load(file: &Path) -> Self {
        let mut state: Self = ferrum_ui::prefs::read(file);
        if !state.sidebar_width.is_finite() {
            state.sidebar_width = DEFAULT_SIDEBAR_WIDTH;
        }
        state.sidebar_width = state
            .sidebar_width
            .clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
        state
    }

    /// Whether `folder` is expanded; `current` starts expanded.
    #[must_use]
    pub fn is_expanded(&self, folder: &Path, current: bool) -> bool {
        current != self.toggled_folders.iter().any(|f| f == folder)
    }

    pub fn toggle_folder(&mut self, folder: &Path) {
        if let Some(i) = self.toggled_folders.iter().position(|f| f == folder) {
            self.toggled_folders.remove(i);
        } else {
            self.toggled_folders.push(folder.to_path_buf());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_and_forward_between_pages_and_threads() {
        let mut h = History::default();
        assert!(!h.can_go_back());
        assert!(h.visit(Location::Page(Page::Search)));
        assert!(!h.visit(Location::Page(Page::Search)));
        let thread = Location::Thread {
            project: "/a".into(),
            id: "t1".into(),
        };
        h.visit(thread.clone());
        assert_eq!(h.back(), Some(Location::Page(Page::Search)));
        assert_eq!(h.back(), Some(Location::Page(Page::Chat)));
        assert_eq!(h.back(), None);
        assert_eq!(h.forward(), Some(Location::Page(Page::Search)));
        // A new visit drops what was ahead.
        h.visit(Location::Page(Page::Sites));
        assert!(!h.can_go_forward());
        assert_eq!(h.back(), Some(Location::Page(Page::Search)));
        assert_eq!(h.back(), Some(Location::Page(Page::Chat)));
        assert_eq!(h.forward(), Some(Location::Page(Page::Search)));
        assert_eq!(h.forward(), Some(Location::Page(Page::Sites)));
        assert_ne!(h.current, thread);
    }

    #[test]
    fn ui_state_round_trips_and_is_repaired() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("ui.json");
        assert_eq!(UiState::load(&file), UiState::default());
        let mut state = UiState {
            sidebar_open: false,
            sidebar_width: 300.,
            toggled_folders: Vec::new(),
        };
        state.toggle_folder(Path::new("/a"));
        assert!(!state.is_expanded(Path::new("/a"), true));
        assert!(state.is_expanded(Path::new("/b"), true));
        assert!(!state.is_expanded(Path::new("/b"), false));
        ferrum_ui::prefs::write(&file, &state).expect("write");
        assert_eq!(UiState::load(&file), state);
        state.toggle_folder(Path::new("/a"));
        assert!(state.toggled_folders.is_empty());
        std::fs::write(&file, r#"{"sidebar_width": 9000}"#).expect("write");
        let repaired = UiState::load(&file);
        assert!(repaired.sidebar_open);
        assert_eq!(repaired.sidebar_width, MAX_SIDEBAR_WIDTH);
    }
}
