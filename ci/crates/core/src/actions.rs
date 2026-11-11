//! GitHub Actions REST endpoints, sent through FerrumGIT's forge [`Api`]
//! (auth headers, rate-limit tracking and error messages come from there).

use crate::dispatch::{DispatchSpec, parse_dispatch};
use crate::model::{Job, JobsPage, RunsPage, Workflow, WorkflowRun, WorkflowsPage};
use base64::Engine as _;
use ferrumgit_forge::api::encode;
use ferrumgit_forge::{Api, ForgeError, ForgeRepo, ForgeResult, Method, RateState};
use serde::Deserialize;
use std::collections::BTreeMap;

/// The `status` query filter for runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusFilter {
    Queued,
    InProgress,
    Success,
    Failure,
    Cancelled,
    Skipped,
}

impl StatusFilter {
    pub const ALL: [Self; 6] = [
        Self::Queued,
        Self::InProgress,
        Self::Success,
        Self::Failure,
        Self::Cancelled,
        Self::Skipped,
    ];

    #[must_use]
    pub const fn api(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::InProgress => "in_progress",
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
        }
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
        }
    }
}

/// Common trigger events offered in the event filter.
pub const EVENTS: [&str; 6] = [
    "push",
    "pull_request",
    "workflow_dispatch",
    "schedule",
    "release",
    "merge_group",
];

/// Which runs to list. Empty / `None` fields mean "any".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunFilter {
    pub workflow_id: Option<u64>,
    pub branch: Option<String>,
    pub event: Option<String>,
    pub status: Option<StatusFilter>,
}

impl RunFilter {
    /// Path (relative to `/repos/{owner}/{repo}`) with the query string.
    #[must_use]
    pub fn path(&self, per_page: u32) -> String {
        let mut path = match self.workflow_id {
            Some(id) => format!("/actions/workflows/{id}/runs"),
            None => "/actions/runs".to_owned(),
        };
        path.push_str(&format!("?per_page={per_page}"));
        let mut add = |key: &str, value: &str| {
            let value = value.trim();
            if !value.is_empty() {
                path.push_str(&format!("&{key}={}", encode(value)));
            }
        };
        add("branch", self.branch.as_deref().unwrap_or(""));
        add("event", self.event.as_deref().unwrap_or(""));
        add("status", self.status.map_or("", StatusFilter::api));
        path
    }
}

/// GitHub Actions for one repository.
#[derive(Debug, Clone)]
pub struct Actions {
    api: Api,
    base: String,
}

#[derive(Deserialize)]
struct Branch {
    name: String,
}

#[derive(Deserialize)]
struct RepoInfo {
    #[serde(default)]
    default_branch: String,
}

#[derive(Deserialize)]
struct FileContent {
    #[serde(default)]
    content: String,
    #[serde(default)]
    encoding: String,
}

impl Actions {
    #[must_use]
    pub fn new(repo: &ForgeRepo, api: Api) -> Self {
        Self {
            api,
            base: format!("/repos/{}/{}", encode(&repo.owner), encode(&repo.name)),
        }
    }

    #[must_use]
    pub fn api(&self) -> &Api {
        &self.api
    }

    #[must_use]
    pub fn rate(&self) -> Option<RateState> {
        self.api.rate()
    }

    pub async fn runs(&self, filter: &RunFilter, per_page: u32) -> ForgeResult<RunsPage> {
        self.api
            .get(format!("{}{}", self.base, filter.path(per_page)))
            .await
    }

    pub async fn run(&self, run_id: u64) -> ForgeResult<WorkflowRun> {
        self.api
            .get(format!("{}/actions/runs/{run_id}", self.base))
            .await
    }

    /// Jobs of the run's latest attempt.
    pub async fn jobs(&self, run_id: u64) -> ForgeResult<Vec<Job>> {
        let page: JobsPage = self
            .api
            .get(format!(
                "{}/actions/runs/{run_id}/jobs?filter=latest&per_page=100",
                self.base
            ))
            .await?;
        Ok(page.jobs)
    }

    /// Plain-text log of one job (GitHub redirects to a short-lived blob URL;
    /// the HTTP client follows it without the token).
    pub async fn job_log(&self, job_id: u64) -> ForgeResult<String> {
        let response = self
            .api
            .request(
                Method::Get,
                format!("{}/actions/jobs/{job_id}/logs", self.base),
                None,
            )
            .await?;
        Ok(response.body)
    }

    pub async fn workflows(&self) -> ForgeResult<Vec<Workflow>> {
        let page: WorkflowsPage = self
            .api
            .get(format!("{}/actions/workflows?per_page=100", self.base))
            .await?;
        Ok(page.workflows)
    }

    /// Re-runs every job, or only the failed ones.
    pub async fn rerun(&self, run_id: u64, failed_only: bool) -> ForgeResult<()> {
        let what = if failed_only {
            "rerun-failed-jobs"
        } else {
            "rerun"
        };
        self.post(format!("{}/actions/runs/{run_id}/{what}", self.base), None)
            .await
    }

    pub async fn cancel(&self, run_id: u64) -> ForgeResult<()> {
        self.post(format!("{}/actions/runs/{run_id}/cancel", self.base), None)
            .await
    }

    /// Starts a `workflow_dispatch` run on `git_ref` (a branch or tag).
    pub async fn dispatch(
        &self,
        workflow_id: u64,
        git_ref: &str,
        inputs: &BTreeMap<String, String>,
    ) -> ForgeResult<()> {
        if git_ref.trim().is_empty() {
            return Err(ForgeError::Invalid(
                "Choose a branch or tag to run on.".into(),
            ));
        }
        let body = serde_json::json!({ "ref": git_ref.trim(), "inputs": inputs });
        self.post(
            format!("{}/actions/workflows/{workflow_id}/dispatches", self.base),
            Some(body),
        )
        .await
    }

    async fn post(&self, path: String, body: Option<serde_json::Value>) -> ForgeResult<()> {
        self.api.request(Method::Post, path, body).await.map(|_| ())
    }

    /// Branch names (first 100).
    pub async fn branches(&self) -> ForgeResult<Vec<String>> {
        let list: Vec<Branch> = self
            .api
            .get(format!("{}/branches?per_page=100", self.base))
            .await?;
        Ok(list.into_iter().map(|b| b.name).collect())
    }

    pub async fn default_branch(&self) -> ForgeResult<String> {
        let info: RepoInfo = self.api.get(&self.base).await?;
        Ok(info.default_branch)
    }

    /// The workflow file at `git_ref`, read for its `workflow_dispatch`
    /// inputs. `None` when the workflow cannot be started by hand.
    pub async fn dispatch_spec(
        &self,
        workflow: &Workflow,
        git_ref: &str,
    ) -> ForgeResult<Option<DispatchSpec>> {
        let path: Vec<String> = workflow.path.split('/').map(encode).collect();
        let file: FileContent = self
            .api
            .get(format!(
                "{}/contents/{}?ref={}",
                self.base,
                path.join("/"),
                encode(git_ref)
            ))
            .await?;
        let text = decode_content(&file)?;
        parse_dispatch(&text).map_err(ForgeError::Invalid)
    }
}

fn decode_content(file: &FileContent) -> ForgeResult<String> {
    if file.encoding != "base64" {
        return Ok(file.content.clone());
    }
    let compact: String = file
        .content
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(compact)
        .map_err(|e| ForgeError::Parse(format!("workflow file: {e}")))?;
    String::from_utf8(bytes).map_err(|e| ForgeError::Parse(format!("workflow file: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_builds_query() {
        assert_eq!(RunFilter::default().path(30), "/actions/runs?per_page=30");
        let f = RunFilter {
            workflow_id: Some(7),
            branch: Some("feature/x y".into()),
            event: Some("push".into()),
            status: Some(StatusFilter::InProgress),
        };
        assert_eq!(
            f.path(10),
            "/actions/workflows/7/runs?per_page=10&branch=feature%2Fx%20y&event=push&status=in_progress"
        );
        let blank = RunFilter {
            branch: Some("  ".into()),
            ..RunFilter::default()
        };
        assert_eq!(blank.path(5), "/actions/runs?per_page=5");
    }

    #[test]
    fn decodes_wrapped_base64() {
        let f = FileContent {
            content: "b246\nIHB1\nc2g=\n".into(),
            encoding: "base64".into(),
        };
        assert_eq!(decode_content(&f).expect("decode"), "on: push");
    }
}
