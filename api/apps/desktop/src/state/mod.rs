//! UI state owned by the root view. Mutations go through the root's
//! `Context`, which re-renders after `cx.notify()`.

pub mod history;
pub mod kv;
pub mod protocols;
pub mod runner;
pub mod tabs;
pub mod tools;
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
    /// Current project folder; its collections (and the shared ones) are listed.
    pub project: Option<String>,
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
    /// Auth secrets typed since the last autosave, by keychain account;
    /// the autosave loop writes them to the keychain (never to disk).
    pub secret_edits: HashMap<String, ferrumapi_domain::Auth>,
    /// Keychain accounts already loaded into a tab (tried once each).
    pub hydrated: HashSet<String>,
    /// Last OAuth 2 token message per auth (keychain account).
    pub oauth_status: HashMap<String, String>,
    pub sidebar_view: SidebarView,
    pub section: RequestSection,
    pub response_view: ResponseView,
    pub body_mode: BodyMode,
    /// Open find bar of the response body view.
    pub response_search: Option<ResponseSearch>,
    /// The current or last collection run (kept after its dialog closes).
    pub runner: Option<runner::RunnerState>,
    /// Mock servers by collection (running or with unsaved edits).
    pub mocks: HashMap<CollectionId, tools::MockState>,
    pub webhooks: tools::WebhookState,
    /// The current or last load test.
    pub load_test: Option<tools::LoadTestState>,
    /// GraphQL schemas, WSDL operations, OData metadata, gRPC methods and
    /// WebSocket sessions, per tab.
    pub protocols: protocols::ProtocolState,
    pub results_height: f64,
    /// Whether the sidebar is shown (⌘B).
    pub sidebar_open: bool,
    /// The response card was used last (it is drawn lighter), else the request card.
    pub response_active: bool,
    pub dialog: Option<Dialog>,
}

impl AppState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            loaded: false,
            project: None,
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
            secret_edits: HashMap::new(),
            oauth_status: HashMap::new(),
            hydrated: HashSet::new(),
            sidebar_view: SidebarView::Collections,
            section: RequestSection::Params,
            response_view: ResponseView::Body,
            body_mode: BodyMode::Pretty,
            response_search: None,
            runner: None,
            mocks: HashMap::new(),
            webhooks: tools::WebhookState::default(),
            load_test: None,
            protocols: protocols::ProtocolState::default(),
            results_height: 320.0,
            sidebar_open: true,
            response_active: false,
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
