//! UI state owned by the root view. Result rows live in bounded
//! [`ResultBuffer`](ferrumdata_result_grid_core::ResultBuffer)s; the grid only
//! ever renders the visible slice.

mod keys;
mod run;
mod tools;

pub use keys::{KeyBrowser, KeyViewLoad};
pub use run::TabRun;
pub use tools::{DiagramView, PlanLoad, SnapshotLoad, ToolTab};

use ferrumdata_domain::{
    AppSettings, ConnectionProfile, EditorTabId, EditorTabState, HistoryEntry, ProfileId,
    TableDetails, TableInfo, TableRef,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub enum ConnStatus {
    Connecting,
    Ready(String),
    Failed(String),
}

/// Table list for one connection, loaded lazily when the tree is expanded.
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaLoad {
    Loading,
    Loaded(Vec<TableInfo>),
    Failed(String),
}

/// Columns and indexes of one table, loaded when its tree node is expanded.
#[derive(Debug, Clone, PartialEq)]
pub enum DetailsLoad {
    Loading,
    Loaded(TableDetails),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorTab {
    pub id: EditorTabId,
    pub title: String,
    pub sql: String,
    pub profile_id: Option<ProfileId>,
    /// Edited since the last save to local state.
    pub unsaved: bool,
    /// Set for table-browser tabs.
    pub table: Option<TableRef>,
    /// Set for key tabs (Redis value viewer). Not restored after a restart.
    pub key: Option<String>,
    /// Set for tool tabs (ER diagram …). Not restored after a restart.
    pub tool: Option<ToolTab>,
}

impl EditorTab {
    #[must_use]
    pub fn new(n: usize, profile_id: Option<ProfileId>) -> Self {
        Self {
            id: EditorTabId::new(),
            title: format!("SQL {n}"),
            sql: String::new(),
            profile_id,
            unsaved: false,
            table: None,
            key: None,
            tool: None,
        }
    }

    #[must_use]
    pub fn to_state(&self, position: usize) -> EditorTabState {
        EditorTabState {
            id: self.id,
            title: self.title.clone(),
            sql: self.sql.clone(),
            profile_id: self.profile_id,
            position: i64::try_from(position).unwrap_or(i64::MAX),
            table: self.table.clone(),
        }
    }
}

impl From<EditorTabState> for EditorTab {
    fn from(s: EditorTabState) -> Self {
        Self {
            id: s.id,
            title: s.title,
            sql: s.sql,
            profile_id: s.profile_id,
            unsaved: false,
            table: s.table,
            key: None,
            tool: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Dialog {
    /// `None` creates a new profile.
    ConnectionForm(Option<ConnectionProfile>),
    DeleteProfile(ConnectionProfile),
    ConfirmRun {
        tab_id: EditorTabId,
        profile: ConnectionProfile,
        sql: String,
    },
    /// `None` clears the history of every connection.
    ClearHistory(Option<ConnectionProfile>),
    /// EXPLAIN ANALYZE executes the statement: confirm first.
    ConfirmAnalyze {
        tab_id: EditorTabId,
        profile: ConnectionProfile,
        sql: String,
    },
    DeleteKey {
        profile: ConnectionProfile,
        key: String,
    },
    /// Staged row edits of a table tab on a production connection.
    ConfirmApply {
        tab_id: EditorTabId,
        profile: ConnectionProfile,
        sql: String,
    },
}

/// What the bottom panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResultsPane {
    #[default]
    Results,
    History,
    /// EXPLAIN plan of the tab.
    Plan,
}

/// Which history entries to list: one connection's, or every connection's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryScope {
    /// `None` lists every connection.
    pub profile_id: Option<ProfileId>,
    pub search: String,
}

#[derive(Debug, Default)]
pub struct HistoryPanel {
    /// List every connection instead of the active tab's.
    pub all_connections: bool,
    pub entries: Vec<HistoryEntry>,
    pub loading: bool,
    pub error: Option<String>,
    /// Scope the entries were loaded for; a different scope triggers a reload.
    pub loaded_for: Option<HistoryScope>,
    /// Bumped per request so a slow, older response cannot overwrite a newer one.
    pub generation: u64,
}

#[derive(Default)]
pub struct AppState {
    pub loaded: bool,
    pub settings: AppSettings,
    pub profiles: Vec<ConnectionProfile>,
    pub conn_status: HashMap<ProfileId, ConnStatus>,
    pub schemas: HashMap<ProfileId, SchemaLoad>,
    pub expanded: HashSet<ProfileId>,
    /// Tables whose column/index details are shown in the tree.
    pub expanded_tables: HashSet<(ProfileId, TableRef)>,
    pub table_details: HashMap<(ProfileId, TableRef), DetailsLoad>,
    pub tabs: Vec<EditorTab>,
    pub active_tab: Option<EditorTabId>,
    pub runs: HashMap<EditorTabId, TabRun>,
    pub dialog: Option<Dialog>,
    pub results_pane: ResultsPane,
    pub history: HistoryPanel,
    /// Key explorer per key-value (Redis) connection.
    pub key_browsers: HashMap<ProfileId, KeyBrowser>,
    /// Value of the key shown in each key tab.
    pub key_views: HashMap<EditorTabId, KeyViewLoad>,
    /// EXPLAIN result per tab.
    pub plans: HashMap<EditorTabId, PlanLoad>,
    /// ER diagram per diagram tab.
    pub diagrams: HashMap<EditorTabId, DiagramView>,
    /// The sidebar is hidden (⌘B).
    pub sidebar_hidden: bool,
    /// The results card was used last (it is drawn lighter), else the editor card.
    pub results_active: bool,
}

impl AppState {
    #[must_use]
    pub fn active_tab(&self) -> Option<&EditorTab> {
        let id = self.active_tab?;
        self.tabs.iter().find(|t| t.id == id)
    }

    #[must_use]
    pub fn profile(&self, id: ProfileId) -> Option<&ConnectionProfile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    /// Profile bound to the active tab.
    #[must_use]
    pub fn active_profile(&self) -> Option<&ConnectionProfile> {
        self.active_tab()
            .and_then(|t| t.profile_id)
            .and_then(|id| self.profile(id))
    }

    /// The active tab's run, if one is in flight.
    #[must_use]
    pub fn active_run_in_flight(&self) -> bool {
        self.active_tab
            .and_then(|id| self.runs.get(&id))
            .is_some_and(|r| !r.state.is_terminal())
    }

    pub fn update_tab(&mut self, id: EditorTabId, f: impl FnOnce(&mut EditorTab)) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            f(tab);
        }
    }

    /// Binds the active tab to `profile_id` (or unbinds it).
    pub fn bind_active_tab(&mut self, profile_id: Option<ProfileId>) {
        if let Some(tab_id) = self.active_tab {
            self.update_tab(tab_id, |t| {
                t.profile_id = profile_id;
                t.unsaved = true;
            });
        }
    }
}
