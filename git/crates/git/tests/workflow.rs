//! End-to-end tests against the real `git` binary in temp directories.
//! A local bare repository stands in for the remote; no network is used.

#![allow(clippy::unwrap_used)]

use ferrumgit_domain::{ChangeKind, Oid, PullMode, RefKind, RepoOp, RepoOperation, ResetMode};
use ferrumgit_git::{Git, OpContext, Repo, patch};
use std::path::{Path, PathBuf};

struct Sandbox {
    dir: tempfile::TempDir,
    git: Git,
}

impl Sandbox {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = dir.path().join("gitconfig");
        std::fs::write(
            &cfg,
            "[init]\n\tdefaultBranch = main\n[commit]\n\tgpgsign = false\n",
        )
        .expect("cfg");
        let git = Git::new()
            .with_env("GIT_CONFIG_GLOBAL", cfg.as_os_str())
            .with_env("GIT_CONFIG_NOSYSTEM", "1")
            .with_env("GIT_AUTHOR_NAME", "Test")
            .with_env("GIT_AUTHOR_EMAIL", "test@example.com")
            .with_env("GIT_COMMITTER_NAME", "Test")
            .with_env("GIT_COMMITTER_EMAIL", "test@example.com");
        Self { dir, git }
    }

    fn path(&self, p: &str) -> PathBuf {
        self.dir.path().join(p)
    }

    async fn init(&self, name: &str) -> Repo {
        self.git
            .init_repo(&self.path(name), "main")
            .await
            .expect("init")
    }
}

fn write(repo: &Repo, file: &str, content: &str) {
    let p = repo.root.join(file);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(p, content).expect("write");
}

async fn op(repo: &Repo, op: RepoOp) -> Option<String> {
    repo.execute(&op, OpContext::default())
        .await
        .unwrap_or_else(|e| panic!("{op:?}: {e}"))
}

async fn commit_file(repo: &Repo, file: &str, content: &str, msg: &str) {
    write(repo, file, content);
    op(repo, RepoOp::Stage(vec![file.to_owned()])).await;
    op(
        repo,
        RepoOp::Commit {
            message: msg.to_owned(),
            amend: false,
        },
    )
    .await;
}

async fn head(repo: &Repo) -> Oid {
    repo.status()
        .await
        .expect("status")
        .branch
        .oid
        .expect("head")
}

#[tokio::test]
async fn empty_repo_then_first_commit() {
    let sb = Sandbox::new();
    let repo = sb.init("a").await;
    let snap = repo.snapshot(100).await.expect("snapshot");
    assert!(snap.commits.is_empty());
    assert!(snap.status.branch.oid.is_none());
    assert_eq!(snap.status.branch.head.as_deref(), Some("main"));

    write(&repo, "README.md", "hi\n");
    let s = repo.status().await.expect("status");
    assert_eq!(s.files[0].index, ChangeKind::Untracked);
    op(&repo, RepoOp::StageAll).await;
    // Unstaging works before the first commit too.
    op(&repo, RepoOp::UnstageAll).await;
    assert!(repo.status().await.expect("s").staged().is_empty());
    op(&repo, RepoOp::Stage(vec!["README.md".into()])).await;

    let identity = Some(("Profile Person", "profile@example.com"));
    repo.execute(
        &RepoOp::Commit {
            message: "Initial commit\n\nBody".into(),
            amend: false,
        },
        OpContext {
            identity,
            ..Default::default()
        },
    )
    .await
    .expect("commit");
    let snap = repo.snapshot(100).await.expect("snapshot");
    assert_eq!(snap.commits.len(), 1);
    assert_eq!(
        snap.commits[0].author_email, "profile@example.com",
        "profile identity wins over git config"
    );
    let d = repo
        .commit_details(snap.commits[0].oid.as_str())
        .await
        .expect("details");
    assert_eq!(d.body, "Body");
    assert_eq!(d.files[0].change, ChangeKind::Added);
    assert!(repo.status().await.expect("s").is_clean());

    // Amend keeps one commit.
    write(&repo, "README.md", "hi there\n");
    op(&repo, RepoOp::StageAll).await;
    op(
        &repo,
        RepoOp::Commit {
            message: "Initial commit (amended)".into(),
            amend: true,
        },
    )
    .await;
    let snap = repo.snapshot(100).await.expect("snapshot");
    assert_eq!(snap.commits.len(), 1);
    assert_eq!(snap.commits[0].summary, "Initial commit (amended)");
    assert_eq!(
        repo.head_message().await.expect("msg"),
        "Initial commit (amended)"
    );
}

#[tokio::test]
async fn branches_merge_conflict_and_abort() {
    let sb = Sandbox::new();
    let repo = sb.init("b").await;
    commit_file(&repo, "f.txt", "base\n", "base").await;
    op(
        &repo,
        RepoOp::CreateBranch {
            name: "feature".into(),
            start: None,
            checkout: true,
        },
    )
    .await;
    commit_file(&repo, "f.txt", "feature\n", "feature change").await;
    op(&repo, RepoOp::Checkout("main".into())).await;
    commit_file(&repo, "f.txt", "main\n", "main change").await;

    let refs = repo.refs().await.expect("refs");
    let local: Vec<_> = refs
        .iter()
        .filter(|r| r.is(RefKind::LocalBranch))
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(local, ["feature", "main"]);
    assert!(
        refs.iter()
            .find(|r| r.name == "main")
            .expect("main")
            .is_head
    );

    let err = repo
        .execute(
            &RepoOp::Merge {
                branch: "feature".into(),
                no_ff: false,
            },
            OpContext::default(),
        )
        .await;
    assert!(err.is_err(), "conflicting merge reports an error");
    let s = repo.status().await.expect("status");
    assert_eq!(s.operation, Some(RepoOperation::Merge));
    assert_eq!(s.conflicted().len(), 1);

    op(&repo, RepoOp::Abort).await;
    let s = repo.status().await.expect("status");
    assert!(s.operation.is_none() && s.is_clean());

    // Resolve with "theirs" and continue.
    let _ = repo
        .execute(
            &RepoOp::Merge {
                branch: "feature".into(),
                no_ff: false,
            },
            OpContext::default(),
        )
        .await;
    op(&repo, RepoOp::TakeTheirs(vec!["f.txt".into()])).await;
    op(&repo, RepoOp::Continue).await;
    let s = repo.status().await.expect("status");
    assert!(s.operation.is_none() && s.is_clean());
    assert_eq!(
        std::fs::read_to_string(repo.root.join("f.txt")).expect("read"),
        "feature\n"
    );
    let (log, _) = repo.log(10, true).await.expect("log");
    assert!(log[0].is_merge());
    let refs = repo.refs().await.expect("refs");
    assert!(
        refs.iter()
            .find(|r| r.name == "feature")
            .expect("feature")
            .merged,
        "feature is merged into main"
    );
    assert!(
        !refs.iter().find(|r| r.name == "main").expect("main").merged,
        "HEAD is never marked"
    );

    op(
        &repo,
        RepoOp::RenameBranch {
            old: "feature".into(),
            new: "done".into(),
        },
    )
    .await;
    op(
        &repo,
        RepoOp::DeleteBranch {
            name: "done".into(),
            force: false,
        },
    )
    .await;
    assert!(
        !repo
            .refs()
            .await
            .expect("refs")
            .iter()
            .any(|r| r.name == "done")
    );
}

#[tokio::test]
async fn clone_push_fetch_pull_with_remote() {
    let sb = Sandbox::new();
    let bare = sb.path("remote.git");
    sb.git
        .run(ferrumgit_git::Cmd::new(["init", "-q", "--bare", "-b", "main"]).arg(bare.as_os_str()))
        .await
        .expect("bare");
    let url = bare.display().to_string();

    let a = sb
        .git
        .clone_repo(&url, &sb.path("a"), None, None)
        .await
        .expect("clone a");
    commit_file(&a, "x.txt", "1\n", "first").await;
    // No upstream yet → push sets it on origin.
    op(
        &a,
        RepoOp::Push {
            remote: None,
            force_with_lease: false,
        },
    )
    .await;
    let s = a.status().await.expect("status");
    assert_eq!(s.branch.upstream.as_deref(), Some("origin/main"));

    let b = sb
        .git
        .clone_repo(&url, &sb.path("b"), None, None)
        .await
        .expect("clone b");
    assert_eq!(b.snapshot(10).await.expect("snap").commits.len(), 1);
    assert!(
        sb.git
            .clone_repo(&url, &sb.path("b"), None, None)
            .await
            .is_err(),
        "non-empty destination"
    );

    commit_file(&a, "x.txt", "2\n", "second").await;
    op(
        &a,
        RepoOp::Push {
            remote: None,
            force_with_lease: false,
        },
    )
    .await;

    op(
        &b,
        RepoOp::Fetch {
            remote: None,
            prune: true,
        },
    )
    .await;
    assert_eq!(b.status().await.expect("s").branch.behind, 1);
    op(&b, RepoOp::Pull(PullMode::FastForwardOnly)).await;
    assert_eq!(head(&b).await, head(&a).await);

    // Tags and remote branches.
    let target = head(&a).await.0;
    op(
        &a,
        RepoOp::CreateTag {
            name: "v1".into(),
            target: target.clone(),
            message: Some("Release 1".into()),
        },
    )
    .await;
    op(
        &a,
        RepoOp::PushTag {
            remote: "origin".into(),
            tag: "v1".into(),
        },
    )
    .await;
    op(
        &a,
        RepoOp::CreateBranch {
            name: "topic".into(),
            start: None,
            checkout: true,
        },
    )
    .await;
    op(
        &a,
        RepoOp::Push {
            remote: Some("origin".into()),
            force_with_lease: false,
        },
    )
    .await;
    op(
        &b,
        RepoOp::Fetch {
            remote: Some("origin".into()),
            prune: false,
        },
    )
    .await;
    let refs = b.refs().await.expect("refs");
    let tag = refs.iter().find(|r| r.name == "v1").expect("tag fetched");
    assert_eq!(tag.target.0, target, "annotated tag peeled to commit");
    assert!(
        refs.iter()
            .any(|r| r.name == "origin/topic" && r.is(RefKind::RemoteBranch))
    );
    op(
        &b,
        RepoOp::CheckoutRemote {
            remote_branch: "origin/topic".into(),
            local: "topic".into(),
        },
    )
    .await;
    assert_eq!(
        b.status().await.expect("s").branch.upstream.as_deref(),
        Some("origin/topic")
    );

    op(&a, RepoOp::Checkout("main".into())).await;
    op(
        &a,
        RepoOp::DeleteRemoteBranch {
            remote: "origin".into(),
            branch: "topic".into(),
        },
    )
    .await;
    op(
        &a,
        RepoOp::DeleteRemoteTag {
            remote: "origin".into(),
            tag: "v1".into(),
        },
    )
    .await;
    op(
        &b,
        RepoOp::Fetch {
            remote: None,
            prune: true,
        },
    )
    .await;
    let topic = b
        .refs()
        .await
        .expect("refs")
        .into_iter()
        .find(|r| r.name == "topic")
        .expect("topic");
    assert!(topic.upstream_gone);

    let remotes = a.remotes().await.expect("remotes");
    assert_eq!(remotes[0].name, "origin");
    op(
        &a,
        RepoOp::AddRemote {
            name: "mirror".into(),
            url: url.clone(),
        },
    )
    .await;
    op(&a, RepoOp::RemoveRemote("mirror".into())).await;
    assert_eq!(a.remotes().await.expect("r").len(), 1);
}

#[tokio::test]
async fn stash_save_apply_pop_drop() {
    let sb = Sandbox::new();
    let repo = sb.init("s").await;
    commit_file(&repo, "a.txt", "a\n", "a").await;
    write(&repo, "a.txt", "changed\n");
    write(&repo, "new.txt", "untracked\n");
    op(
        &repo,
        RepoOp::StashSave {
            message: "my work".into(),
            include_untracked: true,
        },
    )
    .await;
    assert!(repo.status().await.expect("s").is_clean());
    let stashes = repo.stashes().await.expect("stashes");
    assert_eq!(stashes.len(), 1);
    assert!(stashes[0].message.contains("my work"));
    let d = repo
        .stash_details(&stashes[0])
        .await
        .expect("stash details");
    let paths: Vec<_> = d.files.iter().map(|f| f.path.as_str()).collect();
    assert!(
        paths.contains(&"a.txt") && paths.contains(&"new.txt"),
        "{paths:?}"
    );
    // Stashes are not shown as graph commits.
    assert_eq!(repo.log(10, true).await.expect("log").0.len(), 1);

    op(&repo, RepoOp::StashApply(0)).await;
    assert_eq!(repo.stashes().await.expect("s").len(), 1);
    op(
        &repo,
        RepoOp::Discard(vec!["a.txt".into(), "new.txt".into()]),
    )
    .await;
    assert!(
        repo.status().await.expect("s").is_clean(),
        "discard restores tracked and deletes untracked"
    );
    op(&repo, RepoOp::StashPop(0)).await;
    assert!(repo.stashes().await.expect("s").is_empty());
    op(
        &repo,
        RepoOp::StashSave {
            message: String::new(),
            include_untracked: true,
        },
    )
    .await;
    op(&repo, RepoOp::StashDrop(0)).await;
    assert!(repo.stashes().await.expect("s").is_empty());
}

#[tokio::test]
async fn hunk_and_line_staging() {
    let sb = Sandbox::new();
    let repo = sb.init("h").await;
    let base: String = (1..=30).map(|i| format!("line {i}\n")).collect();
    commit_file(&repo, "f.txt", &base, "base").await;
    let changed = base
        .replace("line 2\n", "LINE 2\n")
        .replace("line 28\n", "LINE 28\n");
    write(&repo, "f.txt", &changed);

    let diff = repo
        .diff_unstaged("f.txt")
        .await
        .expect("diff")
        .expect("some");
    assert_eq!(diff.hunks.len(), 2);
    let p = patch::hunk_patch(&diff, 1, None, false).expect("patch");
    op(
        &repo,
        RepoOp::ApplyPatch {
            patch: p,
            cached: true,
            reverse: false,
        },
    )
    .await;
    let staged = repo
        .diff_staged("f.txt", None)
        .await
        .expect("staged")
        .expect("some");
    assert_eq!(staged.hunks.len(), 1);
    assert!(staged.hunks[0].lines.iter().any(|l| l.content == "LINE 28"));
    let unstaged = repo
        .diff_unstaged("f.txt")
        .await
        .expect("diff")
        .expect("some");
    assert_eq!(unstaged.hunks.len(), 1);

    // Unstage it again (reverse on the index).
    let p = patch::hunk_patch(&staged, 0, None, true).expect("patch");
    op(
        &repo,
        RepoOp::ApplyPatch {
            patch: p,
            cached: true,
            reverse: true,
        },
    )
    .await;
    assert!(
        repo.diff_staged("f.txt", None)
            .await
            .expect("staged")
            .is_none()
    );

    // Stage one line: only the "+LINE 2" of the first hunk.
    let diff = repo
        .diff_unstaged("f.txt")
        .await
        .expect("diff")
        .expect("some");
    let plus = diff.hunks[0]
        .lines
        .iter()
        .position(|l| l.content == "LINE 2")
        .expect("line");
    let p = patch::hunk_patch(&diff, 0, Some(&[plus]), false).expect("patch");
    op(
        &repo,
        RepoOp::ApplyPatch {
            patch: p,
            cached: true,
            reverse: false,
        },
    )
    .await;
    let staged = repo
        .diff_staged("f.txt", None)
        .await
        .expect("staged")
        .expect("some");
    assert_eq!(
        (staged.additions, staged.deletions),
        (1, 0),
        "line 2 kept, LINE 2 added"
    );

    // Discard the second hunk from the working tree.
    let diff = repo
        .diff_unstaged("f.txt")
        .await
        .expect("diff")
        .expect("some");
    let last = diff.hunks.len() - 1;
    let p = patch::hunk_patch(&diff, last, None, true).expect("patch");
    op(
        &repo,
        RepoOp::ApplyPatch {
            patch: p,
            cached: false,
            reverse: true,
        },
    )
    .await;
    let text = std::fs::read_to_string(repo.root.join("f.txt")).expect("read");
    assert!(text.contains("line 28\n") && !text.contains("LINE 28"));

    write(&repo, "brand new.txt", "a\nb\n");
    let u = repo
        .diff_untracked("brand new.txt")
        .await
        .expect("diff")
        .expect("some");
    assert_eq!(
        (u.path.as_str(), u.additions, u.change),
        ("brand new.txt", 2, ChangeKind::Added)
    );
}

#[tokio::test]
async fn reset_revert_cherry_pick_detached() {
    let sb = Sandbox::new();
    let repo = sb.init("r").await;
    commit_file(&repo, "a.txt", "1\n", "one").await;
    let one = head(&repo).await;
    commit_file(&repo, "a.txt", "2\n", "two").await;
    let two = head(&repo).await;

    op(&repo, RepoOp::Revert(two.clone())).await;
    assert_eq!(
        std::fs::read_to_string(repo.root.join("a.txt")).expect("r"),
        "1\n"
    );
    op(
        &repo,
        RepoOp::Reset {
            target: two.0.clone(),
            mode: ResetMode::Hard,
        },
    )
    .await;
    assert_eq!(head(&repo).await, two);

    op(
        &repo,
        RepoOp::CreateBranch {
            name: "side".into(),
            start: Some(one.0.clone()),
            checkout: true,
        },
    )
    .await;
    op(&repo, RepoOp::CherryPick(two.clone())).await;
    assert_eq!(
        std::fs::read_to_string(repo.root.join("a.txt")).expect("r"),
        "2\n"
    );

    op(
        &repo,
        RepoOp::Reset {
            target: one.0.clone(),
            mode: ResetMode::Soft,
        },
    )
    .await;
    assert_eq!(repo.status().await.expect("s").staged().len(), 1);

    op(
        &repo,
        RepoOp::StashSave {
            message: "x".into(),
            include_untracked: false,
        },
    )
    .await;
    op(&repo, RepoOp::CheckoutDetached(one.clone())).await;
    let s = repo.status().await.expect("s");
    assert!(s.branch.head.is_none());
    let err = repo
        .execute(
            &RepoOp::Push {
                remote: None,
                force_with_lease: false,
            },
            OpContext::default(),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("detached"));
}

#[tokio::test]
async fn rejects_option_injection_and_missing_repo() {
    let sb = Sandbox::new();
    let repo = sb.init("i").await;
    for bad in [
        RepoOp::Checkout("--orphan".into()),
        RepoOp::CreateBranch {
            name: "-x".into(),
            start: None,
            checkout: false,
        },
        RepoOp::Reset {
            target: "--hard".into(),
            mode: ResetMode::Soft,
        },
        RepoOp::AddRemote {
            name: "o".into(),
            url: "--upload-pack=evil".into(),
        },
    ] {
        assert!(
            repo.execute(&bad, OpContext::default()).await.is_err(),
            "{bad:?}"
        );
    }
    let err = Repo::open(&sb.git, Path::new("/")).await.unwrap_err();
    assert!(
        matches!(err, ferrumgit_git::GitError::NotARepo(_)),
        "{err:?}"
    );
}
