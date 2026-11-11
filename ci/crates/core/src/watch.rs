//! When to poll next, and which watched runs just finished.

use crate::model::{RunState, WorkflowRun};
use chrono::{DateTime, Utc};
use ferrumgit_forge::RateState;
use std::collections::HashMap;
use std::time::Duration;

/// Poll interval while anything is queued or running.
pub const ACTIVE_INTERVAL: Duration = Duration::from_secs(12);
/// Poll interval when everything has finished.
pub const IDLE_INTERVAL: Duration = Duration::from_secs(60);
/// Below this many requests left, polling slows down four times.
pub const LOW_REMAINING: u64 = 200;

/// The wait before the next automatic refresh.
#[must_use]
pub fn next_refresh(active: bool, rate: Option<RateState>, now_unix: i64) -> Duration {
    let base = if active {
        ACTIVE_INTERVAL
    } else {
        IDLE_INTERVAL
    };
    match rate {
        Some(RateState {
            remaining: 0,
            reset_at: Some(at),
        }) if at > now_unix => Duration::from_secs(u64::try_from(at - now_unix).unwrap_or(0) + 1),
        Some(r) if r.remaining < LOW_REMAINING => base * 4,
        _ => base,
    }
}

/// A watched run that has finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finished {
    pub run_id: u64,
    pub title: String,
    pub workflow: String,
    pub state: RunState,
    pub url: String,
}

impl Finished {
    /// `CI #42 succeeded` style headline.
    #[must_use]
    pub fn headline(&self) -> String {
        let verb = match self.state {
            RunState::Success => "succeeded",
            RunState::Failure => "failed",
            RunState::Cancelled => "was cancelled",
            RunState::Skipped => "was skipped",
            RunState::ActionRequired => "needs approval",
            RunState::Queued | RunState::InProgress => "is running",
        };
        format!("{} {verb}", self.workflow)
    }
}

/// A `workflow_dispatch` the user just sent; its run gets watched once it shows up.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Expected {
    workflow_id: u64,
    branch: String,
    since: DateTime<Utc>,
}

/// Runs the user asked to be told about.
#[derive(Debug, Default)]
pub struct Watcher {
    runs: HashMap<u64, RunState>,
    expected: Vec<Expected>,
}

impl Watcher {
    pub fn watch(&mut self, run: &WorkflowRun) {
        self.runs.insert(run.id, run.state());
    }

    /// Watches by id; its state is learnt on the next update.
    pub fn watch_id(&mut self, run_id: u64) {
        self.runs.entry(run_id).or_insert(RunState::Queued);
    }

    pub fn unwatch(&mut self, run_id: u64) {
        self.runs.remove(&run_id);
    }

    #[must_use]
    pub fn is_watching(&self, run_id: u64) -> bool {
        self.runs.contains_key(&run_id)
    }

    /// Watched run ids, smallest first.
    #[must_use]
    pub fn ids(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = self.runs.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty() && self.expected.is_empty()
    }

    /// Watches the next `workflow_dispatch` run of `workflow_id` on `branch`.
    pub fn expect_dispatch(&mut self, workflow_id: u64, branch: &str, now: DateTime<Utc>) {
        self.expected.push(Expected {
            workflow_id,
            branch: branch.to_owned(),
            since: now - chrono::Duration::seconds(10),
        });
    }

    /// Feeds fresh runs in; returns the watched ones that just finished
    /// (they stop being watched).
    pub fn observe(&mut self, runs: &[WorkflowRun]) -> Vec<Finished> {
        let mut done = Vec::new();
        for run in runs {
            if let Some(i) = self.expected.iter().position(|e| {
                e.workflow_id == run.workflow_id
                    && run.event == "workflow_dispatch"
                    && run.branch() == e.branch
                    && run.created_at >= e.since
            }) {
                self.expected.remove(i);
                self.runs.entry(run.id).or_insert(RunState::Queued);
            }
            let Some(before) = self.runs.get(&run.id).copied() else {
                continue;
            };
            let now = run.state();
            if !now.is_active() && (before.is_active() || before != now) {
                self.runs.remove(&run.id);
                done.push(Finished {
                    run_id: run.id,
                    title: run.title(),
                    workflow: format!("{} #{}", run.workflow_name(), run.run_number),
                    state: now,
                    url: run.html_url.clone(),
                });
            } else {
                self.runs.insert(run.id, now);
            }
        }
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(id: u64, status: &str, conclusion: Option<&str>) -> WorkflowRun {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": "CI", "run_number": 7, "workflow_id": 3,
            "event": "workflow_dispatch", "head_branch": "main",
            "status": status, "conclusion": conclusion,
            "display_title": "Fix things\n\nbody",
            "created_at": "2024-05-01T10:00:00Z", "updated_at": "2024-05-01T10:01:00Z",
        }))
        .expect("run")
    }

    #[test]
    fn reports_watched_runs_once_when_they_finish() {
        let mut w = Watcher::default();
        w.watch(&run(1, "in_progress", None));
        assert!(
            w.observe(&[
                run(1, "in_progress", None),
                run(2, "completed", Some("failure"))
            ])
            .is_empty()
        );
        let done = w.observe(&[run(1, "completed", Some("failure"))]);
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].headline(), "CI #7 failed");
        assert_eq!(done[0].title, "Fix things");
        assert!(
            w.observe(&[run(1, "completed", Some("failure"))])
                .is_empty()
        );
        assert!(w.is_empty());
    }

    #[test]
    fn rerun_of_a_finished_run_is_reported() {
        let mut w = Watcher::default();
        w.watch_id(5);
        assert!(w.observe(&[run(5, "queued", None)]).is_empty());
        assert_eq!(w.observe(&[run(5, "completed", Some("success"))]).len(), 1);
    }

    #[test]
    fn dispatched_runs_are_picked_up() {
        let mut w = Watcher::default();
        let now: DateTime<Utc> = "2024-05-01T10:00:05Z".parse().expect("now");
        w.expect_dispatch(3, "main", now);
        assert!(w.observe(&[run(9, "queued", None)]).is_empty());
        assert!(w.is_watching(9));
        assert_eq!(w.observe(&[run(9, "completed", Some("success"))]).len(), 1);
    }

    #[test]
    fn polling_slows_down_with_low_rate_limit() {
        assert_eq!(next_refresh(true, None, 0), ACTIVE_INTERVAL);
        assert_eq!(next_refresh(false, None, 0), IDLE_INTERVAL);
        let low = RateState {
            remaining: 50,
            reset_at: Some(100),
        };
        assert_eq!(next_refresh(true, Some(low), 0), ACTIVE_INTERVAL * 4);
        let out = RateState {
            remaining: 0,
            reset_at: Some(100),
        };
        assert_eq!(next_refresh(true, Some(out), 40), Duration::from_secs(61));
        assert_eq!(next_refresh(true, Some(out), 200), ACTIVE_INTERVAL * 4);
    }
}
