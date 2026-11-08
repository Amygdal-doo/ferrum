//! Small UI-state value types.

use ferrumapi_domain::{CollectionId, EnvironmentId, HttpResponse, ItemId, TabId};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub message: String,
    pub error: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarView {
    Collections,
    Environments,
    History,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestSection {
    Params,
    Auth,
    Headers,
    Body,
    Settings,
}

impl RequestSection {
    pub const ALL: [Self; 5] = [
        Self::Params,
        Self::Auth,
        Self::Headers,
        Self::Body,
        Self::Settings,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Params => "Params",
            Self::Auth => "Auth",
            Self::Headers => "Headers",
            Self::Body => "Body",
            Self::Settings => "Settings",
        }
    }
}

/// Remount counters for uncontrolled inputs. Inputs use `initial_value`
/// (avoids lost keystrokes); bumping a counter re-keys them so a value
/// changed from elsewhere (e.g. URL ↔ params sync) is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EditorRevs {
    pub url: u64,
    pub params: u64,
    pub body: u64,
}

/// The in-memory response state of one tab.
#[derive(Debug, Clone, PartialEq)]
pub enum TabResponse {
    Loading { run: u64, started: Instant },
    Done(Box<HttpResponse>),
    Failed(String),
    Cancelled,
}

impl TabResponse {
    #[must_use]
    pub const fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Dialog {
    /// Save an unsaved tab into a collection.
    SaveRequest(TabId),
    /// Delete a collection (`item: None`) or a folder/request in it.
    DeleteItem {
        collection: CollectionId,
        item: Option<ItemId>,
        name: String,
    },
    CollectionSettings(CollectionId),
    EditEnvironment(EnvironmentId),
    DeleteEnvironment(EnvironmentId, String),
    ClearHistory,
    Import,
    Code(TabId),
}
