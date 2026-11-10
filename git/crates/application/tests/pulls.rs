//! Pull requests end to end without network: a local bare repository plays
//! GitHub's git side (`refs/pull/<n>/head`), reached through an `insteadOf`
//! rewrite of `https://github.com/acme/widgets.git`; a fake transport plays
//! the REST API. Tokens are fake and kept in session memory.

use async_trait::async_trait;
use ferrumgit_application::AppServices;
use ferrumgit_application::forge::{
    ForgeResult, HttpRequest, HttpResponse, NewPullRequest, PrFilter, Transport,
};
use ferrumgit_credentials::{CredentialStore, SessionStore};
use ferrumgit_domain::{RepoEntry, Secret, TokenSource};
use ferrumgit_git::Git;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

const API: &str = "https://api.github.com";

/// Answers the few endpoints these tests use; records every request.
#[derive(Default)]
struct FakeGitHub {
    sent: Mutex<Vec<HttpRequest>>,
}

fn pull(n: u64, head: &str, head_repo: &str, sha: &str) -> serde_json::Value {
    serde_json::json!({
        "number": n, "title": format!("PR {n}"), "state": "open", "draft": false,
        "user": {"login": "someone"}, "updated_at": format!("2026-11-0{n}T00:00:00Z"),
        "html_url": format!("https://github.com/acme/widgets/pull/{n}"),
        "requested_reviewers": [], "labels": [],
        "head": {"ref": head, "sha": sha, "repo": {"full_name": head_repo}},
        "base": {"ref": "main", "sha": "0", "repo": {"full_name": "acme/widgets"}}
    })
}

#[async_trait]
impl Transport for FakeGitHub {
    async fn send(&self, request: HttpRequest) -> ForgeResult<HttpResponse> {
        self.sent.lock().expect("lock").push(request.clone());
        let path = request
            .url
            .strip_prefix(API)
            .unwrap_or(&request.url)
            .to_owned();
        let (status, body) = match (request.method.as_str(), path.as_str()) {
            ("GET", "/user") => (200, serde_json::json!({"login": "me"})),
            ("GET", p) if p.starts_with("/repos/acme/widgets/pulls?") => (
                200,
                serde_json::json!([
                    pull(7, "feature", "acme/widgets", "7"),
                    pull(8, "patch-1", "fork/widgets", "8")
                ]),
            ),
            ("GET", p) if p.contains("/reviews") => (200, serde_json::json!([])),
            ("GET", p) if p.contains("/check-runs") => (200, serde_json::json!({"check_runs": []})),
            ("GET", p) if p.ends_with("/status") => (200, serde_json::json!({"statuses": []})),
            ("POST", "/repos/acme/widgets/pulls") => {
                let sent: serde_json::Value =
                    serde_json::from_str(request.body.as_deref().unwrap_or("{}")).expect("json");
                let head = sent["head"].as_str().unwrap_or_default().to_owned();
                (201, pull(9, &head, "acme/widgets", "9"))
            }
            other => panic!("unexpected request {other:?}"),
        };
        Ok(HttpResponse {
            status,
            headers: vec![("x-ratelimit-remaining".into(), "4999".into())],
            body: body.to_string(),
        })
    }
}

struct Env {
    _tmp: tempfile::TempDir,
    cfg: PathBuf,
    remote: PathBuf,
    work: PathBuf,
}

fn git(env: &Env, dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", &env.cfg)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn commit(env: &Env, file: &str, text: &str, msg: &str) {
    std::fs::write(env.work.join(file), text).expect("write");
    git(env, &env.work, &["add", file]);
    git(env, &env.work, &["commit", "-q", "-m", msg]);
}

/// main: base → later; feature (PR 7): base → f1 → f2; PR 8 (fork): base → fork-only.
fn setup() -> Env {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().to_path_buf();
    let remote = root.join("remote.git");
    let work = root.join("work");
    let cfg = root.join("gitconfig");
    std::fs::write(
        &cfg,
        format!(
            "[user]\n\tname = Tester\n\temail = t@example.com\n[commit]\n\tgpgsign = false\n[init]\n\tdefaultBranch = main\n[url \"{}\"]\n\tinsteadOf = https://github.com/acme/widgets.git\n",
            remote.display()
        ),
    )
    .expect("cfg");
    std::fs::create_dir_all(&work).expect("work");
    let env = Env {
        _tmp: tmp,
        cfg,
        remote,
        work,
    };
    git(&env, &root, &["init", "-q", "--bare", "remote.git"]);
    git(&env, &env.work, &["init", "-q"]);
    git(
        &env,
        &env.work,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/acme/widgets.git",
        ],
    );
    commit(&env, "base.txt", "base\n", "base");
    git(&env, &env.work, &["push", "-q", "origin", "main"]);
    git(&env, &env.work, &["switch", "-q", "-c", "feature"]);
    commit(&env, "feature.txt", "one\n", "f1");
    commit(&env, "feature.txt", "one\ntwo\n", "f2");
    git(
        &env,
        &env.work,
        &[
            "push",
            "-q",
            "origin",
            "feature",
            "feature:refs/pull/7/head",
        ],
    );
    git(&env, &env.work, &["switch", "-q", "--detach", "main"]);
    commit(&env, "fork.txt", "from a fork\n", "fork-only");
    git(
        &env,
        &env.work,
        &["push", "-q", "origin", "HEAD:refs/pull/8/head"],
    );
    git(&env, &env.work, &["switch", "-q", "main"]);
    commit(&env, "later.txt", "later\n", "later on main");
    git(&env, &env.work, &["push", "-q", "origin", "main"]);
    // Forget the local feature branch so checkout has to create it.
    git(&env, &env.work, &["branch", "-q", "-D", "feature"]);
    env
}

type GhToken = Arc<Mutex<Option<String>>>;

fn services(env: &Env) -> (AppServices, Arc<FakeGitHub>, Arc<SessionStore>, GhToken) {
    let git = Git::new()
        .with_env("GIT_CONFIG_GLOBAL", env.cfg.as_os_str())
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
        .with_askpass("/usr/bin/false");
    let fake = Arc::new(FakeGitHub::default());
    let tokens = Arc::new(SessionStore::default());
    let gh: GhToken = Arc::default();
    let gh_lookup = gh.clone();
    let svc = AppServices::start_with(
        &env.cfg.with_file_name("data"),
        git,
        Arc::new(SessionStore::default()),
    )
    .expect("services")
    .with_token_store(tokens.clone())
    .with_forge_transport(
        fake.clone(),
        Arc::new(move |_host: &str| gh_lookup.lock().expect("lock").clone().map(Secret::new)),
    );
    (svc, fake, tokens, gh)
}

fn block_on<F: std::future::Future<Output = ()> + Send + 'static>(svc: &AppServices, f: F) {
    let handle = svc.spawn(f);
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("rt");
    if let Err(e) = rt.block_on(handle) {
        std::panic::resume_unwind(e.into_panic());
    }
}

#[test]
fn lists_diffs_and_checks_out_pull_requests() {
    let env = setup();
    let (svc, fake, tokens, _) = services(&env);
    let s = svc.clone();
    let work = env.work.display().to_string();
    let remote = env.remote.clone();
    block_on(&svc, async move {
        let entry: RepoEntry = s.add_repo(&work).await.expect("add");

        // Detected from the configured URL (git shows the rewritten local path).
        let info = s.forge_info(&entry).await.expect("info");
        assert_eq!(info.repo.path(), "acme/widgets");
        assert!(!info.has_token);
        assert_eq!(info.help.scopes, "repo");
        let err = s
            .pull_requests(&entry, PrFilter::All)
            .await
            .expect_err("no token");
        assert!(
            err.to_string().contains("No access token for github.com"),
            "{err}"
        );

        s.set_forge_token("github.com", "  fake-token-1 \n")
            .await
            .expect("token");
        assert_eq!(
            tokens.get("github.com").expect("get").as_deref(),
            Some("fake-token-1")
        );
        assert!(s.forge_info(&entry).await.expect("info").has_token);

        let list = s.pull_requests(&entry, PrFilter::All).await.expect("list");
        assert_eq!(list.viewer, "me");
        let (pr7, pr8) = (list.prs[1].clone(), list.prs[0].clone());
        assert_eq!((pr7.number, pr8.number), (7, 8));
        assert!(fake.sent.lock().expect("lock").iter().all(|r| {
            r.headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer fake-token-1")
        }));

        // Diff against the merge base: main's later commit is not part of it.
        let changes = s
            .pull_request_changes(&entry, None, &pr7, None)
            .await
            .expect("changes");
        let files: Vec<&str> = changes.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(files, ["feature.txt"]);
        assert_eq!(changes.files[0].additions, 2);

        // Fetched PR refs stay out of the commit graph.
        s.pull_request_changes(&entry, None, &pr8, None)
            .await
            .expect("fork changes");
        let data = s.load(&entry.path, 100).await.expect("load");
        assert!(
            !data
                .snapshot
                .commits
                .iter()
                .any(|c| c.summary == "fork-only")
        );

        // Same-project PR: tracking branch with the head's name.
        let msg = s
            .checkout_pull_request(&entry, None, &pr7, None)
            .await
            .expect("checkout 7");
        assert_eq!(msg, "Checked out feature");
        let status = s.load(&entry.path, 10).await.expect("load").snapshot.status;
        assert_eq!(status.branch.head.as_deref(), Some("feature"));
        assert_eq!(status.branch.upstream.as_deref(), Some("origin/feature"));

        // Fork PR: a prefixed local branch at the PR head; again is a no-op switch.
        let msg = s
            .checkout_pull_request(&entry, None, &pr8, None)
            .await
            .expect("checkout 8");
        assert_eq!(msg, "Checked out pr-8-patch-1");
        s.checkout_pull_request(&entry, None, &pr8, None)
            .await
            .expect("checkout 8 again");
        let head = s.load(&entry.path, 10).await.expect("load").snapshot;
        assert_eq!(head.status.branch.head.as_deref(), Some("pr-8-patch-1"));
        let tip = head
            .commits
            .iter()
            .find(|c| c.summary == "fork-only")
            .expect("fork commit");
        assert_eq!(head.status.branch.oid.as_ref(), Some(&tip.oid));

        // Create from a new local branch: it is pushed first, then proposed.
        s.run_op(
            &entry,
            None,
            &ferrumgit_domain::RepoOp::CreateBranch {
                name: "new-thing".into(),
                start: Some("main".into()),
                checkout: true,
            },
            None,
        )
        .await
        .expect("branch");
        let created = s
            .create_pull_request(
                &entry,
                None,
                NewPullRequest {
                    title: "New thing".into(),
                    base: "main".into(),
                    ..NewPullRequest::default()
                },
                None,
            )
            .await
            .expect("create");
        assert_eq!(
            (created.number, created.head_branch.as_str()),
            (9, "new-thing")
        );
        let pushed = Command::new("git")
            .args(["rev-parse", "--verify", "-q", "refs/heads/new-thing"])
            .current_dir(&remote)
            .output()
            .expect("git");
        assert!(pushed.status.success(), "branch pushed before creating");

        // Validation happens before any push or request.
        let err = s
            .create_pull_request(
                &entry,
                None,
                NewPullRequest {
                    title: "x".into(),
                    base: "new-thing".into(),
                    ..NewPullRequest::default()
                },
                None,
            )
            .await
            .expect_err("same branch");
        assert!(err.to_string().contains("base branch"), "{err}");

        s.clear_forge_token("github.com").await.expect("clear");
        assert_eq!(tokens.get("github.com").expect("get"), None);
        assert!(!s.forge_info(&entry).await.expect("info").has_token);
    });
}

#[test]
fn github_cli_token_source() {
    let env = setup();
    let (svc, fake, _tokens, gh) = services(&env);
    let s = svc.clone();
    let work = env.work.display().to_string();
    block_on(&svc, async move {
        let entry = s.add_repo(&work).await.expect("add");
        let err = s.use_gh_cli("github.com").await.expect_err("gh logged out");
        assert!(
            err.to_string()
                .contains("gh auth login --hostname github.com"),
            "{err}"
        );

        *gh.lock().expect("lock") = Some("fake-gh-token".into());
        s.use_gh_cli("github.com").await.expect("gh");
        let info = s.forge_info(&entry).await.expect("info");
        assert_eq!(info.token_source, TokenSource::GhCli);
        assert!(info.has_token);
        s.pull_requests(&entry, PrFilter::Mine).await.expect("list");
        let sent = fake.sent.lock().expect("lock").clone();
        assert!(sent.iter().all(|r| {
            r.headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer fake-gh-token")
        }));
        // Mine: nothing authored by "me".
        let mine = s.pull_requests(&entry, PrFilter::Mine).await.expect("mine");
        assert!(mine.prs.is_empty());

        // Settings: a bad API URL is rejected; a custom one is used.
        let bad = s
            .set_repo_forge(
                entry.clone(),
                ferrumgit_domain::RepoForge {
                    api_url: "ftp://x".into(),
                    ..Default::default()
                },
            )
            .await;
        assert!(matches!(
            bad,
            Err(ferrumgit_application::AppError::Invalid(_))
        ));
        let entry = s
            .set_repo_forge(
                entry,
                ferrumgit_domain::RepoForge {
                    api_url: "https://ghe.example.com/api/v3/".into(),
                    ..Default::default()
                },
            )
            .await
            .expect("forge settings");
        assert_eq!(
            s.forge_repo(&entry).await.expect("repo").api_url,
            "https://ghe.example.com/api/v3"
        );
    });
}
