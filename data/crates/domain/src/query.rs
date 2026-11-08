//! Query requests, streamed events and the execution lifecycle.

use crate::{CellValue, ColumnMeta, DbError, EditorTabId, ExecutionId, ProfileId};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Client-side limits. These never rewrite user SQL: `max_rows` stops
/// fetching and marks the result as truncated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryLimits {
    pub max_rows: usize,
    pub batch_rows: usize,
    pub timeout: Option<Duration>,
}

impl Default for QueryLimits {
    fn default() -> Self {
        Self {
            max_rows: 10_000,
            batch_rows: 250,
            timeout: Some(Duration::from_secs(300)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct QueryRequest {
    pub execution_id: ExecutionId,
    pub profile_id: ProfileId,
    pub editor_id: Option<EditorTabId>,
    pub sql: String,
    pub limits: QueryLimits,
}

pub type RowBatch = Vec<Vec<CellValue>>;

/// Terminal summary of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryOutcome {
    pub rows_returned: usize,
    pub rows_affected: Option<u64>,
    pub truncated: bool,
    pub elapsed: Duration,
}

/// Events emitted by a driver for a single execution, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryEvent {
    Started,
    Metadata(Vec<ColumnMeta>),
    Rows(RowBatch),
    Message(String),
    Completed(QueryOutcome),
    Cancelled { elapsed: Duration },
    TimedOut { elapsed: Duration },
    Failed(DbError),
}

impl QueryEvent {
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed(_) | Self::Cancelled { .. } | Self::TimedOut { .. } | Self::Failed(_)
        )
    }
}

/// Event stamped with its execution and a monotonically increasing sequence.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionEnvelope {
    pub execution_id: ExecutionId,
    pub seq: u64,
    pub event: QueryEvent,
}

/// Lifecycle: Created → Running → Streaming → Completed, with terminal
/// branches Cancelled / Failed / TimedOut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QueryState {
    #[default]
    Created,
    Running,
    Streaming,
    Completed,
    Cancelled,
    Failed,
    TimedOut,
}

impl QueryState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Cancelled | Self::Failed | Self::TimedOut
        )
    }

    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::Streaming)
    }

    /// Applies an event. Returns `None` for an invalid transition, e.g. any
    /// event after a terminal state; callers must ignore such events.
    #[must_use]
    pub fn apply(self, event: &QueryEvent) -> Option<Self> {
        use QueryEvent as E;
        if self.is_terminal() {
            return None;
        }
        Some(match (self, event) {
            (Self::Created, E::Started) => Self::Running,
            (Self::Running | Self::Streaming, E::Metadata(_) | E::Rows(_)) => Self::Streaming,
            (Self::Running | Self::Streaming, E::Message(_)) => self,
            (_, E::Completed(_)) if self != Self::Created => Self::Completed,
            (_, E::Cancelled { .. }) => Self::Cancelled,
            (_, E::TimedOut { .. }) => Self::TimedOut,
            (_, E::Failed(_)) => Self::Failed,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome() -> QueryOutcome {
        QueryOutcome {
            rows_returned: 0,
            rows_affected: None,
            truncated: false,
            elapsed: Duration::ZERO,
        }
    }

    #[test]
    fn happy_path() {
        let s = QueryState::Created;
        let s = s.apply(&QueryEvent::Started).expect("started");
        let s = s.apply(&QueryEvent::Metadata(vec![])).expect("meta");
        let s = s.apply(&QueryEvent::Rows(vec![])).expect("rows");
        let s = s.apply(&QueryEvent::Completed(outcome())).expect("done");
        assert_eq!(s, QueryState::Completed);
    }

    #[test]
    fn events_after_terminal_are_rejected() {
        let s = QueryState::Cancelled;
        assert_eq!(s.apply(&QueryEvent::Rows(vec![])), None);
        assert_eq!(s.apply(&QueryEvent::Completed(outcome())), None);
    }

    #[test]
    fn rows_before_start_are_rejected() {
        assert_eq!(QueryState::Created.apply(&QueryEvent::Rows(vec![])), None);
        assert_eq!(
            QueryState::Created.apply(&QueryEvent::Completed(outcome())),
            None
        );
    }

    #[test]
    fn cancel_and_failure_allowed_from_any_live_state() {
        for s in [
            QueryState::Created,
            QueryState::Running,
            QueryState::Streaming,
        ] {
            let c = QueryEvent::Cancelled {
                elapsed: Duration::ZERO,
            };
            assert_eq!(s.apply(&c), Some(QueryState::Cancelled));
            let f = QueryEvent::Failed(DbError::cancelled());
            assert_eq!(s.apply(&f), Some(QueryState::Failed));
        }
    }
}
