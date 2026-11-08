//! Reactive UI state. Every field is a `Copy` signal, so `AppState` moves
//! freely into closures.

mod types;

pub use types::{Busy, Dialog, DiffView, Notice, Page, PromptKind, RepoView, Selection};

use dioxus::prelude::*;
use ferrumgit_application::DEFAULT_LOG_LIMIT;
use ferrumgit_domain::{AppSettings, Profile, RepoEntry, RepoId, Theme};
use std::collections::HashMap;

pub const DEFAULT_SIDEBAR_WIDTH: f64 = 260.0;
/// Width of the middle column (commit list / file list).
pub const DEFAULT_DETAIL_WIDTH: f64 = 420.0;

#[derive(Clone, Copy)]
pub struct AppState {
    pub loaded: Signal<bool>,
    pub settings: Signal<AppSettings>,
    pub system_dark: Signal<bool>,
    /// Known repositories, most recent first.
    pub repos: Signal<Vec<RepoEntry>>,
    pub profiles: Signal<Vec<Profile>>,
    /// Open repository tabs.
    pub open: Signal<Vec<RepoId>>,
    /// `None` shows the home screen.
    pub active: Signal<Option<RepoId>>,
    pub views: Signal<HashMap<RepoId, RepoView>>,
    pub busy: Signal<HashMap<RepoId, Busy>>,
    pub dialog: Signal<Option<Dialog>>,
    pub notice: Signal<Option<Notice>>,
}

impl AppState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            loaded: Signal::new(false),
            settings: Signal::new(AppSettings::default()),
            system_dark: Signal::new(false),
            repos: Signal::new(Vec::new()),
            profiles: Signal::new(Vec::new()),
            open: Signal::new(Vec::new()),
            active: Signal::new(None),
            views: Signal::new(HashMap::new()),
            busy: Signal::new(HashMap::new()),
            dialog: Signal::new(None),
            notice: Signal::new(None),
        }
    }

    #[must_use]
    pub fn is_dark(&self) -> bool {
        match self.settings.read().theme {
            Theme::Dark => true,
            Theme::Light => false,
            Theme::System => *self.system_dark.read(),
        }
    }

    #[must_use]
    pub fn sidebar_width(&self) -> f64 {
        match self.settings.read().sidebar_width {
            w if w <= 0.0 => DEFAULT_SIDEBAR_WIDTH,
            w => w.clamp(180.0, 480.0),
        }
    }

    #[must_use]
    pub fn detail_width(&self) -> f64 {
        match self.settings.read().detail_width {
            w if w <= 0.0 => DEFAULT_DETAIL_WIDTH,
            w => w.clamp(300.0, 900.0),
        }
    }

    #[must_use]
    pub fn log_limit(&self) -> usize {
        match self.settings.peek().log_limit {
            0 => DEFAULT_LOG_LIMIT,
            n => n as usize,
        }
    }

    #[must_use]
    pub fn entry(&self, id: RepoId) -> Option<RepoEntry> {
        self.repos.read().iter().find(|r| r.id == id).cloned()
    }

    #[must_use]
    pub fn active_entry(&self) -> Option<RepoEntry> {
        (*self.active.read()).and_then(|id| self.entry(id))
    }

    #[must_use]
    pub fn profile_of(&self, entry: &RepoEntry) -> Option<Profile> {
        let id = entry.profile?;
        self.profiles.read().iter().find(|p| p.id == id).cloned()
    }

    #[must_use]
    pub fn view(&self, id: RepoId) -> RepoView {
        self.views.read().get(&id).cloned().unwrap_or_default()
    }

    pub fn update_view(mut self, id: RepoId, f: impl FnOnce(&mut RepoView)) {
        f(self.views.write().entry(id).or_default());
    }

    #[must_use]
    pub fn is_busy(&self, id: RepoId) -> bool {
        self.busy.read().contains_key(&id)
    }

    pub fn notify(mut self, message: impl Into<String>, error: bool) {
        self.notice.set(Some(Notice {
            message: message.into(),
            error,
        }));
    }
}
