//! Sent-request history.

use crate::collection::RequestOrigin;
use crate::ids::HistoryId;
use crate::request::HttpRequest;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One sent request. `request` is stored as authored (before variable
/// substitution) so reopening it keeps the `{{variables}}`. The response body
/// is not stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: HistoryId,
    pub sent_at: DateTime<Utc>,
    pub request: HttpRequest,
    #[serde(default)]
    pub origin: Option<RequestOrigin>,
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default)]
    pub time_ms: Option<u64>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    /// Sanitized error message when the request failed.
    #[serde(default)]
    pub error: Option<String>,
}

impl HistoryEntry {
    #[must_use]
    pub fn new(request: HttpRequest, origin: Option<RequestOrigin>) -> Self {
        Self {
            id: HistoryId::new(),
            sent_at: Utc::now(),
            request,
            origin,
            status: None,
            time_ms: None,
            size_bytes: None,
            error: None,
        }
    }
}
