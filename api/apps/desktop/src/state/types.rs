//! Small UI-state value types.

use ferrumapi_application::interop::codegen::CodeTarget;
use ferrumapi_domain::{
    Collection, CollectionId, CookieJar, Environment, HttpResponse, ItemId, RequestKind, TabId,
};
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
    /// GraphQL query + schema panel.
    Query,
    /// GraphQL variables.
    Variables,
    /// SOAP envelope + WSDL.
    Envelope,
    /// OData query builder.
    Builder,
    /// tRPC procedure + input.
    Procedure,
    /// WebSocket composer / gRPC method + message.
    Message,
}

impl RequestSection {
    /// The sub-tabs of a request of `kind`, in order.
    #[must_use]
    pub const fn for_kind(kind: RequestKind) -> &'static [Self] {
        use RequestSection as S;
        match kind {
            RequestKind::Rest => &[S::Params, S::Auth, S::Headers, S::Body, S::Settings],
            RequestKind::Graphql => &[S::Query, S::Variables, S::Auth, S::Headers, S::Settings],
            RequestKind::Soap => &[S::Envelope, S::Auth, S::Headers, S::Settings],
            RequestKind::Odata => &[
                S::Builder,
                S::Params,
                S::Auth,
                S::Headers,
                S::Body,
                S::Settings,
            ],
            RequestKind::Trpc => &[S::Procedure, S::Auth, S::Headers, S::Settings],
            RequestKind::WebSocket | RequestKind::Grpc => {
                &[S::Message, S::Headers, S::Auth, S::Settings]
            }
        }
    }

    /// `current` when `kind` has it, else the kind's first section.
    #[must_use]
    pub fn effective(current: Self, kind: RequestKind) -> Self {
        let all = Self::for_kind(kind);
        if all.contains(&current) {
            current
        } else {
            all[0]
        }
    }

    #[must_use]
    pub const fn label(self, kind: RequestKind) -> &'static str {
        match self {
            Self::Params => "Params",
            Self::Auth => "Auth",
            Self::Headers if matches!(kind, RequestKind::Grpc) => "Metadata",
            Self::Headers => "Headers",
            Self::Body => "Body",
            Self::Settings => "Settings",
            Self::Query | Self::Builder => "Query",
            Self::Variables => "Variables",
            Self::Envelope => "Envelope",
            Self::Procedure => "Procedure",
            Self::Message => "Message",
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
    /// tRPC: `result.data` unwrapped.
    Data,
}

/// Find-in-response (⌘F) state. Matches are recomputed from the displayed
/// body; `current` indexes them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResponseSearch {
    pub query: String,
    pub case_sensitive: bool,
    pub current: usize,
    /// Scroll the current match into view on the next render.
    pub reveal: bool,
    /// Move keyboard focus to the search field on the next render.
    pub focus: bool,
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
    /// The cookie jar as last loaded from the services.
    Cookies(Box<CookieJar>),
    /// Progress of `AppState::runner`.
    Runner,
    /// Mock server of a collection (`AppState::mocks`).
    MockServer(CollectionId),
    /// The webhook inspector (`AppState::webhooks`).
    Webhooks,
    /// Pick the kind of a new request added to (collection, folder).
    NewRequest {
        target: (CollectionId, Option<ItemId>),
    },
    /// The load test (`AppState::load_test`).
    LoadTest,
}
