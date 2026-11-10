//! GitLab REST v4 mapping (self-managed host, nested group), replayed from
//! recorded fixtures (no network).

mod common;

use common::Fixtures;
use ferrumgit_domain::{ForgeKind, RepoForge, Secret};
use ferrumgit_forge::{
    Api, CheckState, Forge, ForgeError, MergeMethod, NewPullRequest, PrFilter, ReviewDecision,
    ReviewEvent, TimelineKind, client, resolve,
};
use std::sync::Arc;

const BASE: &str = "https://gitlab.example.com/api/v4";
const P: &str = "/projects/platform%2Ftools%2Fapp";
const TOKEN: &str = "glpat-test-not-real";

fn forge(fx: Arc<Fixtures>) -> Box<dyn Forge> {
    let repo = resolve(
        &[(
            "origin".into(),
            "ssh://git@gitlab.example.com:2222/platform/tools/app.git".into(),
        )],
        &RepoForge::default(),
    )
    .expect("repo");
    assert_eq!(repo.kind, ForgeKind::GitLab);
    assert_eq!(repo.api_url, BASE);
    let api = Api::new(
        fx,
        repo.kind,
        &repo.host,
        &repo.api_url,
        Secret::new(TOKEN),
        Arc::default(),
    );
    client(repo, api)
}

fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("rt")
        .block_on(f)
}

fn listing(query: &str) -> Fixtures {
    Fixtures::new(BASE, "gitlab")
        .on("GET /user", "user.json")
        .on(
            &format!("GET {P}/merge_requests?state=opened&order_by=updated_at&sort=desc&per_page=50{query}"),
            "mrs.json",
        )
        .on(&format!("GET {P}/merge_requests/4"), "mr_4.json")
        .on(&format!("GET {P}/merge_requests/6"), "mr_6.json")
        .on(&format!("GET {P}/merge_requests/4/approvals"), "approvals_4.json")
        .on(&format!("GET {P}/merge_requests/6/approvals"), "approvals_6.json")
}

#[test]
fn lists_merge_requests_with_pipeline_and_approval_status() {
    let fx = Arc::new(listing(""));
    let list = run(forge(fx.clone()).list(PrFilter::All)).expect("list");
    assert_eq!(list.viewer, "jdoe");
    assert_eq!(
        list.prs.iter().map(|p| p.number).collect::<Vec<_>>(),
        [6, 4]
    );

    let fix = &list.prs[0];
    assert_eq!(fix.title, "Fix crash on start");
    assert_eq!(
        (fix.head_branch.as_str(), fix.base_branch.as_str()),
        ("fix-crash", "main")
    );
    assert!(fix.from_fork, "source project differs from target");
    assert_eq!(fix.review, ReviewDecision::Approved);
    assert_eq!(fix.checks, CheckState::Failure);

    let ci = &list.prs[1];
    assert!(ci.draft);
    assert_eq!(ci.labels, ["ci"]);
    assert_eq!(ci.review, ReviewDecision::ReviewRequired);
    assert_eq!(ci.checks, CheckState::Pending);

    assert!(fx.sent().iter().all(|r| {
        r.headers
            .iter()
            .any(|(k, v)| k == "private-token" && v == TOKEN)
    }));
}

#[test]
fn filters_on_the_server() {
    let fx = Arc::new(listing("&author_username=jdoe"));
    let mine = run(forge(fx).list(PrFilter::Mine)).expect("mine");
    // The fixture returns both; the client still drops other authors.
    assert_eq!(mine.prs.iter().map(|p| p.number).collect::<Vec<_>>(), [4]);
    let fx = Arc::new(listing("&reviewer_username=jdoe"));
    let asked = run(forge(fx).list(PrFilter::ReviewRequested)).expect("review");
    assert_eq!(asked.prs.iter().map(|p| p.number).collect::<Vec<_>>(), [6]);
}

#[test]
fn loads_detail_with_jobs_notes_and_merge_options() {
    let fx = Arc::new(
        Fixtures::new(BASE, "gitlab")
            .on(&format!("GET {P}/merge_requests/6"), "mr_6.json")
            .on(
                &format!("GET {P}/merge_requests/6/commits?per_page=100"),
                "commits_6.json",
            )
            .on(
                &format!(
                    "GET {P}/merge_requests/6/notes?sort=asc&order_by=created_at&per_page=100"
                ),
                "notes_6.json",
            )
            .on(
                &format!("GET {P}/merge_requests/6/approvals"),
                "approvals_6.json",
            )
            .on(&format!("GET {P}"), "project.json")
            .on(
                &format!("GET {P}/pipelines/901/jobs?per_page=100"),
                "jobs_901.json",
            ),
    );
    let d = run(forge(fx).detail(6)).expect("detail");
    assert!(d.body.contains("stack trace"));
    assert_eq!(d.mergeable, Some(true));
    assert_eq!(d.merge_methods, [MergeMethod::Rebase, MergeMethod::Squash]);
    assert_eq!(d.commits[0].summary, "Guard against missing config");
    assert_eq!(d.commits[0].author, "Alex Smith");
    let checks: Vec<(&str, CheckState)> = d
        .checks
        .iter()
        .map(|c| (c.name.as_str(), c.state))
        .collect();
    assert_eq!(
        checks,
        [
            ("build", CheckState::Success),
            ("flaky-e2e", CheckState::Neutral),
            ("unit", CheckState::Failure)
        ]
    );
    assert_eq!(d.checks[2].detail, "test · failed");
    assert_eq!(d.pr.checks, CheckState::Failure);
    let kinds: Vec<&TimelineKind> = d.timeline.iter().map(|t| &t.kind).collect();
    assert_eq!(
        kinds,
        [
            &TimelineKind::Event,
            &TimelineKind::LineComment {
                path: "src/main.rs".into(),
                line: Some(17)
            },
            &TimelineKind::Comment,
            &TimelineKind::Event,
        ]
    );
}

#[test]
fn creates_reviews_and_merges() {
    let mr6 = format!("{P}/merge_requests/6");
    let fx = Arc::new(
        Fixtures::new(BASE, "gitlab")
            .on_status(
                &format!("POST {P}/merge_requests"),
                201,
                &[],
                "created.json",
            )
            .on_status(&format!("POST {mr6}/notes"), 201, &[], "note.json")
            .on_status(&format!("POST {mr6}/approve"), 201, &[], "approve.json")
            .on_status(&format!("POST {mr6}/unapprove"), 404, &[], "not_found.json")
            .on_status(&format!("PUT {mr6}/merge"), 405, &[], "merge_refused.json"),
    );
    let gl = forge(fx.clone());
    let created = run(gl.create(&NewPullRequest {
        title: "Add docs".into(),
        body: "Docs.".into(),
        head: "docs".into(),
        base: "main".into(),
        draft: true,
    }))
    .expect("create");
    assert_eq!(created.number, 7);
    let body: serde_json::Value = serde_json::from_str(
        fx.sent_to(&format!("POST {P}/merge_requests"))
            .and_then(|r| r.body)
            .as_deref()
            .expect("body"),
    )
    .expect("json");
    assert_eq!(
        body,
        serde_json::json!({"source_branch": "docs", "target_branch": "main", "title": "Draft: Add docs", "description": "Docs."})
    );

    run(gl.review(6, ReviewEvent::Approve, "")).expect("approve");
    // Not approved before: the 404 from unapprove is fine; the note is posted.
    run(gl.review(6, ReviewEvent::RequestChanges, "Needs a test")).expect("request changes");
    let notes: Vec<String> = fx
        .sent()
        .into_iter()
        .filter(|r| r.url.ends_with("/notes"))
        .filter_map(|r| r.body)
        .collect();
    assert_eq!(notes.len(), 1);
    assert!(notes[0].contains("Changes requested") && notes[0].contains("Needs a test"));

    let err = run(gl.merge(6, MergeMethod::Squash, "6666")).expect_err("refused");
    assert!(matches!(err, ForgeError::Rejected(ref m) if m == "Branch cannot be merged"));
    let merge: serde_json::Value = serde_json::from_str(
        fx.sent_to(&format!("PUT {mr6}/merge"))
            .and_then(|r| r.body)
            .as_deref()
            .expect("body"),
    )
    .expect("json");
    assert_eq!(merge["squash"], true);
    assert_eq!(merge["sha"], "6666");
}
