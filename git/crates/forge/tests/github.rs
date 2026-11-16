//! GitHub REST v3 mapping, replayed from recorded fixtures (no network).

mod common;

use common::Fixtures;
use ferrumgit_domain::{RepoForge, Secret};
use ferrumgit_forge::{
    Api, CheckState, DiffSide, Forge, ForgeError, MergeMethod, NewLineComment, NewPullRequest,
    PrDetail, PrFilter, RateLimits, ReviewDecision, ReviewEvent, ReviewState, TimelineKind, client,
    resolve,
};
use std::sync::Arc;

const BASE: &str = "https://api.github.com";
const TOKEN: &str = "test-token-not-real";

fn forge(fixtures: Arc<Fixtures>, limits: Arc<RateLimits>) -> Box<dyn Forge> {
    let repo = resolve(
        &[("origin".into(), "git@github.com:acme/widgets.git".into())],
        &RepoForge::default(),
    )
    .expect("repo");
    let api = Api::new(
        fixtures,
        repo.kind,
        &repo.host,
        &repo.api_url,
        Secret::new(TOKEN),
        limits,
    );
    client(repo, api)
}

fn listing() -> Fixtures {
    let mut f = Fixtures::new(BASE, "github")
        .on("GET /user", "user.json")
        .on(
            "GET /repos/acme/widgets/pulls?state=open&sort=updated&direction=desc&per_page=50",
            "pulls.json",
        );
    for (n, sha) in [(9, "9"), (12, "12"), (15, "15")] {
        let full = sha.repeat(40 / sha.len());
        f = f
            .on(
                &format!("GET /repos/acme/widgets/pulls/{n}/reviews?per_page=100"),
                &format!("reviews_{n}.json"),
            )
            .on(
                &format!("GET /repos/acme/widgets/commits/{full}/check-runs?per_page=100"),
                &format!("check_runs_{n}.json"),
            )
            .on(
                &format!("GET /repos/acme/widgets/commits/{full}/status"),
                &format!("status_{n}.json"),
            );
    }
    f
}

fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("rt")
        .block_on(f)
}

#[test]
fn lists_open_pull_requests_with_review_and_ci_status() {
    let fx = Arc::new(listing());
    let gh = forge(fx.clone(), Arc::default());
    let list = run(gh.list(PrFilter::All)).expect("list");
    assert_eq!(list.viewer, "octocat");
    assert!(!list.status_skipped);
    let numbers: Vec<u64> = list.prs.iter().map(|p| p.number).collect();
    assert_eq!(numbers, [12, 15, 9], "newest activity first");

    let login = &list.prs[0];
    assert_eq!(login.title, "Add login screen");
    assert_eq!(login.author, "octocat");
    assert_eq!(
        (login.head_branch.as_str(), login.base_branch.as_str()),
        ("feature/login", "main")
    );
    assert!(!login.from_fork && !login.draft);
    assert_eq!(login.labels, ["feature", "ui"]);
    assert_eq!(login.web_url, "https://github.com/acme/widgets/pull/12");
    // hubot: changes requested, then a plain comment; monalisa approved.
    assert_eq!(login.review, ReviewDecision::ChangesRequested);
    // success + skipped + in progress + legacy success.
    assert_eq!(login.checks, CheckState::Pending);
    assert!(login.status_loaded);

    let typo = &list.prs[1];
    assert!(typo.draft && typo.from_fork);
    assert_eq!(typo.review, ReviewDecision::ReviewRequired);
    assert_eq!(typo.checks, CheckState::Failure);

    let refactor = &list.prs[2];
    assert_eq!(refactor.review, ReviewDecision::Approved);
    assert_eq!(refactor.checks, CheckState::None);

    // Every request carries the token as a bearer header and the API version.
    let sent = fx.sent();
    assert!(sent.iter().all(|r| {
        r.headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == &format!("Bearer {TOKEN}"))
    }));
    assert!(
        sent.iter()
            .all(|r| r.headers.iter().any(|(k, _)| k == "x-github-api-version"))
    );
}

#[test]
fn filters_mine_and_review_requested() {
    let gh = forge(Arc::new(listing()), Arc::default());
    let mine = run(gh.list(PrFilter::Mine)).expect("mine");
    assert_eq!(mine.prs.iter().map(|p| p.number).collect::<Vec<_>>(), [12]);
    let asked = run(gh.list(PrFilter::ReviewRequested)).expect("review");
    assert_eq!(asked.prs.iter().map(|p| p.number).collect::<Vec<_>>(), [15]);
}

#[test]
fn skips_status_lookups_when_rate_limit_is_low() {
    let fx = Arc::new(
        Fixtures::new(BASE, "github")
            .on_status(
                "GET /user",
                200,
                &[
                    ("x-ratelimit-remaining", "12"),
                    ("x-ratelimit-reset", "1999999999"),
                ],
                "user.json",
            )
            .on_status(
                "GET /repos/acme/widgets/pulls?state=open&sort=updated&direction=desc&per_page=50",
                200,
                &[
                    ("x-ratelimit-remaining", "11"),
                    ("x-ratelimit-reset", "1999999999"),
                ],
                "pulls.json",
            ),
    );
    let limits = Arc::new(RateLimits::default());
    let gh = forge(fx.clone(), limits.clone());
    let list = run(gh.list(PrFilter::All)).expect("list");
    assert!(list.status_skipped);
    assert!(list.prs.iter().all(|p| !p.status_loaded));
    assert_eq!(fx.sent().len(), 2, "no per-PR lookups");
    assert_eq!(limits.get().map(|s| s.remaining), Some(11));
}

#[test]
fn exhausted_rate_limit_is_reported_and_not_retried() {
    let fx = Arc::new(Fixtures::new(BASE, "github").on_status(
        "GET /user",
        403,
        &[
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", "4102444800"),
        ],
        "bad_credentials.json",
    ));
    let gh = forge(fx.clone(), Arc::default());
    let err = run(gh.viewer()).expect_err("limited");
    assert_eq!(
        err,
        ForgeError::RateLimited {
            reset_at: Some(4_102_444_800)
        }
    );
    // The next call is refused locally until the reset time.
    let again = run(gh.viewer()).expect_err("still limited");
    assert!(matches!(again, ForgeError::RateLimited { .. }));
    assert_eq!(fx.sent().len(), 1);
}

#[test]
fn bad_token_explains_scopes_without_leaking_it() {
    let fx = Arc::new(Fixtures::new(BASE, "github").on_status(
        "GET /user",
        401,
        &[],
        "bad_credentials.json",
    ));
    let err = run(forge(fx, Arc::default()).viewer()).expect_err("401");
    let text = err.to_string();
    assert!(
        text.contains("github.com") && text.contains("`repo`"),
        "{text}"
    );
    assert!(!text.contains(TOKEN));
}

#[test]
fn loads_detail_with_commits_checks_and_conversation() {
    let fx = Arc::new(
        Fixtures::new(BASE, "github")
            .on("GET /repos/acme/widgets/pulls/12", "pull_12.json")
            .on("GET /repos/acme/widgets/pulls/12/commits?per_page=100", "commits_12.json")
            .on("GET /repos/acme/widgets/pulls/12/reviews?per_page=100", "reviews_12.json")
            .on("GET /repos/acme/widgets/issues/12/comments?per_page=100", "issue_comments_12.json")
            .on("GET /repos/acme/widgets/pulls/12/comments?per_page=100", "review_comments_12.json")
            .on(
                "GET /repos/acme/widgets/commits/1212121212121212121212121212121212121212/check-runs?per_page=100",
                "check_runs_12.json",
            )
            .on(
                "GET /repos/acme/widgets/commits/1212121212121212121212121212121212121212/status",
                "status_12.json",
            )
            .on("GET /repos/acme/widgets", "repo.json"),
    );
    let d = run(forge(fx, Arc::default()).detail(12)).expect("detail");
    assert!(d.body.starts_with("## Summary"));
    assert_eq!(d.mergeable, Some(true));
    assert_eq!(d.merge_methods, [MergeMethod::Squash, MergeMethod::Rebase]);
    assert_eq!(d.pr.review, ReviewDecision::ChangesRequested);

    assert_eq!(d.commits.len(), 2);
    assert_eq!(d.commits[0].summary, "Add login form");
    assert_eq!(d.commits[0].author, "octocat");
    assert_eq!(
        d.commits[1].author, "Someone Else",
        "falls back to the git author"
    );

    let names: Vec<(&str, CheckState)> = d
        .checks
        .iter()
        .map(|c| (c.name.as_str(), c.state))
        .collect();
    assert_eq!(
        names,
        [
            ("build", CheckState::Success),
            ("lint", CheckState::Neutral),
            ("test", CheckState::Pending),
            ("ci/legacy", CheckState::Success),
        ]
    );
    assert_eq!(d.checks[0].detail, "Build passed");
    assert_eq!(d.checks[3].url.as_deref(), Some("https://ci.example.com/1"));

    // Oldest first; the pending (unsubmitted) review is not shown.
    let kinds: Vec<(&str, &TimelineKind)> = d
        .timeline
        .iter()
        .map(|t| (t.author.as_str(), &t.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("monalisa", &TimelineKind::Comment),
            (
                "hubot",
                &TimelineKind::LineComment {
                    path: "src/login.rs".into(),
                    line: Some(42)
                }
            ),
            (
                "hubot",
                &TimelineKind::Review(ReviewState::ChangesRequested)
            ),
            ("hubot", &TimelineKind::Review(ReviewState::Commented)),
            (
                "monalisa",
                &TimelineKind::LineComment {
                    path: "src/form.rs".into(),
                    line: Some(7)
                }
            ),
            (
                "octocat",
                &TimelineKind::LineComment {
                    path: "src/login.rs".into(),
                    line: Some(42)
                }
            ),
            (
                "octocat",
                &TimelineKind::LineComment {
                    path: "src/form.rs".into(),
                    line: Some(7)
                }
            ),
            ("monalisa", &TimelineKind::Review(ReviewState::Approved)),
        ]
    );

    // Replies join their thread; the outdated one is not on a current line.
    let threads: Vec<(&str, &str, Option<u64>, DiffSide, usize)> = d
        .threads
        .iter()
        .map(|t| {
            (
                t.id.as_str(),
                t.path.as_str(),
                t.line,
                t.side,
                t.comments.len(),
            )
        })
        .collect();
    assert_eq!(
        threads,
        [
            ("20", "src/login.rs", None, DiffSide::Right, 2),
            ("23", "src/form.rs", Some(7), DiffSide::Left, 2),
        ]
    );
    assert_eq!(d.threads[1].comments[1].body, "Dead code.");
    assert!(d.threads[1].is_on("src/form.rs", Some(7), None));
}

#[test]
fn comments_on_lines_and_replies_in_threads() {
    let fx = Arc::new(
        Fixtures::new(BASE, "github")
            .on_status("POST /repos/acme/widgets/pulls/12/comments", 201, &[], "")
            .on_status(
                "POST /repos/acme/widgets/pulls/12/comments/23/replies",
                201,
                &[],
                "",
            ),
    );
    let gh = forge(fx.clone(), Arc::default());
    let detail = PrDetail {
        pr: ferrumgit_forge::PullRequest {
            number: 12,
            head_sha: "1212".into(),
            ..Default::default()
        },
        ..PrDetail::default()
    };
    let removed = NewLineComment {
        path: "src/form.rs".into(),
        old_line: Some(9),
        new_line: None,
        body: "Keep this?".into(),
    };
    run(gh.line_comment(&detail, &removed)).expect("line comment");
    let body: serde_json::Value = serde_json::from_str(
        fx.sent_to("POST /repos/acme/widgets/pulls/12/comments")
            .and_then(|r| r.body)
            .as_deref()
            .expect("body"),
    )
    .expect("json");
    assert_eq!(
        body,
        serde_json::json!({"body": "Keep this?", "commit_id": "1212", "path": "src/form.rs", "line": 9, "side": "LEFT"})
    );
    run(gh.reply(12, "23", "Agreed")).expect("reply");
    let reply: serde_json::Value = serde_json::from_str(
        fx.sent_to("POST /repos/acme/widgets/pulls/12/comments/23/replies")
            .and_then(|r| r.body)
            .as_deref()
            .expect("body"),
    )
    .expect("json");
    assert_eq!(reply, serde_json::json!({"body": "Agreed"}));

    // Empty text or no line: nothing is sent.
    let before = fx.sent().len();
    assert!(matches!(
        run(gh.reply(12, "23", " ")),
        Err(ForgeError::Invalid(_))
    ));
    assert!(matches!(
        run(gh.line_comment(
            &detail,
            &NewLineComment {
                body: "x".into(),
                path: "a".into(),
                ..NewLineComment::default()
            }
        )),
        Err(ForgeError::Invalid(_))
    ));
    assert_eq!(fx.sent().len(), before);
}

#[test]
fn creates_comments_reviews_and_merges() {
    let fx = Arc::new(
        Fixtures::new(BASE, "github")
            .on_status("POST /repos/acme/widgets/pulls", 201, &[], "created.json")
            .on_status("POST /repos/acme/widgets/issues/12/comments", 201, &[], "")
            .on("POST /repos/acme/widgets/pulls/12/reviews", "")
            .on_status(
                "PUT /repos/acme/widgets/pulls/12/merge",
                409,
                &[],
                "merge_conflict.json",
            ),
    );
    let gh = forge(fx.clone(), Arc::default());
    let created = run(gh.create(&NewPullRequest {
        title: "  New thing ".into(),
        body: "Body".into(),
        head: "new-thing".into(),
        base: "main".into(),
        draft: true,
    }))
    .expect("create");
    assert_eq!(created.number, 16);
    assert!(created.draft);
    let body: serde_json::Value = serde_json::from_str(
        fx.sent_to("POST /repos/acme/widgets/pulls")
            .and_then(|r| r.body)
            .as_deref()
            .expect("body"),
    )
    .expect("json");
    assert_eq!(
        body,
        serde_json::json!({"title": "New thing", "body": "Body", "head": "new-thing", "base": "main", "draft": true})
    );

    run(gh.comment(12, "Thanks!")).expect("comment");
    run(gh.review(12, ReviewEvent::Approve, "")).expect("approve");
    let review: serde_json::Value = serde_json::from_str(
        fx.sent_to("POST /repos/acme/widgets/pulls/12/reviews")
            .and_then(|r| r.body)
            .as_deref()
            .expect("body"),
    )
    .expect("json");
    assert_eq!(review, serde_json::json!({"event": "APPROVE"}));

    // Text is required to request changes; nothing is sent.
    let before = fx.sent().len();
    assert!(matches!(
        run(gh.review(12, ReviewEvent::RequestChanges, "  ")),
        Err(ForgeError::Invalid(_))
    ));
    assert!(matches!(
        run(gh.create(&NewPullRequest {
            title: "x".into(),
            head: "main".into(),
            base: "main".into(),
            ..NewPullRequest::default()
        })),
        Err(ForgeError::Invalid(_))
    ));
    assert_eq!(fx.sent().len(), before);

    let err = run(gh.merge(12, MergeMethod::Squash, "abc")).expect_err("conflict");
    assert_eq!(
        err.to_string(),
        "Head branch was modified. Review and try the merge again."
    );
    let merge: serde_json::Value = serde_json::from_str(
        fx.sent_to("PUT /repos/acme/widgets/pulls/12/merge")
            .and_then(|r| r.body)
            .as_deref()
            .expect("body"),
    )
    .expect("json");
    assert_eq!(
        merge,
        serde_json::json!({"merge_method": "squash", "sha": "abc"})
    );
}
