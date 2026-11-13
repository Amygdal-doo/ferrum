//! UI state owned by the root view. Plain data: the view mutates it and
//! calls `cx.notify()`.

mod types;

pub use types::{
    Busy, Dialog, DiffView, FileTab, FileView, Load, Notice, Page, PrTab, PromptKind, RepoView,
    SearchHits, Selection,
};

use ferrumgit_application::DEFAULT_LOG_LIMIT;
use ferrumgit_domain::{AppSettings, Profile, RepoEntry, RepoId};
use std::collections::HashMap;

pub const DEFAULT_SIDEBAR_WIDTH: f64 = 260.0;
/// Width of the middle column (commit list / file list).
pub const DEFAULT_DETAIL_WIDTH: f64 = 420.0;

#[derive(Default)]
pub struct AppState {
    pub loaded: bool,
    pub settings: AppSettings,
    /// Known repositories, most recent first.
    pub repos: Vec<RepoEntry>,
    pub profiles: Vec<Profile>,
    /// Open repository tabs.
    pub open: Vec<RepoId>,
    /// `None` shows the home screen.
    pub active: Option<RepoId>,
    pub views: HashMap<RepoId, RepoView>,
    pub busy: HashMap<RepoId, Busy>,
    pub notice: Option<Notice>,
}

impl AppState {
    #[must_use]
    pub fn sidebar_width(&self) -> f64 {
        match self.settings.sidebar_width {
            w if w <= 0.0 => DEFAULT_SIDEBAR_WIDTH,
            w => w.clamp(180.0, 480.0),
        }
    }

    #[must_use]
    pub fn detail_width(&self) -> f64 {
        match self.settings.detail_width {
            w if w <= 0.0 => DEFAULT_DETAIL_WIDTH,
            w => w.clamp(300.0, 900.0),
        }
    }

    #[must_use]
    pub fn log_limit(&self) -> usize {
        match self.settings.log_limit {
            0 => DEFAULT_LOG_LIMIT,
            n => n as usize,
        }
    }

    #[must_use]
    pub fn entry(&self, id: RepoId) -> Option<RepoEntry> {
        self.repos.iter().find(|r| r.id == id).cloned()
    }

    #[must_use]
    pub fn active_entry(&self) -> Option<RepoEntry> {
        self.active.and_then(|id| self.entry(id))
    }

    #[must_use]
    pub fn profile_of(&self, entry: &RepoEntry) -> Option<Profile> {
        let id = entry.profile?;
        self.profiles.iter().find(|p| p.id == id).cloned()
    }

    #[must_use]
    pub fn view(&self, id: RepoId) -> RepoView {
        self.views.get(&id).cloned().unwrap_or_default()
    }

    pub fn update_view(&mut self, id: RepoId, f: impl FnOnce(&mut RepoView)) {
        f(self.views.entry(id).or_default());
    }

    #[must_use]
    pub fn is_busy(&self, id: RepoId) -> bool {
        self.busy.contains_key(&id)
    }
}
