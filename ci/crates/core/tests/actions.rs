//! GitHub Actions endpoints against recorded API responses (no network).

use async_trait::async_trait;
use ferrumci_core::cache::EtagTransport;
use ferrumci_core::logs::{JobLog, LineKind};
use ferrumci_core::{
    CheckState, Client, ForgeError, ForgeResult, RateLimits, RunFilter, RunState, StatusFilter,
    Transport, repo,
};
use ferrumgit_domain::Secret;
use ferrumgit_forge::{HttpRequest, HttpResponse, Method};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

const BASE: &str = "https://api.github.com";

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {path}: {e}"))
}

#[derive(Default)]
struct Fixtures {
    routes: HashMap<String, HttpResponse>,
    sent: Mutex<Vec<HttpRequest>>,
}

impl Fixtures {
    fn on(self, route: &str, file: &str) -> Self {
        self.on_status(route, 200, file)
    }

    fn on_status(mut self, route: &str, status: u16, file: &str) -> Self {
        let body = if file.is_empty() {
            String::new()
        } else {
            fixture(file)
        };
        self.routes.insert(
            route.to_owned(),
            HttpResponse {
                status,
                headers: vec![
                    ("x-ratelimit-remaining".into(), "4321".into()),
                    ("x-ratelimit-reset".into(), "1999999999".into()),
                ],
                body,
            },
        );
        self
    }

    fn sent_to(&self, route: &str) -> Option<HttpRequest> {
        self.sent
            .lock()
            .expect("lock")
            .iter()
            .find(|r| key(r) == route)
            .cloned()
    }
}

fn key(r: &HttpRequest) -> String {
    format!(
        "{} {}",
        r.method.as_str(),
        r.url.strip_prefix(BASE).unwrap_or(&r.url)
    )
}

#[async_trait]
impl Transport for Fixtures {
    async fn send(&self, request: HttpRequest) -> ForgeResult<HttpResponse> {
        let k = key(&request);
        self.sent.lock().expect("lock").push(request);
        let Some(r) = self.routes.get(&k).cloned() else {
            panic!("no fixture for {k}");
        };
        Ok(r)
    }
}

fn client(f: Fixtures) -> (Client, Arc<Fixtures>) {
    let f = Arc::new(f);
    let status = repo::from_remotes(&[("origin".into(), "git@github.com:octo/app.git".into())]);
    let repo::RepoStatus::GitHub(forge_repo) = status else {
        panic!("not github");
    };
    let transport = Arc::new(EtagTransport::new(f.clone()));
    let c = Client::new(
        forge_repo,
        transport,
        Secret::new("ghp_test"),
        Arc::new(RateLimits::default()),
    );
    (c, f)
}

const R: &str = "/repos/octo/app";

#[tokio::test]
async fn lists_runs_with_filters_and_tracks_rate_limit() {
    let route =
        format!("GET {R}/actions/workflows/11/runs?per_page=30&branch=main&status=in_progress");
    let (c, f) = client(Fixtures::default().on(&route, "runs.json"));
    let filter = RunFilter {
        workflow_id: Some(11),
        branch: Some("main".into()),
        event: None,
        status: Some(StatusFilter::InProgress),
    };
    let page = c.actions.runs(&filter, 30).await.expect("runs");
    assert_eq!(page.total_count, 3);
    let runs = page.workflow_runs;
    assert_eq!(runs[0].state(), RunState::InProgress);
    assert_eq!(runs[0].title(), "Fix flaky test");
    assert_eq!(runs[0].actor(), "octocat");
    assert_eq!(runs[1].state(), RunState::Failure);
    assert_eq!(runs[1].run_attempt, 2);
    assert_eq!(runs[1].duration_secs(chrono::Utc::now()), 205);
    assert_eq!(runs[2].state(), RunState::Success);
    assert_eq!(c.actions.rate().map(|r| r.remaining), Some(4321));
    let sent = f.sent_to(&route).expect("sent");
    assert!(
        sent.headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer ghp_test")
    );
    // Debug output never shows the token.
    assert!(!format!("{sent:?}").contains("ghp_test"));
}

#[tokio::test]
async fn run_jobs_steps_and_logs() {
    let (c, _) = client(
        Fixtures::default()
            .on(&format!("GET {R}/actions/runs/900"), "run_900.json")
            .on(
                &format!("GET {R}/actions/runs/900/jobs?filter=latest&per_page=100"),
                "jobs_900.json",
            )
            .on(&format!("GET {R}/actions/jobs/5001/logs"), "log_5001.txt"),
    );
    let run = c.actions.run(900).await.expect("run");
    assert_eq!(run.workflow_name(), "CI");
    let jobs = c.actions.jobs(900).await.expect("jobs");
    assert_eq!(jobs.len(), 2);
    let now = chrono::Utc::now();
    assert_eq!(jobs[0].state(), RunState::Failure);
    assert_eq!(jobs[0].duration_secs(now), Some(190));
    assert_eq!(jobs[0].steps[1].state(), RunState::Failure);
    assert_eq!(jobs[0].steps[1].duration_secs(now), Some(186));
    assert_eq!(jobs[0].steps[2].state(), RunState::Skipped);
    assert_eq!(jobs[0].steps[2].duration_secs(now), None);

    let text = c.actions.job_log(5001).await.expect("log");
    let log = JobLog::parse(&text);
    assert_eq!(log.lines.len(), 4);
    assert_eq!(log.lines[0].kind, LineKind::Group);
    assert_eq!(log.lines[1].text, "cargo test");
    assert_eq!(log.lines[2].text, "test tests::flaky ... FAILED");
    assert_eq!(log.lines[3].kind, LineKind::Error);
    assert_eq!(log.errors(), vec![3]);
}

#[tokio::test]
async fn rerun_cancel_and_dispatch() {
    let (c, f) = client(
        Fixtures::default()
            .on_status(&format!("POST {R}/actions/runs/900/rerun"), 201, "")
            .on_status(
                &format!("POST {R}/actions/runs/900/rerun-failed-jobs"),
                201,
                "",
            )
            .on_status(
                &format!("POST {R}/actions/runs/900/cancel"),
                409,
                "cancel_conflict.json",
            )
            .on_status(
                &format!("POST {R}/actions/workflows/12/dispatches"),
                204,
                "",
            ),
    );
    c.actions.rerun(900, false).await.expect("rerun");
    c.actions.rerun(900, true).await.expect("rerun failed");
    let err = c.actions.cancel(900).await.expect_err("completed");
    assert_eq!(
        err,
        ForgeError::Rejected("Cannot cancel a workflow run that is completed.".into())
    );
    let inputs = BTreeMap::from([("environment".to_owned(), "production".to_owned())]);
    c.actions
        .dispatch(12, "main", &inputs)
        .await
        .expect("dispatch");
    let sent = f
        .sent_to(&format!("POST {R}/actions/workflows/12/dispatches"))
        .expect("sent");
    let body: serde_json::Value =
        serde_json::from_str(sent.body.as_deref().expect("body")).expect("json");
    assert_eq!(
        body,
        serde_json::json!({"ref": "main", "inputs": {"environment": "production"}})
    );
    assert!(matches!(
        c.actions.dispatch(12, " ", &inputs).await,
        Err(ForgeError::Invalid(_))
    ));
    assert_eq!(
        f.sent_to(&format!("POST {R}/actions/runs/900/rerun"))
            .map(|r| r.method),
        Some(Method::Post)
    );
}

#[tokio::test]
async fn workflows_inputs_branches_and_default_branch() {
    let (c, _) = client(
        Fixtures::default()
            .on(
                &format!("GET {R}/actions/workflows?per_page=100"),
                "workflows.json",
            )
            .on(
                &format!("GET {R}/contents/.github/workflows/deploy.yml?ref=feature%2Fx"),
                "contents_deploy.json",
            )
            .on(&format!("GET {R}/branches?per_page=100"), "branches.json")
            .on(&format!("GET {R}"), "repo.json"),
    );
    let workflows = c.actions.workflows().await.expect("workflows");
    assert_eq!(workflows.len(), 2);
    let spec = c
        .actions
        .dispatch_spec(&workflows[1], "feature/x")
        .await
        .expect("spec")
        .expect("dispatchable");
    assert_eq!(spec.inputs.len(), 2);
    assert_eq!(spec.defaults()["dry_run"], "true");
    assert_eq!(
        c.actions.branches().await.expect("branches"),
        ["main", "feature/x"]
    );
    assert_eq!(c.actions.default_branch().await.expect("default"), "main");
}

#[tokio::test]
async fn pull_requests_with_checks_reuse_the_forge_listing() {
    let (c, _) = client(
        Fixtures::default()
            .on("GET /user", "user.json")
            .on(
                &format!("GET {R}/pulls?state=open&sort=updated&direction=desc&per_page=50"),
                "pulls.json",
            )
            .on(
                &format!("GET {R}/pulls/12/reviews?per_page=100"),
                "reviews_12.json",
            )
            .on(
                &format!("GET {R}/commits/abc12/check-runs?per_page=100"),
                "check_runs_abc12.json",
            )
            .on(
                &format!("GET {R}/commits/abc12/status"),
                "status_abc12.json",
            ),
    );
    let prs = c.pull_requests().await.expect("prs");
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0].head_branch, "feature/x");
    assert_eq!(prs[0].checks, CheckState::Failure);
}

#[tokio::test]
async fn bad_token_is_a_clear_error() {
    let (c, _) = client(Fixtures::default().on_status("GET /user", 401, "bad_credentials.json"));
    let err = c.viewer().await.expect_err("401");
    assert!(matches!(err, ForgeError::Unauthorized { .. }));
    assert!(!err.to_string().contains("ghp_test"));
}
