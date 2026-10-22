//! Reactive UI state. Every field is a `Copy` signal, so `AppState` can be
//! moved freely into closures.

pub mod history;
pub mod kv;
pub mod tabs;
mod types;

pub use types::{Dialog, EditorRevs, Notice, RequestSection, SidebarView, TabResponse};

use dioxus::prelude::*;
use ferrumapi_application::CancellationToken;
use ferrumapi_application::{Prepared, prepare};
use ferrumapi_domain::{
    AppSettings, Collection, CollectionId, Environment, EnvironmentId, HistoryEntry, RequestTab,
    TabId, Theme,
};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Default sidebar width when settings hold 0.
pub const DEFAULT_SIDEBAR_WIDTH: f64 = 280.0;

#[derive(Clone, Copy)]
pub struct AppState {
    pub loaded: Signal<bool>,
    pub settings: Signal<AppSettings>,
    pub system_dark: Signal<bool>,
    pub collections: Signal<Vec<Collection>>,
    pub environments: Signal<Vec<Environment>>,
    pub globals: Signal<Environment>,
    pub active_env: Signal<Option<EnvironmentId>>,
    pub history: Signal<Vec<HistoryEntry>>,
    pub tabs: Signal<Vec<RequestTab>>,
    pub active_tab: Signal<Option<TabId>>,
    pub responses: Signal<HashMap<TabId, TabResponse>>,
    pub cancels: Signal<HashMap<TabId, CancellationToken>>,
    /// Bumped on every workspace change; the autosave loop compares it.
    pub ws_rev: Signal<u64>,
    pub revs: Signal<EditorRevs>,
    /// Expanded collection / folder ids in the tree.
    pub expanded: Signal<HashSet<Uuid>>,
    /// Collection / item / environment id being renamed inline.
    pub renaming: Signal<Option<Uuid>>,
    pub sidebar_view: Signal<SidebarView>,
    pub section: Signal<RequestSection>,
    pub results_height: Signal<f64>,
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
            collections: Signal::new(Vec::new()),
            environments: Signal::new(Vec::new()),
            globals: Signal::new(Environment::globals()),
            active_env: Signal::new(None),
            history: Signal::new(Vec::new()),
            tabs: Signal::new(Vec::new()),
            active_tab: Signal::new(None),
            responses: Signal::new(HashMap::new()),
            cancels: Signal::new(HashMap::new()),
            ws_rev: Signal::new(0),
            revs: Signal::new(EditorRevs::default()),
            expanded: Signal::new(HashSet::new()),
            renaming: Signal::new(None),
            sidebar_view: Signal::new(SidebarView::Collections),
            section: Signal::new(RequestSection::Params),
            results_height: Signal::new(320.0),
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
        let w = self.settings.read().sidebar_width;
        if w <= 0.0 {
            DEFAULT_SIDEBAR_WIDTH
        } else {
            w.clamp(200.0, 560.0)
        }
    }

    #[must_use]
    pub fn active_tab(&self) -> Option<RequestTab> {
        let id = (*self.active_tab.read())?;
        self.tabs.read().iter().find(|t| t.id == id).cloned()
    }

    #[must_use]
    pub fn active_environment(&self) -> Option<Environment> {
        let id = (*self.active_env.read())?;
        self.environments
            .read()
            .iter()
            .find(|e| e.id == id)
            .cloned()
    }

    #[must_use]
    pub fn collection(&self, id: CollectionId) -> Option<Collection> {
        self.collections.read().iter().find(|c| c.id == id).cloned()
    }

    /// Auth resolved and variables gathered for `tab`.
    #[must_use]
    pub fn prepare_tab(&self, tab: &RequestTab) -> Prepared {
        let env = self.active_environment();
        prepare(
            &tab.request,
            tab.origin,
            &self.collections.read(),
            &self.globals.read(),
            env.as_ref(),
        )
    }

    /// Keys marked secret in any layer visible to `tab` (for masking).
    #[must_use]
    pub fn secret_keys(&self, tab: &RequestTab) -> Vec<String> {
        let mut keys: Vec<String> = Vec::new();
        let mut take = |vars: &[ferrumapi_domain::Variable]| {
            keys.extend(
                vars.iter()
                    .filter(|v| v.secret)
                    .map(|v| v.key.trim().to_owned()),
            );
        };
        take(&self.globals.read().variables);
        if let Some(c) = tab.origin.and_then(|o| self.collection(o.collection)) {
            take(&c.variables);
        }
        if let Some(e) = self.active_environment() {
            take(&e.variables);
        }
        keys
    }

    pub fn notify(mut self, message: impl Into<String>, error: bool) {
        self.notice.set(Some(Notice {
            message: message.into(),
            error,
        }));
    }

    /// Marks the workspace as changed so the autosave loop persists it.
    pub fn touch(mut self) {
        *self.ws_rev.write() += 1;
    }

    /// Edits a tab's request and marks it dirty.
    pub fn edit_request(mut self, id: TabId, f: impl FnOnce(&mut ferrumapi_domain::HttpRequest)) {
        if let Some(tab) = self.tabs.write().iter_mut().find(|t| t.id == id) {
            f(&mut tab.request);
            tab.dirty = true;
        }
        self.touch();
    }

    /// Edits tab metadata (title, origin, …) without marking it dirty.
    pub fn update_tab(mut self, id: TabId, f: impl FnOnce(&mut RequestTab)) {
        if let Some(tab) = self.tabs.write().iter_mut().find(|t| t.id == id) {
            f(tab);
        }
        self.touch();
    }
}
