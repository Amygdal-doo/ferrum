//! Reactive UI state. Result rows live in bounded [`ResultBuffer`]s; the grid
//! only ever renders the visible slice.

mod run;

pub use run::TabRun;

use dioxus::prelude::*;
use ferrum_domain::{
    AppSettings, ConnectionProfile, EditorTabId, EditorTabState, ProfileId, TableInfo, TableRef,
    Theme,
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub message: String,
    pub error: bool,
}

/// All signals are `Copy`; pass `AppState` freely into closures.
#[derive(Clone, Copy)]
pub struct AppState {
    pub loaded: Signal<bool>,
    pub settings: Signal<AppSettings>,
    pub system_dark: Signal<bool>,
    pub profiles: Signal<Vec<ConnectionProfile>>,
    pub conn_status: Signal<HashMap<ProfileId, ConnStatus>>,
    pub schemas: Signal<HashMap<ProfileId, SchemaLoad>>,
    pub expanded: Signal<HashSet<ProfileId>>,
    pub tabs: Signal<Vec<EditorTab>>,
    pub active_tab: Signal<Option<EditorTabId>>,
    pub runs: Signal<HashMap<EditorTabId, TabRun>>,
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
            profiles: Signal::new(Vec::new()),
            conn_status: Signal::new(HashMap::new()),
            schemas: Signal::new(HashMap::new()),
            expanded: Signal::new(HashSet::new()),
            tabs: Signal::new(Vec::new()),
            active_tab: Signal::new(None),
            runs: Signal::new(HashMap::new()),
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
    pub fn active_tab(&self) -> Option<EditorTab> {
        let id = (*self.active_tab.read())?;
        self.tabs.read().iter().find(|t| t.id == id).cloned()
    }

    #[must_use]
    pub fn profile(&self, id: ProfileId) -> Option<ConnectionProfile> {
        self.profiles.read().iter().find(|p| p.id == id).cloned()
    }

    /// Profile bound to the active tab.
    #[must_use]
    pub fn active_profile(&self) -> Option<ConnectionProfile> {
        self.active_tab()
            .and_then(|t| t.profile_id)
            .and_then(|id| self.profile(id))
    }

    pub fn notify(mut self, message: impl Into<String>, error: bool) {
        self.notice.set(Some(Notice {
            message: message.into(),
            error,
        }));
    }

    pub fn update_tab(mut self, id: EditorTabId, f: impl FnOnce(&mut EditorTab)) {
        if let Some(tab) = self.tabs.write().iter_mut().find(|t| t.id == id) {
            f(tab);
        }
    }
}
