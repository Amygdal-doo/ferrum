//! UI state owned by the root view. Mutations go through the root's
//! `Context`, which re-renders after `cx.notify()`.

pub mod history;
pub mod kv;
pub mod runner;
pub mod tabs;
mod types;

pub use types::{
    BodyMode, Dialog, RequestSection, ResponseSearch, ResponseView, SettingsPane, SidebarView,
    TabResponse,
};

use ferrumapi_application::CancellationToken;
use ferrumapi_application::{Prepared, prepare};
use ferrumapi_domain::{
    AppSettings, Collection, CollectionId, Environment, EnvironmentId, HistoryEntry, RequestTab,
    TabId,
};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Default sidebar width when settings hold 0.
pub const DEFAULT_SIDEBAR_WIDTH: f64 = 280.0;

pub struct AppState {
    pub loaded: bool,
    pub settings: AppSettings,
    pub collections: Vec<Collection>,
    pub environments: Vec<Environment>,
    pub globals: Environment,
    pub active_env: Option<EnvironmentId>,
    pub history: Vec<HistoryEntry>,
    pub tabs: Vec<RequestTab>,
    pub active_tab: Option<TabId>,
    pub responses: HashMap<TabId, TabResponse>,
    pub cancels: HashMap<TabId, CancellationToken>,
    /// Bumped on every workspace change; the autosave loop compares it.
    pub ws_rev: u64,
    /// Expanded collection / folder ids in the tree.
    pub expanded: HashSet<Uuid>,
    /// Collection / item / environment id being renamed inline.
    pub renaming: Option<Uuid>,
    /// Request tab being renamed in the tab strip.
    pub renaming_tab: Option<TabId>,
    pub collection_filter: String,
    /// Field keys of secret inputs currently shown in clear text.
    pub revealed: HashSet<String>,
    pub sidebar_view: SidebarView,
    pub section: RequestSection,
    pub response_view: ResponseView,
    pub body_mode: BodyMode,
    /// Open find bar of the response body view.
    pub response_search: Option<ResponseSearch>,
    /// The current or last collection run (kept after its dialog closes).
    pub runner: Option<runner::RunnerState>,
    pub results_height: f64,
    pub dialog: Option<Dialog>,
}

impl AppState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            loaded: false,
            settings: AppSettings::default(),
            collections: Vec::new(),
            environments: Vec::new(),
            globals: Environment::globals(),
            active_env: None,
            history: Vec::new(),
            tabs: Vec::new(),
            active_tab: None,
            responses: HashMap::new(),
            cancels: HashMap::new(),
            ws_rev: 0,
            expanded: HashSet::new(),
            renaming: None,
            renaming_tab: None,
            collection_filter: String::new(),
            revealed: HashSet::new(),
            sidebar_view: SidebarView::Collections,
            section: RequestSection::Params,
            response_view: ResponseView::Body,
            body_mode: BodyMode::Pretty,
            response_search: None,
            runner: None,
            results_height: 320.0,
            dialog: None,
        }
    }

    #[must_use]
    pub fn sidebar_width(&self) -> f64 {
        let w = self.settings.sidebar_width;
        if w <= 0.0 {
            DEFAULT_SIDEBAR_WIDTH
        } else {
            w.clamp(200.0, 560.0)
        }
    }

    #[must_use]
    pub fn active_tab(&self) -> Option<&RequestTab> {
        let id = self.active_tab?;
        self.tabs.iter().find(|t| t.id == id)
    }

    #[must_use]
    pub fn tab(&self, id: TabId) -> Option<&RequestTab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    #[must_use]
    pub fn active_environment(&self) -> Option<&Environment> {
        let id = self.active_env?;
        self.environments.iter().find(|e| e.id == id)
    }

    #[must_use]
    pub fn collection(&self, id: CollectionId) -> Option<&Collection> {
        self.collections.iter().find(|c| c.id == id)
    }

    /// Auth resolved and variables gathered for `tab`.
    #[must_use]
    pub fn prepare_tab(&self, tab: &RequestTab) -> Prepared {
        prepare(
            &tab.request,
            tab.origin,
            &self.collections,
            &self.globals,
            self.active_environment(),
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
        take(&self.globals.variables);
        if let Some(c) = tab.origin.and_then(|o| self.collection(o.collection)) {
            take(&c.variables);
        }
        if let Some(e) = self.active_environment() {
            take(&e.variables);
        }
        keys
    }

    #[must_use]
    pub fn is_loading(&self, id: TabId) -> bool {
        self.responses.get(&id).is_some_and(TabResponse::is_loading)
    }

    /// Marks the workspace as changed so the autosave loop persists it.
    pub fn touch(&mut self) {
        self.ws_rev += 1;
    }

    /// Edits a tab's request and marks it dirty.
    pub fn edit_request(&mut self, id: TabId, f: impl FnOnce(&mut ferrumapi_domain::HttpRequest)) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            f(&mut tab.request);
            tab.dirty = true;
        }
        self.touch();
    }

    /// Edits tab metadata (title, origin, …) without marking it dirty.
    pub fn update_tab(&mut self, id: TabId, f: impl FnOnce(&mut RequestTab)) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            f(tab);
        }
        self.touch();
    }
}
