//! The forge-neutral client trait and its constructor.

use crate::api::Api;
use crate::error::ForgeResult;
use crate::model::{
    MergeMethod, NewLineComment, NewPullRequest, PrDetail, PrFilter, PullRequest, ReviewEvent,
};
use crate::remote::ForgeRepo;
use crate::{github::GitHub, gitlab::GitLab};
use async_trait::async_trait;
use ferrumgit_domain::ForgeKind;

/// Open pull requests plus who is asking.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrList {
    /// Login / username of the token's owner.
    pub viewer: String,
    pub prs: Vec<PullRequest>,
    /// Review / CI status was skipped for some entries to save rate limit.
    pub status_skipped: bool,
}

/// Lists enrich at most this many entries with review / CI status.
pub const STATUS_LOOKUPS: usize = 20;

#[async_trait]
pub trait Forge: Send + Sync {
    fn repo(&self) -> &ForgeRepo;

    /// Login (GitHub) / username (GitLab) of the token's owner; also validates the token.
    async fn viewer(&self) -> ForgeResult<String>;

    /// Open pull / merge requests, newest activity first.
    async fn list(&self, filter: PrFilter) -> ForgeResult<PrList>;

    async fn detail(&self, number: u64) -> ForgeResult<PrDetail>;

    async fn create(&self, new: &NewPullRequest) -> ForgeResult<PullRequest>;

    async fn comment(&self, number: u64, body: &str) -> ForgeResult<()>;

    async fn review(&self, number: u64, event: ReviewEvent, body: &str) -> ForgeResult<()>;

    /// Starts a thread on a diff line of `pr` (its detail, as loaded).
    async fn line_comment(&self, pr: &PrDetail, comment: &NewLineComment) -> ForgeResult<()>;

    /// Replies in the review thread `thread` (a [`crate::ReviewThread::id`]).
    async fn reply(&self, number: u64, thread: &str, body: &str) -> ForgeResult<()>;

    /// Merges with `method`, only if the head is still `head_sha`.
    async fn merge(&self, number: u64, method: MergeMethod, head_sha: &str) -> ForgeResult<()>;
}

/// A client for `repo` over `api`.
#[must_use]
pub fn client(repo: ForgeRepo, api: Api) -> Box<dyn Forge> {
    match repo.kind {
        ForgeKind::GitHub => Box::new(GitHub::new(repo, api)),
        ForgeKind::GitLab => Box::new(GitLab::new(repo, api)),
    }
}

/// Validates a review before sending: GitHub and GitLab need text for
/// "comment" and "request changes".
pub(crate) fn check_review_body(event: ReviewEvent, body: &str) -> ForgeResult<()> {
    if event != ReviewEvent::Approve && body.trim().is_empty() {
        return Err(crate::ForgeError::Invalid(format!(
            "Write a comment to {}.",
            match event {
                ReviewEvent::RequestChanges => "request changes",
                _ => "comment",
            }
        )));
    }
    Ok(())
}

/// A line comment needs text and a line.
pub(crate) fn check_line_comment(c: &NewLineComment) -> ForgeResult<()> {
    check_review_body(ReviewEvent::Comment, &c.body)?;
    if c.path.is_empty() || c.anchor().is_none() {
        return Err(crate::ForgeError::Invalid(
            "Choose a line to comment on.".into(),
        ));
    }
    Ok(())
}

pub(crate) fn check_new(new: &NewPullRequest) -> ForgeResult<()> {
    let bad = |m: &str| Err(crate::ForgeError::Invalid(m.to_owned()));
    if new.title.trim().is_empty() {
        return bad("Enter a title.");
    }
    if new.head.trim().is_empty() {
        return bad("Check out the branch to propose first (HEAD is detached).");
    }
    if new.base.trim().is_empty() {
        return bad("Choose a base branch.");
    }
    if new.head == new.base {
        return bad("The base branch must differ from the current branch.");
    }
    Ok(())
}
