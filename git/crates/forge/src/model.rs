//! Forge-neutral pull / merge request types, plus status aggregation.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrState {
    #[default]
    Open,
    Closed,
    Merged,
}

/// One CI check or the aggregate of several.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord)]
pub enum CheckState {
    /// No checks reported (or not loaded).
    #[default]
    None,
    /// Skipped / neutral / cancelled / manual.
    Neutral,
    Success,
    Pending,
    Failure,
}

impl CheckState {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "No checks",
            Self::Neutral => "Skipped",
            Self::Success => "Checks passed",
            Self::Pending => "Checks running",
            Self::Failure => "Checks failed",
        }
    }
}

/// Aggregate of many checks: any failure wins, then anything pending,
/// then success; only skipped/neutral checks give `Neutral`.
#[must_use]
pub fn aggregate_checks(states: impl IntoIterator<Item = CheckState>) -> CheckState {
    states.into_iter().max().unwrap_or(CheckState::None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReviewDecision {
    /// Nobody asked, nobody reviewed.
    #[default]
    None,
    /// Reviewers requested, no decision yet.
    ReviewRequired,
    Approved,
    ChangesRequested,
}

impl ReviewDecision {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "",
            Self::ReviewRequired => "Review required",
            Self::Approved => "Approved",
            Self::ChangesRequested => "Changes requested",
        }
    }
}

/// A submitted review's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Pending,
}

/// Decision from each reviewer's latest verdict (comments don't override a
/// verdict; a dismissal clears it). Any "changes requested" wins over approvals.
#[must_use]
pub fn review_decision(
    reviews: &[(String, ReviewState)],
    requested_reviewers: usize,
) -> ReviewDecision {
    let mut latest: HashMap<&str, ReviewState> = HashMap::new();
    for (who, state) in reviews {
        match state {
            ReviewState::Approved | ReviewState::ChangesRequested => {
                latest.insert(who, *state);
            }
            ReviewState::Dismissed => {
                latest.remove(who.as_str());
            }
            ReviewState::Commented | ReviewState::Pending => {}
        }
    }
    if latest.values().any(|s| *s == ReviewState::ChangesRequested) {
        ReviewDecision::ChangesRequested
    } else if latest.values().any(|s| *s == ReviewState::Approved) && requested_reviewers == 0 {
        ReviewDecision::Approved
    } else if requested_reviewers > 0 {
        ReviewDecision::ReviewRequired
    } else {
        ReviewDecision::None
    }
}

/// A pull / merge request as listed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PullRequest {
    /// GitHub number / GitLab iid.
    pub number: u64,
    pub title: String,
    pub author: String,
    pub head_branch: String,
    pub base_branch: String,
    pub head_sha: String,
    /// The head lives in another project (a fork).
    pub from_fork: bool,
    pub draft: bool,
    pub state: PrState,
    /// RFC 3339.
    pub updated_at: String,
    pub web_url: String,
    pub requested_reviewers: Vec<String>,
    pub labels: Vec<String>,
    pub review: ReviewDecision,
    pub checks: CheckState,
    /// Review / CI status was loaded (lists skip it when the rate limit is low).
    pub status_loaded: bool,
}

impl PullRequest {
    /// `true` when `login` asked for a review from this user.
    #[must_use]
    pub fn review_requested_from(&self, login: &str) -> bool {
        self.requested_reviewers
            .iter()
            .any(|r| r.eq_ignore_ascii_case(login))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrFilter {
    #[default]
    All,
    /// Authored by the token's user.
    Mine,
    /// A review is requested from the token's user.
    ReviewRequested,
}

impl PrFilter {
    pub const ALL: [Self; 3] = [Self::All, Self::Mine, Self::ReviewRequested];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Mine => "Mine",
            Self::ReviewRequested => "Review requested",
        }
    }

    /// Client-side filter (also applied after server-side filtering, harmlessly).
    #[must_use]
    pub fn matches(self, pr: &PullRequest, viewer: &str) -> bool {
        match self {
            Self::All => true,
            Self::Mine => pr.author.eq_ignore_ascii_case(viewer),
            Self::ReviewRequested => pr.review_requested_from(viewer),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrCommit {
    pub sha: String,
    pub summary: String,
    pub author: String,
    /// RFC 3339.
    pub date: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Check {
    pub name: String,
    pub state: CheckState,
    pub url: Option<String>,
    /// Status text / job stage.
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineKind {
    Comment,
    Review(ReviewState),
    /// A comment on a line of a file.
    LineComment {
        path: String,
        line: Option<u64>,
    },
    /// System event (GitLab system notes: "approved this merge request", pushes).
    Event,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineItem {
    pub author: String,
    /// RFC 3339.
    pub created_at: String,
    pub kind: TimelineKind,
    pub body: String,
}

/// Merge strategies the project allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

impl MergeMethod {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Merge => "Create a merge commit",
            Self::Squash => "Squash and merge",
            Self::Rebase => "Rebase and merge",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrDetail {
    pub pr: PullRequest,
    pub body: String,
    pub commits: Vec<PrCommit>,
    pub checks: Vec<Check>,
    /// Comments and reviews, oldest first.
    pub timeline: Vec<TimelineItem>,
    /// `None` = the forge is still computing it.
    pub mergeable: Option<bool>,
    pub merge_methods: Vec<MergeMethod>,
}

/// The verdict of a review the user submits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewEvent {
    Approve,
    RequestChanges,
    Comment,
}

impl ReviewEvent {
    pub const ALL: [Self; 3] = [Self::Comment, Self::Approve, Self::RequestChanges];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Approve => "Approve",
            Self::RequestChanges => "Request changes",
            Self::Comment => "Comment",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewPullRequest {
    pub title: String,
    pub body: String,
    /// Source branch (in the same project).
    pub head: String,
    pub base: String,
    pub draft: bool,
}

/// Sorts newest first by `updated_at` (RFC 3339 strings sort chronologically in UTC).
pub fn sort_by_updated(prs: &mut [PullRequest]) {
    prs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_aggregation_priorities() {
        use CheckState::*;
        assert_eq!(aggregate_checks([]), None);
        assert_eq!(aggregate_checks([Success, Neutral]), Success);
        assert_eq!(aggregate_checks([Success, Pending, Neutral]), Pending);
        assert_eq!(aggregate_checks([Pending, Failure, Success]), Failure);
        assert_eq!(aggregate_checks([Neutral, Neutral]), Neutral);
    }

    fn r(who: &str, s: ReviewState) -> (String, ReviewState) {
        (who.to_owned(), s)
    }

    #[test]
    fn review_decision_uses_latest_verdict_per_reviewer() {
        use ReviewState::*;
        assert_eq!(review_decision(&[], 0), ReviewDecision::None);
        assert_eq!(review_decision(&[], 2), ReviewDecision::ReviewRequired);
        assert_eq!(
            review_decision(&[r("a", ChangesRequested), r("a", Approved)], 0),
            ReviewDecision::Approved
        );
        // A later plain comment does not undo the verdict.
        assert_eq!(
            review_decision(&[r("a", ChangesRequested), r("a", Commented)], 0),
            ReviewDecision::ChangesRequested
        );
        assert_eq!(
            review_decision(&[r("a", Approved), r("b", ChangesRequested)], 0),
            ReviewDecision::ChangesRequested
        );
        assert_eq!(
            review_decision(&[r("a", ChangesRequested), r("a", Dismissed)], 0),
            ReviewDecision::None
        );
        // Approved by one, still waiting on a requested reviewer.
        assert_eq!(
            review_decision(&[r("a", Approved)], 1),
            ReviewDecision::ReviewRequired
        );
    }

    #[test]
    fn filters_match_viewer() {
        let pr = PullRequest {
            author: "Alice".into(),
            requested_reviewers: vec!["bob".into()],
            ..PullRequest::default()
        };
        assert!(PrFilter::All.matches(&pr, "x"));
        assert!(PrFilter::Mine.matches(&pr, "alice"));
        assert!(!PrFilter::Mine.matches(&pr, "bob"));
        assert!(PrFilter::ReviewRequested.matches(&pr, "BOB"));
        assert!(!PrFilter::ReviewRequested.matches(&pr, "alice"));
    }
}
