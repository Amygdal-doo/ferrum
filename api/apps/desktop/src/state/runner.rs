//! Collection-runner UI state: one row per request, updated from
//! [`RunEvent`]s (pure reducer, unit tested).

use ferrumapi_application::{CancellationToken, RunEvent, RunItem, RunOutcome};
use ferrumapi_domain::{CollectionId, HttpMethod, ItemId, RequestOrigin};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowStatus {
    Pending,
    Running,
    Done(RunOutcome),
    /// Not sent because the run was stopped.
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRow {
    pub origin: RequestOrigin,
    pub name: String,
    pub method: HttpMethod,
    pub status: RowStatus,
}

/// Tally of finished rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunCounts {
    pub done: usize,
    pub passed: usize,
    pub failed: usize,
    pub time_ms: u64,
}

#[derive(Debug, Clone)]
pub struct RunnerState {
    /// Distinguishes runs, so events from a replaced run are ignored.
    pub run: u64,
    pub collection: CollectionId,
    pub folder: Option<ItemId>,
    pub title: String,
    pub rows: Vec<RunRow>,
    pub running: bool,
    pub stopped: bool,
    pub cancel: CancellationToken,
}

impl RunnerState {
    #[must_use]
    pub fn new(
        run: u64,
        target: (CollectionId, Option<ItemId>),
        title: String,
        items: &[RunItem],
        cancel: CancellationToken,
    ) -> Self {
        Self {
            run,
            collection: target.0,
            folder: target.1,
            title,
            rows: items
                .iter()
                .map(|item| RunRow {
                    origin: item.origin,
                    name: item.name.clone(),
                    method: item.method,
                    status: RowStatus::Pending,
                })
                .collect(),
            running: true,
            stopped: false,
            cancel,
        }
    }

    pub fn apply(&mut self, event: RunEvent) {
        match event {
            RunEvent::Started(i) => self.set(i, RowStatus::Running),
            RunEvent::Finished(i, outcome) => self.set(i, RowStatus::Done(outcome)),
            RunEvent::Done { stopped } => {
                self.running = false;
                self.stopped = stopped;
                for row in &mut self.rows {
                    if matches!(row.status, RowStatus::Pending | RowStatus::Running) {
                        row.status = RowStatus::Skipped;
                    }
                }
            }
        }
    }

    fn set(&mut self, index: usize, status: RowStatus) {
        if let Some(row) = self.rows.get_mut(index) {
            row.status = status;
        }
    }

    /// Asks the run to stop; the current request is cancelled.
    pub fn stop(&self) {
        self.cancel.cancel();
    }

    #[must_use]
    pub fn counts(&self) -> RunCounts {
        let mut counts = RunCounts::default();
        for row in &self.rows {
            let RowStatus::Done(outcome) = &row.status else {
                continue;
            };
            counts.done += 1;
            if outcome.passed() {
                counts.passed += 1;
            } else {
                counts.failed += 1;
            }
            if let RunOutcome::Response { time_ms, .. } = outcome {
                counts.time_ms += time_ms;
            }
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumapi_application::Prepared;
    use ferrumapi_domain::{HttpRequest, VariableSet};

    fn item(name: &str) -> RunItem {
        RunItem {
            origin: RequestOrigin {
                collection: CollectionId::new(),
                item: ItemId::new(),
            },
            name: name.into(),
            method: HttpMethod::Get,
            prepared: Prepared {
                request: HttpRequest::default(),
                variables: VariableSet::new(),
            },
        }
    }

    fn response(status: u16, time_ms: u64) -> RunOutcome {
        RunOutcome::Response {
            status,
            time_ms,
            passed: (200..300).contains(&status),
        }
    }

    fn runner() -> RunnerState {
        let items = [item("a"), item("b"), item("c")];
        RunnerState::new(
            1,
            (CollectionId::new(), None),
            "C".into(),
            &items,
            CancellationToken::new(),
        )
    }

    #[test]
    fn rows_follow_events_and_counts_tally() {
        let mut r = runner();
        assert!(r.running);
        assert!(r.rows.iter().all(|row| row.status == RowStatus::Pending));
        r.apply(RunEvent::Started(0));
        assert_eq!(r.rows[0].status, RowStatus::Running);
        r.apply(RunEvent::Finished(0, response(200, 10)));
        r.apply(RunEvent::Started(1));
        r.apply(RunEvent::Finished(1, response(500, 5)));
        r.apply(RunEvent::Started(2));
        r.apply(RunEvent::Finished(2, RunOutcome::Error("refused".into())));
        r.apply(RunEvent::Done { stopped: false });
        assert!(!r.running && !r.stopped);
        assert_eq!(
            r.counts(),
            RunCounts {
                done: 3,
                passed: 1,
                failed: 2,
                time_ms: 15
            }
        );
    }

    #[test]
    fn stopping_marks_unfinished_rows_skipped() {
        let mut r = runner();
        r.apply(RunEvent::Started(0));
        r.apply(RunEvent::Finished(0, response(204, 1)));
        r.apply(RunEvent::Started(1));
        r.stop();
        assert!(r.cancel.is_cancelled());
        r.apply(RunEvent::Done { stopped: true });
        assert!(r.stopped && !r.running);
        assert_eq!(r.rows[1].status, RowStatus::Skipped);
        assert_eq!(r.rows[2].status, RowStatus::Skipped);
        assert_eq!(r.counts().done, 1);
        // Out-of-range events are ignored.
        r.apply(RunEvent::Started(9));
    }
}
