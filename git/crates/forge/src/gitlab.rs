//! GitLab REST v4 (gitlab.com and self-managed).
//!
//! GitLab v4 has no REST call that sets the "requested changes" reviewer
//! state on every edition, so "Request changes" withdraws the user's approval
//! (if any) and posts the text as a comment marked as requesting changes.

use crate::api::{Api, encode};
use crate::client::{Forge, PrList, STATUS_LOOKUPS, check_new, check_review_body};
use crate::error::{ForgeError, ForgeResult};
use crate::model::{
    Check, CheckState, MergeMethod, NewPullRequest, PrCommit, PrDetail, PrFilter, PrState,
    PullRequest, ReviewDecision, ReviewEvent, TimelineItem, TimelineKind, aggregate_checks,
    sort_by_updated,
};
use crate::remote::ForgeRepo;
use crate::transport::Method;
use async_trait::async_trait;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;

pub struct GitLab {
    repo: ForgeRepo,
    api: Api,
}

impl GitLab {
    #[must_use]
    pub fn new(repo: ForgeRepo, api: Api) -> Self {
        Self { repo, api }
    }

    fn project(&self) -> String {
        format!("/projects/{}", encode(&self.repo.path()))
    }

    fn mr(&self, iid: u64) -> String {
        format!("{}/merge_requests/{iid}", self.project())
    }

    async fn approvals(&self, iid: u64) -> ForgeResult<Approvals> {
        self.api.get(format!("{}/approvals", self.mr(iid))).await
    }

    async fn enrich(&self, mut pr: PullRequest) -> ForgeResult<PullRequest> {
        let (mr, approvals) = futures::try_join!(
            self.api.get::<Mr>(self.mr(pr.number)),
            self.approvals(pr.number)
        )?;
        pr.checks = mr
            .head_pipeline
            .as_ref()
            .map_or(CheckState::None, |p| pipeline_state(&p.status));
        pr.review = decision(&mr, &approvals);
        pr.status_loaded = true;
        Ok(pr)
    }

    async fn note(&self, iid: u64, body: &str) -> ForgeResult<()> {
        self.api
            .send_unit(
                Method::Post,
                &format!("{}/notes", self.mr(iid)),
                json!({ "body": body }),
            )
            .await
    }
}

#[async_trait]
impl Forge for GitLab {
    fn repo(&self) -> &ForgeRepo {
        &self.repo
    }

    async fn viewer(&self) -> ForgeResult<String> {
        Ok(self.api.get::<User>("/user").await?.username)
    }

    async fn list(&self, filter: PrFilter) -> ForgeResult<PrList> {
        let viewer = self.viewer().await?;
        let mut path = format!(
            "{}/merge_requests?state=opened&order_by=updated_at&sort=desc&per_page=50",
            self.project()
        );
        match filter {
            PrFilter::All => {}
            PrFilter::Mine => path.push_str(&format!("&author_username={}", encode(&viewer))),
            PrFilter::ReviewRequested => {
                path.push_str(&format!("&reviewer_username={}", encode(&viewer)));
            }
        }
        let mrs: Vec<Mr> = self.api.get(&path).await?;
        let mut prs: Vec<PullRequest> = mrs
            .into_iter()
            .map(PullRequest::from)
            .filter(|p| filter.matches(p, &viewer))
            .collect();
        sort_by_updated(&mut prs);
        let status_skipped;
        if self.api.has_headroom() {
            let rest = prs.split_off(prs.len().min(STATUS_LOOKUPS));
            status_skipped = !rest.is_empty();
            let enriched: Vec<PullRequest> = futures::stream::iter(prs)
                .map(|pr| async move {
                    match self.enrich(pr.clone()).await {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::debug!(iid = pr.number, error = %e, "MR status lookup failed");
                            pr
                        }
                    }
                })
                .buffered(4)
                .collect()
                .await;
            prs = enriched;
            prs.extend(rest);
        } else {
            status_skipped = !prs.is_empty();
        }
        Ok(PrList {
            viewer,
            prs,
            status_skipped,
        })
    }

    async fn detail(&self, iid: u64) -> ForgeResult<PrDetail> {
        let mr_path = self.mr(iid);
        let (mr, commits, notes, approvals, project) = futures::try_join!(
            self.api.get::<Mr>(&mr_path),
            self.api
                .get::<Vec<CommitItem>>(format!("{mr_path}/commits?per_page=100")),
            self.api.get::<Vec<Note>>(format!(
                "{mr_path}/notes?sort=asc&order_by=created_at&per_page=100"
            )),
            self.approvals(iid),
            self.api.get::<Project>(self.project()),
        )?;
        let checks = match &mr.head_pipeline {
            Some(p) => self
                .api
                .get::<Vec<Job>>(format!(
                    "{}/pipelines/{}/jobs?per_page=100",
                    self.project(),
                    p.id
                ))
                .await?
                .into_iter()
                .map(Check::from)
                .collect(),
            None => Vec::new(),
        };
        let mergeable = match mr.detailed_merge_status.as_str() {
            _ if mr.has_conflicts => Some(false),
            "mergeable" => Some(true),
            "" | "checking" | "unchecked" | "preparing" | "approvals_syncing" => None,
            _ => Some(false),
        };
        let pipeline = mr.head_pipeline.as_ref().map(|p| pipeline_state(&p.status));
        let review = decision(&mr, &approvals);
        let body = mr.description.clone().unwrap_or_default();
        let mut pr = PullRequest::from(mr);
        pr.review = review;
        pr.checks = if checks.is_empty() {
            pipeline.unwrap_or_default()
        } else {
            aggregate_checks(checks.iter().map(|c| c.state))
        };
        pr.status_loaded = true;
        let timeline = notes
            .into_iter()
            .map(|n| TimelineItem {
                kind: if n.system {
                    TimelineKind::Event
                } else if n.kind.as_deref() == Some("DiffNote") {
                    let pos = n.position.unwrap_or_default();
                    TimelineKind::LineComment {
                        path: pos.new_path.or(pos.old_path).unwrap_or_default(),
                        line: pos.new_line.or(pos.old_line),
                    }
                } else {
                    TimelineKind::Comment
                },
                author: n.author.username,
                created_at: n.created_at,
                body: n.body,
            })
            .collect();
        Ok(PrDetail {
            pr,
            body,
            commits: commits.into_iter().map(PrCommit::from).collect(),
            checks,
            timeline,
            mergeable,
            merge_methods: project.methods(),
        })
    }

    async fn create(&self, new: &NewPullRequest) -> ForgeResult<PullRequest> {
        check_new(new)?;
        let title = new.title.trim();
        let title = if new.draft && !title.to_lowercase().starts_with("draft:") {
            format!("Draft: {title}")
        } else {
            title.to_owned()
        };
        let mr: Mr = self
            .api
            .send(
                Method::Post,
                &format!("{}/merge_requests", self.project()),
                json!({
                    "source_branch": new.head,
                    "target_branch": new.base,
                    "title": title,
                    "description": new.body,
                }),
            )
            .await?;
        Ok(mr.into())
    }

    async fn comment(&self, iid: u64, body: &str) -> ForgeResult<()> {
        check_review_body(ReviewEvent::Comment, body)?;
        self.note(iid, body).await
    }

    async fn review(&self, iid: u64, event: ReviewEvent, body: &str) -> ForgeResult<()> {
        check_review_body(event, body)?;
        match event {
            ReviewEvent::Approve => {
                self.api
                    .send_unit(
                        Method::Post,
                        &format!("{}/approve", self.mr(iid)),
                        json!({}),
                    )
                    .await?;
                if !body.trim().is_empty() {
                    self.note(iid, body).await?;
                }
                Ok(())
            }
            ReviewEvent::RequestChanges => {
                // Not approved yet: GitLab answers 404 / 401; nothing to withdraw.
                match self
                    .api
                    .send_unit(
                        Method::Post,
                        &format!("{}/unapprove", self.mr(iid)),
                        json!({}),
                    )
                    .await
                {
                    Ok(()) | Err(ForgeError::NotFound(_) | ForgeError::Unauthorized { .. }) => {}
                    Err(e) => return Err(e),
                }
                self.note(iid, &format!("**Changes requested**\n\n{body}"))
                    .await
            }
            ReviewEvent::Comment => self.note(iid, body).await,
        }
    }

    async fn merge(&self, iid: u64, method: MergeMethod, head_sha: &str) -> ForgeResult<()> {
        self.api
            .send_unit(
                Method::Put,
                &format!("{}/merge", self.mr(iid)),
                json!({
                    "sha": head_sha,
                    "squash": method == MergeMethod::Squash,
                    "should_remove_source_branch": false,
                }),
            )
            .await
    }
}

fn pipeline_state(status: &str) -> CheckState {
    match status {
        "success" => CheckState::Success,
        "failed" => CheckState::Failure,
        "canceled" | "canceling" | "skipped" | "manual" => CheckState::Neutral,
        "" => CheckState::None,
        _ => CheckState::Pending,
    }
}

fn decision(mr: &Mr, approvals: &Approvals) -> ReviewDecision {
    if mr.detailed_merge_status == "requested_changes" {
        ReviewDecision::ChangesRequested
    } else if !approvals.approved_by.is_empty() && approvals.approvals_left.unwrap_or(0) == 0 {
        ReviewDecision::Approved
    } else if !mr.reviewers.is_empty() || approvals.approvals_left.unwrap_or(0) > 0 {
        ReviewDecision::ReviewRequired
    } else {
        ReviewDecision::None
    }
}

// Wire types (only the fields used).

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct User {
    username: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Pipeline {
    id: u64,
    status: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Mr {
    iid: u64,
    title: String,
    author: User,
    source_branch: String,
    target_branch: String,
    sha: Option<String>,
    source_project_id: u64,
    target_project_id: u64,
    draft: bool,
    work_in_progress: bool,
    state: String,
    updated_at: String,
    web_url: String,
    reviewers: Vec<User>,
    labels: Vec<String>,
    description: Option<String>,
    detailed_merge_status: String,
    has_conflicts: bool,
    head_pipeline: Option<Pipeline>,
}

impl From<Mr> for PullRequest {
    fn from(m: Mr) -> Self {
        Self {
            number: m.iid,
            title: m.title,
            author: m.author.username,
            head_branch: m.source_branch,
            base_branch: m.target_branch,
            head_sha: m.sha.unwrap_or_default(),
            from_fork: m.source_project_id != m.target_project_id,
            draft: m.draft || m.work_in_progress,
            state: match m.state.as_str() {
                "merged" => PrState::Merged,
                "closed" | "locked" => PrState::Closed,
                _ => PrState::Open,
            },
            updated_at: m.updated_at,
            web_url: m.web_url,
            requested_reviewers: m.reviewers.into_iter().map(|u| u.username).collect(),
            labels: m.labels,
            ..Self::default()
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Approvals {
    approved_by: Vec<serde_json::Value>,
    approvals_left: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct CommitItem {
    id: String,
    title: String,
    author_name: String,
    authored_date: Option<String>,
    created_at: String,
}

impl From<CommitItem> for PrCommit {
    fn from(c: CommitItem) -> Self {
        Self {
            sha: c.id,
            summary: c.title,
            author: c.author_name,
            date: c.authored_date.unwrap_or(c.created_at),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Position {
    new_path: Option<String>,
    old_path: Option<String>,
    new_line: Option<u64>,
    old_line: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Note {
    body: String,
    author: User,
    created_at: String,
    system: bool,
    #[serde(rename = "type")]
    kind: Option<String>,
    position: Option<Position>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Job {
    name: String,
    stage: String,
    status: String,
    web_url: Option<String>,
    allow_failure: bool,
}

impl From<Job> for Check {
    fn from(j: Job) -> Self {
        let state = match pipeline_state(&j.status) {
            CheckState::Failure if j.allow_failure => CheckState::Neutral,
            s => s,
        };
        Self {
            name: j.name,
            state,
            url: j.web_url,
            detail: format!("{} · {}", j.stage, j.status),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Project {
    merge_method: String,
    squash_option: String,
}

impl Project {
    fn methods(&self) -> Vec<MergeMethod> {
        // GitLab merges with the project's method; squash is a per-merge flag.
        let base = match self.merge_method.as_str() {
            "ff" | "rebase_merge" => MergeMethod::Rebase,
            _ => MergeMethod::Merge,
        };
        match self.squash_option.as_str() {
            "never" => vec![base],
            "always" => vec![MergeMethod::Squash],
            _ => vec![base, MergeMethod::Squash],
        }
    }
}
