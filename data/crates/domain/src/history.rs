//! Query history: one record per executed statement, per connection.

use crate::{ProfileId, QueryEvent};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// How a recorded run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryStatus {
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

impl HistoryStatus {
    /// Stable storage name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "timed_out" => Self::TimedOut,
            _ => return None,
        })
    }
}

/// One executed statement. Holds the query text and a summary, never row values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Assigned by the store; ignored when recording.
    pub id: i64,
    pub profile_id: ProfileId,
    pub sql: String,
    pub executed_at: DateTime<Utc>,
    pub duration: Duration,
    pub status: HistoryStatus,
    /// Rows fetched into the grid (completed runs only).
    pub rows_returned: Option<u64>,
    /// Rows changed by a DML statement, when the engine reports it.
    pub rows_affected: Option<u64>,
    /// Sanitized error message for failed runs.
    pub error: Option<String>,
}

impl HistoryEntry {
    /// Builds the record for a run that ended with `terminal`. Returns `None`
    /// for non-terminal events.
    #[must_use]
    pub fn from_terminal(
        profile_id: ProfileId,
        sql: String,
        executed_at: DateTime<Utc>,
        wall_time: Duration,
        terminal: &QueryEvent,
    ) -> Option<Self> {
        let mut entry = Self {
            id: 0,
            profile_id,
            sql,
            executed_at,
            duration: wall_time,
            status: HistoryStatus::Completed,
            rows_returned: None,
            rows_affected: None,
            error: None,
        };
        match terminal {
            QueryEvent::Completed(o) => {
                entry.duration = o.elapsed;
                entry.rows_returned = Some(o.rows_returned as u64);
                entry.rows_affected = o.rows_affected;
            }
            QueryEvent::Failed(e) => {
                entry.status = HistoryStatus::Failed;
                entry.error = Some(e.message.clone());
            }
            QueryEvent::Cancelled { elapsed } => {
                entry.status = HistoryStatus::Cancelled;
                entry.duration = *elapsed;
            }
            QueryEvent::TimedOut { elapsed } => {
                entry.status = HistoryStatus::TimedOut;
                entry.duration = *elapsed;
            }
            QueryEvent::Started
            | QueryEvent::Metadata(_)
            | QueryEvent::Rows(_)
            | QueryEvent::Message(_) => return None,
        }
        Some(entry)
    }

    /// Short result summary for lists, e.g. `42 rows`, `3 affected`, `Failed`.
    #[must_use]
    pub fn summary(&self) -> String {
        match self.status {
            HistoryStatus::Completed => match (self.rows_affected, self.rows_returned) {
                (Some(n), Some(0) | None) => format!("{n} affected"),
                (_, Some(n)) => format!("{n} rows"),
                (None, None) => "Completed".into(),
            },
            HistoryStatus::Failed => "Failed".into(),
            HistoryStatus::Cancelled => "Cancelled".into(),
            HistoryStatus::TimedOut => "Timed out".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DbError, QueryOutcome};

    fn entry(event: &QueryEvent) -> Option<HistoryEntry> {
        HistoryEntry::from_terminal(
            ProfileId::new(),
            "select 1".into(),
            Utc::now(),
            Duration::from_millis(900),
            event,
        )
    }

    #[test]
    fn status_names_round_trip() {
        for s in [
            HistoryStatus::Completed,
            HistoryStatus::Failed,
            HistoryStatus::Cancelled,
            HistoryStatus::TimedOut,
        ] {
            assert_eq!(HistoryStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(HistoryStatus::parse("bogus"), None);
    }

    #[test]
    fn completed_run_records_driver_elapsed_and_counts() {
        let e = entry(&QueryEvent::Completed(QueryOutcome {
            rows_returned: 0,
            rows_affected: Some(3),
            truncated: false,
            elapsed: Duration::from_millis(12),
        }))
        .expect("terminal");
        assert_eq!(e.status, HistoryStatus::Completed);
        assert_eq!(e.duration, Duration::from_millis(12));
        assert_eq!(e.summary(), "3 affected");

        let e = entry(&QueryEvent::Completed(QueryOutcome {
            rows_returned: 42,
            rows_affected: None,
            truncated: true,
            elapsed: Duration::ZERO,
        }))
        .expect("terminal");
        assert_eq!(e.summary(), "42 rows");
    }

    #[test]
    fn failure_keeps_message_and_wall_time() {
        let e = entry(&QueryEvent::Failed(DbError::cancelled())).expect("terminal");
        assert_eq!(e.status, HistoryStatus::Failed);
        assert_eq!(e.duration, Duration::from_millis(900));
        assert!(e.error.is_some());
        assert_eq!(e.rows_returned, None);
    }

    #[test]
    fn non_terminal_events_are_not_recorded() {
        assert_eq!(entry(&QueryEvent::Started), None);
        assert_eq!(entry(&QueryEvent::Rows(vec![])), None);
    }
}
