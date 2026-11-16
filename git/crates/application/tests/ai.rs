//! AI drafts with a scripted model: the staged diff or the branch's commits
//! go into the prompt, the answer comes back as a draft, and nothing is
//! committed. The model store is the shared agent core's, with a mock
//! provider and an in-memory key store.

use ferrumagent_core::models::ModelStore;
use ferrumagent_core::provider::mock::MockProvider;
use ferrumagent_core::secrets::MemoryStore;
use ferrumagent_core::settings::AgentSettings;
use ferrumgit_application::AppServices;
use ferrumgit_credentials::SessionStore;
use ferrumgit_git::Git;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git");
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn services(tmp: &Path, mock: &MockProvider) -> AppServices {
    let store = ModelStore::new(
        tmp.join("agent-home"),
        Arc::new(MemoryStore::default()),
        Arc::new(Mutex::new(AgentSettings::default())),
        Some(Arc::new(mock.clone())),
        Vec::new(),
    );
    AppServices::start_with(
        &tmp.join("data"),
        Git::new(),
        Arc::new(SessionStore::default()),
    )
    .expect("services")
    .with_model_store(store)
}

fn block_on<F: std::future::Future + Send + 'static>(svc: &AppServices, f: F) -> F::Output
where
    F::Output: Send + 'static,
{
    let handle = svc.spawn(f);
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("rt")
        .block_on(handle)
        .expect("task")
}

#[test]
fn drafts_a_commit_message_from_the_staged_diff_without_committing() {
    let tmp = tempfile::tempdir().expect("tmp");
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("hello.rs"), "fn main() {}\n").expect("write");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "Start"]);
    std::fs::write(repo.join("hello.rs"), "fn main() { println!(\"hi\"); }\n").expect("write");

    let mock = MockProvider::new();
    mock.push_text("```\nPrint a greeting\n\nSays hi on start.\n```");
    let svc = services(tmp.path(), &mock);
    let path = repo.display().to_string();

    // Nothing staged: no request is sent.
    let s = svc.clone();
    let p = path.clone();
    let err = block_on(&svc, async move { s.draft_commit_message(&p, None).await })
        .expect_err("nothing staged");
    assert!(err.to_string().contains("Stage"), "{err}");
    assert!(mock.requests().is_empty());

    git(&repo, &["add", "."]);
    let head = git(&repo, &["rev-parse", "HEAD"]);
    let s = svc.clone();
    let p = path.clone();
    let draft =
        block_on(&svc, async move { s.draft_commit_message(&p, None).await }).expect("draft");
    assert_eq!(draft.title, "Print a greeting");
    assert_eq!(draft.body, "Says hi on start.");

    let sent = mock.requests();
    assert_eq!(sent.len(), 1);
    // The agent's default model (the first configured one); never FerrumAI.
    assert_eq!(sent[0].model, "claude-sonnet-5-5");
    assert!(sent[0].tools.is_empty());
    let prompt = format!("{:?}", sent[0].messages);
    assert!(prompt.contains("println!"), "{prompt}");
    assert!(prompt.contains("hello.rs"), "{prompt}");

    // Drafting never commits: HEAD and the index are as they were.
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        git(&repo, &["diff", "--cached", "--name-only"]).trim(),
        "hello.rs"
    );
}

#[test]
fn drafts_a_pull_request_from_the_branch_commits_with_a_chosen_model() {
    let tmp = tempfile::tempdir().expect("tmp");
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "a\n").expect("write");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "Start"]);
    git(&repo, &["switch", "-q", "-c", "search"]);
    for (file, msg) in [("b.txt", "Add search index"), ("c.txt", "Rank results")] {
        std::fs::write(repo.join(file), "x\n").expect("write");
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", msg, "-m", "Details."]);
    }

    let mock = MockProvider::new();
    mock.push_text("Title: Faster search\n\nAdds an index and ranking.\n\n- index\n- ranking");
    let svc = services(tmp.path(), &mock);
    let path = repo.display().to_string();

    let s = svc.clone();
    let models = block_on(&svc, async move { s.ai_models().await }).expect("models");
    assert!(models.iter().all(|m| !m.key.starts_with("ferrum/")));
    let chosen = models
        .iter()
        .find(|m| m.key == "openai/gpt-5")
        .expect("openai")
        .key
        .clone();

    // No `origin/main`: the local base is used.
    let s = svc.clone();
    let draft = block_on(&svc, async move {
        s.draft_pull_request(&path, "origin", "main", Some(chosen))
            .await
    })
    .expect("draft");
    assert_eq!(draft.title, "Faster search");
    assert!(draft.body.starts_with("Adds an index"));
    let sent = mock.requests();
    assert_eq!(sent[0].model, "gpt-5");
    let prompt = format!("{:?}", sent[0].messages);
    assert!(prompt.contains("Add search index") && prompt.contains("Rank results"));
    assert!(prompt.contains("`search` into `main`"), "{prompt}");
    assert!(
        !prompt.contains("Start"),
        "commits on the base are left out"
    );
}
