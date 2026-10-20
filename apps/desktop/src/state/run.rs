use ferrum_domain::{DbError, ExecutionId, ProfileId, QueryEvent, QueryOutcome, QueryState};
use ferrum_result_grid_core::{ResultBuffer, WIDTH_SAMPLE_ROWS, estimate_widths};
use std::time::{Duration, Instant};

/// The latest execution for one editor tab.
#[derive(Debug, Clone, PartialEq)]
pub struct TabRun {
    pub execution_id: ExecutionId,
    pub profile_id: ProfileId,
    pub sql: String,
    pub state: QueryState,
    pub buffer: ResultBuffer,
    pub outcome: Option<QueryOutcome>,
    pub error: Option<DbError>,
    pub messages: Vec<String>,
    pub started_at: Instant,
    pub elapsed: Option<Duration>,
    /// Pixel widths per column, estimated from the first rows.
    pub col_widths: Vec<f64>,
    /// Informational note shown above the grid (e.g. non-deterministic order).
    pub note: Option<String>,
}

impl TabRun {
    #[must_use]
    pub fn new(
        execution_id: ExecutionId,
        profile_id: ProfileId,
        sql: String,
        max_rows: usize,
    ) -> Self {
        Self {
            execution_id,
            profile_id,
            sql,
            state: QueryState::Created,
            buffer: ResultBuffer::new(max_rows),
            outcome: None,
            error: None,
            messages: Vec::new(),
            started_at: Instant::now(),
            elapsed: None,
            col_widths: Vec::new(),
            note: None,
        }
    }

    /// Applies one event; invalid transitions (e.g. after a terminal state) are dropped.
    pub fn apply(&mut self, event: QueryEvent) {
        let Some(next) = self.state.apply(&event) else {
            tracing::debug!(execution = %self.execution_id, "ignoring out-of-order query event");
            return;
        };
        self.state = next;
        match event {
            QueryEvent::Started => {}
            QueryEvent::Metadata(columns) => {
                self.col_widths = estimate_widths(&columns, std::iter::empty());
                self.buffer.set_columns(columns);
            }
            QueryEvent::Rows(rows) => {
                let sampled_before = self.buffer.len();
                self.buffer.push_batch(rows);
                // Re-estimate only while still inside the sample window.
                if sampled_before < WIDTH_SAMPLE_ROWS {
                    let sample = self.buffer.rows(0..WIDTH_SAMPLE_ROWS).map(|(_, r)| r);
                    self.col_widths = estimate_widths(self.buffer.columns(), sample);
                }
            }
            QueryEvent::Message(m) => self.messages.push(m),
            QueryEvent::Completed(outcome) => {
                if outcome.truncated {
                    self.buffer.mark_truncated();
                }
                self.elapsed = Some(outcome.elapsed);
                self.outcome = Some(outcome);
            }
            QueryEvent::Cancelled { elapsed } | QueryEvent::TimedOut { elapsed } => {
                self.elapsed = Some(elapsed);
            }
            QueryEvent::Failed(e) => {
                self.elapsed = Some(self.started_at.elapsed());
                self.error = Some(e);
            }
        }
    }

    /// Marks a run that failed before the driver produced any event.
    pub fn fail(&mut self, error: DbError) {
        self.apply(QueryEvent::Failed(error));
    }

    #[must_use]
    pub fn status_label(&self) -> String {
        let ms = |d: Duration| format!("{} ms", d.as_millis());
        match self.state {
            QueryState::Created | QueryState::Running => "Running…".into(),
            QueryState::Streaming => format!("Fetching… {} rows", self.buffer.len()),
            QueryState::Completed => match &self.outcome {
                Some(o) if o.rows_affected.is_some() && self.buffer.columns().is_empty() => {
                    format!(
                        "{} rows affected · {}",
                        o.rows_affected.unwrap_or(0),
                        ms(o.elapsed)
                    )
                }
                Some(o) => {
                    let more = if o.truncated { " (limit reached)" } else { "" };
                    format!("{} rows{more} · {}", self.buffer.len(), ms(o.elapsed))
                }
                None => "Completed".into(),
            },
            QueryState::Cancelled => {
                format!("Cancelled · {}", self.elapsed.map(ms).unwrap_or_default())
            }
            QueryState::TimedOut => {
                format!("Timed out · {}", self.elapsed.map(ms).unwrap_or_default())
            }
            QueryState::Failed => "Failed".into(),
        }
    }
}
