//! GitHub Actions types (runs, jobs, steps, workflows) as the REST API
//! returns them, plus the status each one boils down to.

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// What a run, job or step is doing or how it ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RunState {
    Queued,
    InProgress,
    Success,
    Failure,
    Cancelled,
    Skipped,
    /// Waiting for a reviewer / environment approval.
    ActionRequired,
}

impl RunState {
    /// From the API's `status` + `conclusion` pair.
    #[must_use]
    pub fn from_api(status: Option<&str>, conclusion: Option<&str>) -> Self {
        match status.unwrap_or("completed") {
            "in_progress" => Self::InProgress,
            "queued" | "waiting" | "requested" | "pending" => Self::Queued,
            _ => match conclusion.unwrap_or("") {
                "success" => Self::Success,
                "failure" | "timed_out" | "startup_failure" => Self::Failure,
                "cancelled" => Self::Cancelled,
                "action_required" => Self::ActionRequired,
                // skipped, neutral, stale, or no conclusion yet.
                _ => Self::Skipped,
            },
        }
    }

    /// Still queued or running.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::InProgress)
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::InProgress => "In progress",
            Self::Success => "Success",
            Self::Failure => "Failure",
            Self::Cancelled => "Cancelled",
            Self::Skipped => "Skipped",
            Self::ActionRequired => "Action required",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct Actor {
    pub login: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct HeadCommit {
    #[serde(default)]
    pub message: String,
}

/// One workflow run.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WorkflowRun {
    pub id: u64,
    /// Workflow name.
    #[serde(default)]
    pub name: Option<String>,
    /// Commit message / PR title GitHub shows for the run.
    #[serde(default)]
    pub display_title: String,
    #[serde(default)]
    pub head_branch: Option<String>,
    #[serde(default)]
    pub head_sha: String,
    #[serde(default)]
    pub event: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub conclusion: Option<String>,
    #[serde(default)]
    pub run_number: u64,
    #[serde(default = "one")]
    pub run_attempt: u64,
    #[serde(default)]
    pub workflow_id: u64,
    #[serde(default)]
    pub html_url: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub run_started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub actor: Option<Actor>,
    #[serde(default)]
    pub head_commit: Option<HeadCommit>,
}

const fn one() -> u64 {
    1
}

impl WorkflowRun {
    #[must_use]
    pub fn state(&self) -> RunState {
        RunState::from_api(self.status.as_deref(), self.conclusion.as_deref())
    }

    /// The first line of the commit message (or the display title).
    #[must_use]
    pub fn title(&self) -> String {
        let text = if self.display_title.is_empty() {
            self.head_commit.as_ref().map_or("", |c| c.message.as_str())
        } else {
            self.display_title.as_str()
        };
        text.lines().next().unwrap_or("").trim().to_owned()
    }

    #[must_use]
    pub fn workflow_name(&self) -> &str {
        self.name.as_deref().unwrap_or("Workflow")
    }

    #[must_use]
    pub fn actor(&self) -> &str {
        self.actor.as_ref().map_or("", |a| a.login.as_str())
    }

    #[must_use]
    pub fn branch(&self) -> &str {
        self.head_branch.as_deref().unwrap_or("")
    }

    /// Seconds the run took (or has been running, measured to `now`).
    #[must_use]
    pub fn duration_secs(&self, now: DateTime<Utc>) -> i64 {
        let start = self.run_started_at.unwrap_or(self.created_at);
        let end = if self.state().is_active() {
            now
        } else {
            self.updated_at
        };
        (end - start).num_seconds().max(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RunsPage {
    #[serde(default)]
    pub total_count: u64,
    #[serde(default)]
    pub workflow_runs: Vec<WorkflowRun>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Step {
    pub name: String,
    #[serde(default)]
    pub number: u64,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub conclusion: Option<String>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
}

impl Step {
    #[must_use]
    pub fn state(&self) -> RunState {
        RunState::from_api(self.status.as_deref(), self.conclusion.as_deref())
    }

    #[must_use]
    pub fn duration_secs(&self, now: DateTime<Utc>) -> Option<i64> {
        span(self.started_at, self.completed_at, self.state(), now)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Job {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub conclusion: Option<String>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub steps: Vec<Step>,
}

impl Job {
    #[must_use]
    pub fn state(&self) -> RunState {
        RunState::from_api(self.status.as_deref(), self.conclusion.as_deref())
    }

    #[must_use]
    pub fn duration_secs(&self, now: DateTime<Utc>) -> Option<i64> {
        span(self.started_at, self.completed_at, self.state(), now)
    }
}

fn span(
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    state: RunState,
    now: DateTime<Utc>,
) -> Option<i64> {
    let start = start?;
    let end = match end {
        Some(end) => end,
        None if state.is_active() => now,
        None => return None,
    };
    Some((end - start).num_seconds().max(0))
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct JobsPage {
    #[serde(default)]
    pub jobs: Vec<Job>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Workflow {
    pub id: u64,
    pub name: String,
    /// `.github/workflows/ci.yml`.
    pub path: String,
    /// `active`, `disabled_manually`, …
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub html_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct WorkflowsPage {
    #[serde(default)]
    pub workflows: Vec<Workflow>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_from_status_and_conclusion() {
        use RunState::*;
        let s = |a, b| RunState::from_api(a, b);
        assert_eq!(s(Some("queued"), None), Queued);
        assert_eq!(s(Some("waiting"), None), Queued);
        assert_eq!(s(Some("in_progress"), None), InProgress);
        assert_eq!(s(Some("completed"), Some("success")), Success);
        assert_eq!(s(Some("completed"), Some("timed_out")), Failure);
        assert_eq!(s(Some("completed"), Some("cancelled")), Cancelled);
        assert_eq!(s(Some("completed"), Some("neutral")), Skipped);
        assert_eq!(
            s(Some("completed"), Some("action_required")),
            ActionRequired
        );
        assert!(InProgress.is_active() && !Failure.is_active());
    }

    #[test]
    fn spans_measure_to_now_while_running() {
        let t = |s: &str| s.parse::<DateTime<Utc>>().expect("time");
        let now = t("2024-05-01T10:05:00Z");
        assert_eq!(
            span(
                Some(t("2024-05-01T10:00:00Z")),
                None,
                RunState::InProgress,
                now
            ),
            Some(300)
        );
        assert_eq!(
            span(
                Some(t("2024-05-01T10:00:00Z")),
                None,
                RunState::Skipped,
                now
            ),
            None
        );
        assert_eq!(span(None, None, RunState::Queued, now), None);
    }
}
