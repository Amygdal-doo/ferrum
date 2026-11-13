//! The CI page, headless: the real views and background runtime, with
//! recorded GitHub responses instead of the network and an in-memory token
//! store instead of the keychain. No window is shown.

use crate::ci::services::Services;
use crate::ci::{CiView, Screen, Tab};
use crate::tests::{init, primary};
use crate::workspace::Workspace;
use async_trait::async_trait;
use ferrumci_core::auth::{FoundToken, TokenOrigin, TokenProvider};
use ferrumci_core::repo::{self, RepoStatus};
use ferrumci_core::{ForgeRepo, ForgeResult, Transport};
use ferrumgit_domain::Secret;
use ferrumgit_forge::{HttpRequest, HttpResponse};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const R: &str = "/repos/octo/app";

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/../../../ci/crates/core/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {path}: {e}"))
}

/// Recorded responses by `"METHOD /path"`; unknown routes answer 404.
#[derive(Default)]
struct Fixtures {
    routes: Mutex<HashMap<String, (u16, String)>>,
    sent: Mutex<Vec<HttpRequest>>,
}

impl Fixtures {
    fn on(&self, route: &str, status: u16, body: String) {
        self.routes
            .lock()
            .expect("lock")
            .insert(route.to_owned(), (status, body));
    }

    fn sent(&self, route: &str) -> Option<HttpRequest> {
        self.sent
            .lock()
            .expect("lock")
            .iter()
            .rev()
            .find(|r| key(r) == route)
            .cloned()
    }
}

fn key(r: &HttpRequest) -> String {
    let path = r
        .url
        .strip_prefix("https://api.github.com")
        .unwrap_or(&r.url);
    format!("{} {path}", r.method.as_str())
}

#[async_trait]
impl Transport for Fixtures {
    async fn send(&self, request: HttpRequest) -> ForgeResult<HttpResponse> {
        let k = key(&request);
        self.sent.lock().expect("lock").push(request);
        let (status, body) = self
            .routes
            .lock()
            .expect("lock")
            .get(&k)
            .cloned()
            .unwrap_or((404, format!("{{\"message\":\"Not Found: {k}\"}}")));
        Ok(HttpResponse {
            status,
            headers: vec![
                ("x-ratelimit-remaining".into(), "4999".into()),
                ("x-ratelimit-reset".into(), "1999999999".into()),
            ],
            body,
        })
    }
}

#[derive(Default)]
struct Tokens {
    token: Mutex<Option<String>>,
}

impl TokenProvider for Tokens {
    fn find(&self, _: &str) -> Option<FoundToken> {
        self.token
            .lock()
            .expect("lock")
            .clone()
            .map(|t| FoundToken {
                token: Secret::new(t),
                origin: TokenOrigin::GhCli,
            })
    }

    fn save(&self, _: &str, token: &str) -> Result<(), String> {
        let t = ferrumgit_forge::auth::clean_token(token)?;
        *self.token.lock().expect("lock") = Some(t);
        Ok(())
    }
}

struct Harness {
    fixtures: Arc<Fixtures>,
    tokens: Arc<Tokens>,
    notified: Arc<Mutex<Vec<String>>>,
}

fn github_repo() -> ForgeRepo {
    match repo::from_remotes(&[("origin".into(), "git@github.com:octo/app.git".into())]) {
        RepoStatus::GitHub(r) => r,
        other => panic!("{other:?}"),
    }
}

/// Fake services: recorded GitHub answers, a token store, a notifier.
fn services(signed_in: bool, cx: &mut TestAppContext) -> (Services, Harness) {
    // Requests run on the page's tokio threads.
    cx.executor().allow_parking();
    init(cx);
    let fixtures = Arc::new(Fixtures::default());
    for (route, file) in [
        (format!("GET {R}/actions/runs?per_page=30"), "runs.json"),
        (
            format!("GET {R}/actions/workflows?per_page=100"),
            "workflows.json",
        ),
        (
            format!("GET {R}/actions/runs/900/jobs?filter=latest&per_page=100"),
            "jobs_900.json",
        ),
        (format!("GET {R}/actions/jobs/5001/logs"), "log_5001.txt"),
        (format!("GET {R}/branches?per_page=100"), "branches.json"),
        (format!("GET {R}"), "repo.json"),
        (
            format!("GET {R}/contents/.github/workflows/deploy.yml?ref=main"),
            "contents_deploy.json",
        ),
        ("GET /user".to_owned(), "user.json"),
        (
            format!("GET {R}/pulls?state=open&sort=updated&direction=desc&per_page=50"),
            "pulls.json",
        ),
        (
            format!("GET {R}/pulls/12/reviews?per_page=100"),
            "reviews_12.json",
        ),
        (
            format!("GET {R}/commits/abc12/check-runs?per_page=100"),
            "check_runs_abc12.json",
        ),
        (format!("GET {R}/commits/abc12/status"), "status_abc12.json"),
        (
            format!("GET {R}/actions/runs?per_page=30&branch=feature%2Fx"),
            "runs.json",
        ),
        (
            format!("GET {R}/actions/runs?per_page=30&branch=main"),
            "runs.json",
        ),
    ] {
        fixtures.on(&route, 200, fixture(file));
    }
    let tokens = Arc::new(Tokens::default());
    if signed_in {
        *tokens.token.lock().expect("lock") = Some("ghp_secret_test".into());
    }
    let notified = Arc::new(Mutex::new(Vec::new()));
    let sink = notified.clone();
    let services = Services::with(
        fixtures.clone(),
        tokens.clone(),
        Arc::new(move |title: &str, body: &str| {
            sink.lock().expect("lock").push(format!("{title}: {body}"));
        }),
        Arc::new(Vec::new),
    )
    .expect("runtime");
    (
        services,
        Harness {
            fixtures,
            tokens,
            notified,
        },
    )
}

/// The CI page on its own, showing octo/app.
fn setup(
    signed_in: bool,
    cx: &mut TestAppContext,
) -> (Entity<CiView>, &mut VisualTestContext, Harness) {
    let (services, harness) = services(signed_in, cx);
    let (app, cx) = cx.add_window_view(move |window, cx| CiView::new(services, window, cx));
    app.update(cx, |app, cx| {
        app.set_repo(RepoStatus::GitHub(github_repo()), cx)
    });
    (app, cx, harness)
}

fn wait_until(
    cx: &mut VisualTestContext,
    what: &str,
    mut done: impl FnMut(&mut VisualTestContext) -> bool,
) {
    for _ in 0..400 {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        cx.executor().advance_clock(Duration::from_millis(20));
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("timed out waiting for {what}");
}

fn frames(app: &Entity<CiView>, cx: &mut VisualTestContext) -> u64 {
    app.read_with(cx, |a, _| a.frames)
}

#[gpui::test]
fn lists_runs_opens_a_failed_run_and_its_log(cx: &mut TestAppContext) {
    let (app, cx, h) = setup(true, cx);
    wait_until(cx, "runs", |cx| app.read_with(cx, |a, _| a.runs.len() == 3));
    wait_until(cx, "workflows", |cx| {
        app.read_with(cx, |a, _| a.workflows.len() == 2)
    });
    app.read_with(cx, |a, _| {
        assert!(matches!(a.screen, Screen::Ready));
        assert_eq!(a.token_origin, Some(TokenOrigin::GhCli));
        assert!(a.is_active(), "run 901 is in progress");
        assert_eq!(
            a.client
                .as_ref()
                .and_then(|c| c.actions.rate())
                .map(|r| r.remaining),
            Some(4999)
        );
    });
    let before = frames(&app, cx);

    let failed = app.read_with(cx, |a, _| a.runs[1].clone());
    app.update(cx, |a, cx| a.open_run(failed, cx));
    wait_until(cx, "log", |cx| {
        app.read_with(cx, |a, _| {
            a.detail
                .as_ref()
                .and_then(|d| d.log.as_ref())
                .is_some_and(|l| l.log.is_some())
        })
    });
    app.read_with(cx, |a, _| {
        let d = a.detail.as_ref().expect("detail");
        assert_eq!(d.jobs.len(), 2);
        assert_eq!(d.job, Some(5001), "the failed job is selected");
        let view = d.log.as_ref().expect("log view");
        let log = view.log.as_ref().expect("log");
        assert_eq!(log.errors(), vec![3]);
        assert!(view.collapsed.contains(&0), "quiet groups start folded");
    });
    assert!(frames(&app, cx) > before, "the detail view rendered");

    // Search unfolds the group holding the match.
    app.update(cx, |a, cx| {
        a.log_search
            .update(cx, |s, cx| s.set_text("cargo TEST", cx));
        a.search_log(cx);
    });
    app.read_with(cx, |a, _| {
        let view = a.detail.as_ref().and_then(|d| d.log.as_ref()).expect("log");
        assert_eq!(view.matches, vec![0, 1]);
        assert!(view.collapsed.is_empty());
    });
    app.update(cx, |a, cx| a.next_match(1, cx));
    app.read_with(cx, |a, _| {
        assert_eq!(
            a.detail
                .as_ref()
                .and_then(|d| d.log.as_ref())
                .map(|l| l.current),
            Some(1)
        );
    });

    // Re-run the failed jobs: the run is watched from now on.
    h.fixtures.on(
        &format!("POST {R}/actions/runs/900/rerun-failed-jobs"),
        201,
        String::new(),
    );
    app.update(cx, |a, cx| a.rerun(true, cx));
    wait_until(cx, "rerun", |cx| {
        app.read_with(cx, |a, _| a.watcher.is_watching(900))
    });
    assert!(
        h.fixtures
            .sent(&format!("POST {R}/actions/runs/900/rerun-failed-jobs"))
            .is_some()
    );
    app.read_with(cx, |a, _| {
        let notice = a.notice.as_ref().expect("notice");
        assert!(
            notice.message.contains("Failed jobs of CI #41"),
            "{}",
            notice.message
        );
    });
}

#[gpui::test]
fn watched_run_finishing_notifies(cx: &mut TestAppContext) {
    let (app, cx, h) = setup(true, cx);
    wait_until(cx, "runs", |cx| app.read_with(cx, |a, _| a.runs.len() == 3));
    h.fixtures.on(
        &format!("GET {R}/actions/runs/901/jobs?filter=latest&per_page=100"),
        200,
        r#"{"jobs":[]}"#.into(),
    );
    app.update(cx, |a, cx| {
        let run = a.runs[0].clone();
        a.open_run(run, cx);
        a.toggle_watch(cx);
    });
    app.read_with(cx, |a, _| assert!(a.watcher.is_watching(901)));
    let done = fixture("runs.json").replacen(
        r#""status": "in_progress", "conclusion": null"#,
        r#""status": "completed", "conclusion": "success""#,
        1,
    );
    assert!(done.contains(r#""conclusion": "success", "workflow_id": 11, "run_attempt": 1"#));
    h.fixtures
        .on(&format!("GET {R}/actions/runs?per_page=30"), 200, done);
    wait_until(cx, "idle", |cx| app.read_with(cx, |a, _| !a.runs_loading));
    app.update(cx, |a, cx| a.refresh(cx));
    wait_until(cx, "notified", |cx| {
        cx.run_until_parked();
        !h.notified.lock().expect("lock").is_empty()
    });
    assert_eq!(
        h.notified.lock().expect("lock").as_slice(),
        ["CI #42 succeeded: Fix flaky test"]
    );
    app.read_with(cx, |a, _| {
        assert!(!a.watcher.is_watching(901));
        assert!(
            a.notice
                .as_ref()
                .is_some_and(|n| n.message.contains("succeeded"))
        );
    });
}

#[gpui::test]
fn sign_in_saves_a_pasted_token(cx: &mut TestAppContext) {
    let (app, cx, h) = setup(false, cx);
    wait_until(cx, "sign-in", |cx| {
        app.read_with(cx, |a, _| matches!(a.screen, Screen::SignIn { .. }))
    });
    // A token with spaces is refused.
    app.update(cx, |a, cx| {
        a.token_input
            .update(cx, |t, cx| t.set_text("not a token", cx));
        a.save_token(cx);
    });
    wait_until(cx, "error", |cx| {
        app.read_with(cx, |a, _| {
            matches!(&a.screen, Screen::SignIn { error: Some(_), .. })
        })
    });
    app.update(cx, |a, cx| {
        a.token_input
            .update(cx, |t, cx| t.set_text("  ghp_pasted  ", cx));
        a.save_token(cx);
    });
    wait_until(cx, "runs", |cx| app.read_with(cx, |a, _| a.runs.len() == 3));
    assert_eq!(
        h.tokens.token.lock().expect("lock").as_deref(),
        Some("ghp_pasted")
    );
    app.read_with(cx, |a, cx| {
        assert!(a.token_input.read(cx).text().is_empty())
    });
    let sent = h
        .fixtures
        .sent(&format!("GET {R}/actions/runs?per_page=30"))
        .expect("sent");
    assert!(
        sent.headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer ghp_pasted")
    );

    // A revoked token sends the user back to sign in.
    h.fixtures.on(
        &format!("GET {R}/actions/runs?per_page=30"),
        401,
        r#"{"message":"Bad credentials"}"#.into(),
    );
    app.update(cx, |a, cx| a.refresh(cx));
    wait_until(cx, "signed out", |cx| {
        app.read_with(cx, |a, _| matches!(a.screen, Screen::SignIn { .. }))
    });
}

#[gpui::test]
fn run_workflow_sends_dispatch_with_inputs(cx: &mut TestAppContext) {
    let (app, cx, h) = setup(true, cx);
    wait_until(cx, "workflows", |cx| {
        app.read_with(cx, |a, _| a.workflows.len() == 2)
    });
    h.fixtures.on(
        &format!("POST {R}/actions/workflows/12/dispatches"),
        204,
        String::new(),
    );
    app.update(cx, |a, cx| a.open_dispatch(12, cx));
    wait_until(cx, "spec", |cx| {
        app.read_with(cx, |a, _| {
            a.dispatch
                .as_ref()
                .is_some_and(|d| matches!(d.spec, Some(Some(_))))
        })
    });
    app.read_with(cx, |a, _| {
        let form = a.dispatch.as_ref().expect("form");
        assert_eq!(form.git_ref, "main", "no local branch: the default branch");
        assert_eq!(form.refs, ["main", "feature/x"]);
        assert_eq!(form.values["environment"], "staging");
    });
    app.update(cx, |a, cx| {
        if let Some(f) = a.dispatch.as_mut() {
            f.values.insert("environment".into(), "production".into());
        }
        a.submit_dispatch(cx);
    });
    wait_until(cx, "dispatched", |cx| {
        app.read_with(cx, |a, _| a.dispatch.is_none())
    });
    let sent = h
        .fixtures
        .sent(&format!("POST {R}/actions/workflows/12/dispatches"))
        .expect("dispatch sent");
    let body: serde_json::Value =
        serde_json::from_str(sent.body.as_deref().expect("body")).expect("json");
    assert_eq!(
        body,
        serde_json::json!({"ref": "main", "inputs": {"environment": "production", "dry_run": "true"}})
    );
    app.read_with(cx, |a, _| {
        assert!(!a.watcher.is_empty(), "the new run will be watched")
    });
}

#[gpui::test]
fn pull_requests_show_checks_and_link_to_runs(cx: &mut TestAppContext) {
    let (app, cx, _h) = setup(true, cx);
    wait_until(cx, "runs", |cx| app.read_with(cx, |a, _| a.runs.len() == 3));
    app.update(cx, |a, cx| a.set_tab(Tab::PullRequests, cx));
    wait_until(cx, "prs", |cx| app.read_with(cx, |a, _| a.prs.len() == 1));
    app.read_with(cx, |a, _| {
        assert_eq!(a.prs[0].checks, ferrumci_core::CheckState::Failure);
    });
    let pr = app.read_with(cx, |a, _| a.prs[0].clone());
    app.update(cx, |a, cx| a.show_pr_runs(&pr, cx));
    wait_until(cx, "branch runs", |cx| {
        app.read_with(cx, |a, _| {
            a.tab == Tab::Runs && !a.runs_loading && a.last_refresh.is_some()
        })
    });
    app.read_with(cx, |a, cx| {
        assert_eq!(a.filter.branch.as_deref(), Some("feature/x"));
        assert_eq!(a.branch_input.read(cx).text(), "feature/x");
        assert!(a.error.is_none(), "{:?}", a.error);
    });
}

#[gpui::test]
fn gitlab_and_non_forge_folders_explain_themselves(cx: &mut TestAppContext) {
    let (app, cx, _h) = setup(true, cx);
    let gitlab = repo::from_remotes(&[("origin".into(), "git@gitlab.com:g/p.git".into())]);
    app.update(cx, |a, cx| a.set_repo(gitlab, cx));
    cx.run_until_parked();
    app.read_with(cx, |a, _| {
        assert!(matches!(a.screen, Screen::GitLab(_)));
        assert!(a.client.is_none());
    });
    app.update(cx, |a, cx| a.set_repo(RepoStatus::NoForge, cx));
    cx.run_until_parked();
    app.read_with(cx, |a, _| match &a.screen {
        Screen::Empty { detail, .. } => assert!(detail.contains("GitHub")),
        _ => panic!("expected the empty screen"),
    });
}

/// A folder that looks like a clone of github.com/octo/app on `main`.
fn github_folder() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    let git = root.join(".git");
    for sub in ["objects", "refs/heads", "refs/tags"] {
        std::fs::create_dir_all(git.join(sub)).expect("mkdir");
    }
    std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    std::fs::write(
        git.join("config"),
        "[core]\n\trepositoryformatversion = 0\n\tbare = false\n\
         [remote \"origin\"]\n\turl = git@github.com:octo/app.git\n",
    )
    .expect("config");
    (dir, root)
}

#[gpui::test]
fn the_ci_page_opens_from_the_workspace_on_the_project(cx: &mut TestAppContext) {
    let (services, h) = services(true, cx);
    let (_dir, root) = github_folder();
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    workspace.update_in(cx, |w, window, cx| {
        w.set_ci_services(services);
        w.set_root(root.clone(), window, cx);
    });
    assert!(
        workspace.read_with(cx, |w, _| w.ci().is_none()),
        "the page starts on first use"
    );

    // ⇧⌘I opens the CI page on the editor's project.
    cx.simulate_keystrokes(&primary("cmd-shift-i"));
    assert!(workspace.read_with(cx, |w, _| w.in_ci_view()));
    let ci = workspace.read_with(cx, |w, _| w.ci().cloned().expect("ci page"));
    wait_until(cx, "runs", |cx| ci.read_with(cx, |a, _| a.runs.len() == 3));
    ci.read_with(cx, |a, _| {
        assert_eq!(a.project.as_deref(), Some(root.as_path()));
        assert_eq!(a.local_branch.as_deref(), Some("main"));
        assert_eq!(a.filter.branch.as_deref(), Some("main"));
        assert!(a.frames > 0, "the page rendered");
    });
    assert!(
        h.fixtures
            .sent(&format!("GET {R}/actions/runs?per_page=30&branch=main"))
            .is_some()
    );

    // ⌘R on the page refreshes through `ci:refresh`.
    wait_until(cx, "idle", |cx| ci.read_with(cx, |a, _| !a.runs_loading));
    let seq = ci.read_with(cx, |a, _| a.runs_seq);
    cx.simulate_keystrokes(&primary("cmd-r"));
    assert!(ci.read_with(cx, |a, _| a.runs_seq > seq));

    // The palette has the CI commands.
    workspace.update_in(cx, |w, window, cx| {
        w.run_command("ci:search-log", window, cx)
    });

    // ⇧⌘I again goes back to the editor; reopening keeps what was loaded.
    cx.simulate_keystrokes(&primary("cmd-shift-i"));
    assert!(!workspace.read_with(cx, |w, _| w.in_ci_view()));
    workspace.update_in(cx, |w, window, cx| {
        w.run_command("ci:toggle-view", window, cx)
    });
    assert!(workspace.read_with(cx, |w, _| w.in_ci_view()));
    ci.read_with(cx, |a, _| assert_eq!(a.runs.len(), 3));

    // Other panes open beside it: ⇧⌘Y adds the Debug pane.
    cx.simulate_keystrokes(&primary("cmd-shift-y"));
    assert!(workspace.read_with(cx, |w, _| w.in_ci_view()
        && w.tool_open(ferrumeditor_core::tool_layout::ToolKind::Debug)));
}

#[gpui::test]
fn the_ci_page_without_a_folder_explains_itself(cx: &mut TestAppContext) {
    let (services, _h) = services(true, cx);
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    workspace.update(cx, |w, _| w.set_ci_services(services));
    workspace.update_in(cx, |w, window, cx| {
        w.run_command("ci:toggle-view", window, cx)
    });
    let ci = workspace.read_with(cx, |w, _| w.ci().cloned().expect("ci page"));
    cx.run_until_parked();
    ci.read_with(cx, |a, _| match &a.screen {
        Screen::Empty { title, .. } => assert_eq!(title, "No project open"),
        _ => panic!("expected the empty screen"),
    });
}
