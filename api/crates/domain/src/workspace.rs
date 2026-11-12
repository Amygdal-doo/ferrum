//! Open tabs and app settings, restored after restart.

use crate::collection::RequestOrigin;
use crate::ids::{EnvironmentId, TabId};
use crate::request::HttpRequest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RequestTab {
    pub id: TabId,
    pub title: String,
    pub request: HttpRequest,
    /// Set when the tab was opened from (or saved to) a collection.
    pub origin: Option<RequestOrigin>,
    /// Edited since last saved to its collection.
    pub dirty: bool,
}

impl RequestTab {
    #[must_use]
    pub fn new(title: impl Into<String>, request: HttpRequest) -> Self {
        Self {
            id: TabId::new(),
            title: title.into(),
            request,
            origin: None,
            dirty: false,
        }
    }
}

/// Everything needed to restore the request area after restart.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceState {
    pub tabs: Vec<RequestTab>,
    pub active_tab: Option<TabId>,
    pub active_environment: Option<EnvironmentId>,
    /// Current project folder (absolute path), if any.
    pub project: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub theme: Theme,
    /// Sidebar width in px; 0 means "use the default".
    pub sidebar_width: f64,
    /// Maximum history entries kept (0 = default of 500).
    pub history_limit: u32,
}
