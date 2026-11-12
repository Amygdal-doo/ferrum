//! What protocol a saved request speaks.

use serde::{Deserialize, Serialize};

/// The kind of a request. Records written before kinds existed have no
/// `kind` field and load as [`RequestKind::Rest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    #[default]
    Rest,
    Graphql,
    Soap,
    Odata,
    Trpc,
    Grpc,
    #[serde(rename = "websocket")]
    WebSocket,
}

impl RequestKind {
    pub const ALL: [Self; 7] = [
        Self::Rest,
        Self::Graphql,
        Self::Soap,
        Self::Odata,
        Self::Trpc,
        Self::WebSocket,
        Self::Grpc,
    ];

    /// Full name, e.g. in the "New request" menu.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rest => "HTTP / REST",
            Self::Graphql => "GraphQL",
            Self::Soap => "SOAP",
            Self::Odata => "OData",
            Self::Trpc => "tRPC",
            Self::Grpc => "gRPC",
            Self::WebSocket => "WebSocket",
        }
    }

    /// Short tree / tab badge. REST requests show their method instead.
    #[must_use]
    pub const fn badge(self) -> &'static str {
        match self {
            Self::Rest => "HTTP",
            Self::Graphql => "GQL",
            Self::Soap => "SOAP",
            Self::Odata => "ODATA",
            Self::Trpc => "TRPC",
            Self::Grpc => "GRPC",
            Self::WebSocket => "WS",
        }
    }

    /// Sent as one HTTP request (variables, auth, cookies, history, runner
    /// and load tests all apply).
    #[must_use]
    pub const fn is_http(self) -> bool {
        !matches!(self, Self::Grpc | Self::WebSocket)
    }

    /// Placeholder of the URL field.
    #[must_use]
    pub const fn url_placeholder(self) -> &'static str {
        match self {
            Self::Rest => "Enter URL, e.g. https://api.example.com/users",
            Self::Graphql => "GraphQL endpoint, e.g. https://api.example.com/graphql",
            Self::Soap => "SOAP endpoint, e.g. https://example.com/service.asmx",
            Self::Odata => "OData service root, e.g. https://services.odata.org/V4/TripPinService",
            Self::Trpc => "tRPC base URL, e.g. http://localhost:3000/api/trpc",
            Self::Grpc => "Server, e.g. localhost:50051 (grpcs:// or https:// for TLS)",
            Self::WebSocket => "ws:// or wss:// URL, e.g. wss://echo.example.com/socket",
        }
    }
}
