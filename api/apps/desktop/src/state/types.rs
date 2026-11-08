//! Small UI-state value types.

use ferrumapi_application::interop::codegen::CodeTarget;
use ferrumapi_domain::{Collection, CollectionId, Environment, HttpResponse, ItemId, TabId};
use std::time::Instant;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseView {
    Body,
    Headers,
    Cookies,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyMode {
    Pretty,
    Raw,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPane {
    Auth,
    Variables,
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

/// The open modal and its working copy (drafts are applied on Save).
#[derive(Debug, Clone, PartialEq)]
pub enum Dialog {
    /// Save an unsaved tab into a collection. `target` is the chosen
    /// (collection, folder) destination.
    SaveRequest {
        tab_id: TabId,
        name: String,
        new_collection: String,
        target: Option<(CollectionId, Option<ItemId>)>,
    },
    /// Delete a collection (`item: None`) or a folder/request in it.
    DeleteItem {
        collection: CollectionId,
        item: Option<ItemId>,
        name: String,
    },
    CollectionSettings {
        draft: Box<Collection>,
        pane: SettingsPane,
    },
    EditEnvironment(Box<Environment>),
    DeleteEnvironment(ferrumapi_domain::EnvironmentId, String),
    ClearHistory,
    Import {
        curl: String,
        error: Option<String>,
    },
    Code {
        tab_id: TabId,
        target: CodeTarget,
    },
}
