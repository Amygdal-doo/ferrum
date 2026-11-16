//! GitHub REST v3 (github.com and GitHub Enterprise Server).

use crate::api::{Api, encode};
use crate::client::{
    Forge, PrList, STATUS_LOOKUPS, check_line_comment, check_new, check_review_body,
};
use crate::error::ForgeResult;
use crate::model::{
    Check, CheckState, DiffSide, MergeMethod, NewLineComment, NewPullRequest, PrCommit, PrDetail,
    PrFilter, PrState, PullRequest, ReviewEvent, ReviewState, ReviewThread, ThreadComment,
    TimelineItem, TimelineKind, aggregate_checks, review_decision, sort_by_updated,
};
use crate::remote::ForgeRepo;
use crate::transport::Method;
use async_trait::async_trait;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;

pub struct GitHub {
    repo: ForgeRepo,
    api: Api,
}

impl GitHub {
    #[must_use]
    pub fn new(repo: ForgeRepo, api: Api) -> Self {
        Self { repo, api }
    }

    fn base(&self) -> String {
        format!(
            "/repos/{}/{}",
            encode(&self.repo.owner),
            encode(&self.repo.name)
        )
    }

    async fn reviews(&self, number: u64) -> ForgeResult<Vec<Review>> {
        self.api
            .get(format!(
                "{}/pulls/{number}/reviews?per_page=100",
                self.base()
            ))
            .await
    }

    /// Check runs plus legacy commit statuses for `sha`.
    async fn checks(&self, sha: &str) -> ForgeResult<Vec<Check>> {
        let base = self.base();
        let (runs, status) = futures::try_join!(
            self.api.get::<CheckRuns>(format!(
                "{base}/commits/{}/check-runs?per_page=100",
                encode(sha)
            )),
            self.api
                .get::<CombinedStatus>(format!("{base}/commits/{}/status", encode(sha))),
        )?;
        let mut checks: Vec<Check> = runs.check_runs.into_iter().map(Check::from).collect();
        checks.extend(status.statuses.into_iter().map(Check::from));
        Ok(checks)
    }

    /// Fills review decision and CI state.
    async fn enrich(&self, mut pr: PullRequest) -> ForgeResult<PullRequest> {
        let (reviews, checks) =
            futures::try_join!(self.reviews(pr.number), self.checks(&pr.head_sha))?;
        pr.review = review_decision(
            &reviews.iter().map(Review::verdict).collect::<Vec<_>>(),
            pr.requested_reviewers.len(),
        );
        pr.checks = aggregate_checks(checks.iter().map(|c| c.state));
        pr.status_loaded = true;
        Ok(pr)
    }
}

#[async_trait]
impl Forge for GitHub {
    fn repo(&self) -> &ForgeRepo {
        &self.repo
    }

    async fn viewer(&self) -> ForgeResult<String> {
        Ok(self.api.get::<User>("/user").await?.login)
    }

    async fn list(&self, filter: PrFilter) -> ForgeResult<PrList> {
        let path = format!(
            "{}/pulls?state=open&sort=updated&direction=desc&per_page=50",
            self.base()
        );
        let (viewer, pulls) = futures::try_join!(self.viewer(), self.api.get::<Vec<Pull>>(&path))?;
        let mut prs: Vec<PullRequest> = pulls
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
                            tracing::debug!(number = pr.number, error = %e, "PR status lookup failed");
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

    async fn detail(&self, number: u64) -> ForgeResult<PrDetail> {
        let base = self.base();
        let pull: Pull = self.api.get(format!("{base}/pulls/{number}")).await?;
        let (commits, reviews, comments, line_comments, checks, repo) = futures::try_join!(
            self.api
                .get::<Vec<CommitItem>>(format!("{base}/pulls/{number}/commits?per_page=100")),
            self.reviews(number),
            self.api
                .get::<Vec<Comment>>(format!("{base}/issues/{number}/comments?per_page=100")),
            self.api
                .get::<Vec<LineComment>>(format!("{base}/pulls/{number}/comments?per_page=100")),
            self.checks(&pull.head.sha),
            self.api.get::<RepoInfo>(&base),
        )?;
        let body = pull.body.clone().unwrap_or_default();
        let mergeable = pull.mergeable;
        let mut pr = PullRequest::from(pull);
        pr.review = review_decision(
            &reviews.iter().map(Review::verdict).collect::<Vec<_>>(),
            pr.requested_reviewers.len(),
        );
        pr.checks = aggregate_checks(checks.iter().map(|c| c.state));
        pr.status_loaded = true;

        let mut timeline: Vec<TimelineItem> = Vec::new();
        timeline.extend(comments.into_iter().map(|c| TimelineItem {
            author: c.user.login,
            created_at: c.created_at,
            kind: TimelineKind::Comment,
            body: c.body.unwrap_or_default(),
        }));
        timeline.extend(
            reviews
                .into_iter()
                .filter(|r| r.state != "PENDING")
                .map(|r| TimelineItem {
                    kind: TimelineKind::Review(r.verdict().1),
                    author: r.user.login,
                    created_at: r.submitted_at.unwrap_or_default(),
                    body: r.body.unwrap_or_default(),
                }),
        );
        let threads = threads(&line_comments);
        timeline.extend(line_comments.into_iter().map(|c| TimelineItem {
            author: c.user.login,
            created_at: c.created_at,
            kind: TimelineKind::LineComment {
                path: c.path,
                line: c.line.or(c.original_line),
            },
            body: c.body.unwrap_or_default(),
        }));
        timeline.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        Ok(PrDetail {
            pr,
            body,
            commits: commits.into_iter().map(PrCommit::from).collect(),
            checks,
            timeline,
            mergeable,
            merge_methods: repo.methods(),
            threads,
            diff_refs: None,
        })
    }

    async fn create(&self, new: &NewPullRequest) -> ForgeResult<PullRequest> {
        check_new(new)?;
        let pull: Pull = self
            .api
            .send(
                Method::Post,
                &format!("{}/pulls", self.base()),
                json!({
                    "title": new.title.trim(),
                    "body": new.body,
                    "head": new.head,
                    "base": new.base,
                    "draft": new.draft,
                }),
            )
            .await?;
        Ok(pull.into())
    }

    async fn comment(&self, number: u64, body: &str) -> ForgeResult<()> {
        check_review_body(ReviewEvent::Comment, body)?;
        self.api
            .send_unit(
                Method::Post,
                &format!("{}/issues/{number}/comments", self.base()),
                json!({ "body": body }),
            )
            .await
    }

    async fn review(&self, number: u64, event: ReviewEvent, body: &str) -> ForgeResult<()> {
        check_review_body(event, body)?;
        let event = match event {
            ReviewEvent::Approve => "APPROVE",
            ReviewEvent::RequestChanges => "REQUEST_CHANGES",
            ReviewEvent::Comment => "COMMENT",
        };
        let mut payload = json!({ "event": event });
        if !body.trim().is_empty() {
            payload["body"] = json!(body);
        }
        self.api
            .send_unit(
                Method::Post,
                &format!("{}/pulls/{number}/reviews", self.base()),
                payload,
            )
            .await
    }

    async fn line_comment(&self, pr: &PrDetail, comment: &NewLineComment) -> ForgeResult<()> {
        check_line_comment(comment)?;
        let Some((side, line)) = comment.anchor() else {
            return Ok(());
        };
        self.api
            .send_unit(
                Method::Post,
                &format!("{}/pulls/{}/comments", self.base(), pr.pr.number),
                json!({
                    "body": comment.body,
                    "commit_id": pr.pr.head_sha,
                    "path": comment.path,
                    "line": line,
                    "side": match side {
                        DiffSide::Left => "LEFT",
                        DiffSide::Right => "RIGHT",
                    },
                }),
            )
            .await
    }

    async fn reply(&self, number: u64, thread: &str, body: &str) -> ForgeResult<()> {
        check_review_body(ReviewEvent::Comment, body)?;
        self.api
            .send_unit(
                Method::Post,
                &format!(
                    "{}/pulls/{number}/comments/{}/replies",
                    self.base(),
                    encode(thread)
                ),
                json!({ "body": body }),
            )
            .await
    }

    async fn merge(&self, number: u64, method: MergeMethod, head_sha: &str) -> ForgeResult<()> {
        let method = match method {
            MergeMethod::Merge => "merge",
            MergeMethod::Squash => "squash",
            MergeMethod::Rebase => "rebase",
        };
        self.api
            .send_unit(
                Method::Put,
                &format!("{}/pulls/{number}/merge", self.base()),
                json!({ "merge_method": method, "sha": head_sha }),
            )
            .await
    }
}

// Wire types (only the fields used).

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct User {
    login: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RepoRef {
    full_name: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Branch {
    #[serde(rename = "ref")]
    name: String,
    sha: String,
    repo: Option<RepoRef>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Label {
    name: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Pull {
    number: u64,
    title: String,
    user: User,
    head: Branch,
    base: Branch,
    draft: bool,
    state: String,
    merged_at: Option<String>,
    updated_at: String,
    html_url: String,
    requested_reviewers: Vec<User>,
    labels: Vec<Label>,
    body: Option<String>,
    mergeable: Option<bool>,
}

impl From<Pull> for PullRequest {
    fn from(p: Pull) -> Self {
        let from_fork = match (&p.head.repo, &p.base.repo) {
            (Some(h), Some(b)) => !h.full_name.eq_ignore_ascii_case(&b.full_name),
            // The fork was deleted.
            (None, _) => true,
            (Some(_), None) => false,
        };
        Self {
            number: p.number,
            title: p.title,
            author: p.user.login,
            head_branch: p.head.name,
            base_branch: p.base.name,
            head_sha: p.head.sha,
            from_fork,
            draft: p.draft,
            state: match (p.state.as_str(), p.merged_at.is_some()) {
                (_, true) => PrState::Merged,
                ("closed", false) => PrState::Closed,
                _ => PrState::Open,
            },
            updated_at: p.updated_at,
            web_url: p.html_url,
            requested_reviewers: p.requested_reviewers.into_iter().map(|u| u.login).collect(),
            labels: p.labels.into_iter().map(|l| l.name).collect(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Review {
    user: User,
    state: String,
    body: Option<String>,
    submitted_at: Option<String>,
}

impl Review {
    fn verdict(&self) -> (String, ReviewState) {
        let state = match self.state.as_str() {
            "APPROVED" => ReviewState::Approved,
            "CHANGES_REQUESTED" => ReviewState::ChangesRequested,
            "DISMISSED" => ReviewState::Dismissed,
            "PENDING" => ReviewState::Pending,
            _ => ReviewState::Commented,
        };
        (self.user.login.clone(), state)
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Comment {
    user: User,
    body: Option<String>,
    created_at: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct LineComment {
    id: u64,
    in_reply_to_id: Option<u64>,
    user: User,
    body: Option<String>,
    created_at: String,
    path: String,
    line: Option<u64>,
    original_line: Option<u64>,
    side: Option<String>,
}

/// Groups review comments into threads by the comment they reply to
/// (GitHub replies always point at the thread's first comment).
fn threads(comments: &[LineComment]) -> Vec<ReviewThread> {
    let mut sorted: Vec<&LineComment> = comments.iter().collect();
    sorted.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    let mut out: Vec<ReviewThread> = Vec::new();
    for c in sorted {
        let comment = ThreadComment {
            id: c.id.to_string(),
            author: c.user.login.clone(),
            created_at: c.created_at.clone(),
            body: c.body.clone().unwrap_or_default(),
        };
        let root = c.in_reply_to_id.map(|r| r.to_string());
        match root.and_then(|r| out.iter_mut().find(|t| t.id == r)) {
            Some(thread) => thread.comments.push(comment),
            None => out.push(ReviewThread {
                id: c.id.to_string(),
                path: c.path.clone(),
                line: c.line,
                side: if c.side.as_deref() == Some("LEFT") {
                    DiffSide::Left
                } else {
                    DiffSide::Right
                },
                resolved: false,
                comments: vec![comment],
            }),
        }
    }
    out
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct GitPerson {
    name: String,
    date: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct GitCommit {
    message: String,
    author: GitPerson,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct CommitItem {
    sha: String,
    commit: GitCommit,
    author: Option<User>,
}

impl From<CommitItem> for PrCommit {
    fn from(c: CommitItem) -> Self {
        Self {
            summary: c
                .commit
                .message
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned(),
            author: c
                .author
                .map(|u| u.login)
                .filter(|l| !l.is_empty())
                .unwrap_or(c.commit.author.name),
            date: c.commit.author.date,
            sha: c.sha,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct CheckOutput {
    title: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct CheckRun {
    name: String,
    status: String,
    conclusion: Option<String>,
    html_url: Option<String>,
    details_url: Option<String>,
    output: CheckOutput,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct CheckRuns {
    check_runs: Vec<CheckRun>,
}

impl From<CheckRun> for Check {
    fn from(r: CheckRun) -> Self {
        let state = if r.status == "completed" {
            match r.conclusion.as_deref().unwrap_or_default() {
                "success" => CheckState::Success,
                "failure" | "timed_out" | "action_required" | "startup_failure" => {
                    CheckState::Failure
                }
                _ => CheckState::Neutral,
            }
        } else {
            CheckState::Pending
        };
        let detail = r
            .output
            .title
            .filter(|t| !t.is_empty())
            .or(r.conclusion)
            .unwrap_or(r.status);
        Self {
            name: r.name,
            state,
            url: r.html_url.or(r.details_url),
            detail,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Status {
    context: String,
    state: String,
    target_url: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct CombinedStatus {
    statuses: Vec<Status>,
}

impl From<Status> for Check {
    fn from(s: Status) -> Self {
        Self {
            state: match s.state.as_str() {
                "success" => CheckState::Success,
                "failure" | "error" => CheckState::Failure,
                _ => CheckState::Pending,
            },
            detail: s.description.unwrap_or(s.state),
            name: s.context,
            url: s.target_url,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct RepoInfo {
    allow_merge_commit: bool,
    allow_squash_merge: bool,
    allow_rebase_merge: bool,
}

impl Default for RepoInfo {
    /// Fields are missing without push access; assume all methods.
    fn default() -> Self {
        Self {
            allow_merge_commit: true,
            allow_squash_merge: true,
            allow_rebase_merge: true,
        }
    }
}

impl RepoInfo {
    fn methods(&self) -> Vec<MergeMethod> {
        [
            (self.allow_merge_commit, MergeMethod::Merge),
            (self.allow_squash_merge, MergeMethod::Squash),
            (self.allow_rebase_merge, MergeMethod::Rebase),
        ]
        .into_iter()
        .filter_map(|(on, m)| on.then_some(m))
        .collect()
    }
}
