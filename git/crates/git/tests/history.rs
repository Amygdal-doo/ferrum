//! File history, blame, message search and undo against the real `git`
//! binary in temp directories.

#![allow(clippy::unwrap_used)]

use ferrumgit_domain::{ChangeKind, Oid, RepoOp, Undo};
use ferrumgit_git::{Cmd, Git, OpContext, Repo};

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

    async fn init(&self, name: &str) -> Repo {
        self.git
            .init_repo(&self.dir.path().join(name), "main")
            .await
            .expect("init")
    }
}

fn write(repo: &Repo, file: &str, content: &str) {
    std::fs::write(repo.root.join(file), content).expect("write");
}

async fn op(repo: &Repo, op: RepoOp) {
    repo.execute(&op, OpContext::default())
        .await
        .unwrap_or_else(|e| panic!("{op:?}: {e}"));
}

async fn commit_file(repo: &Repo, file: &str, content: &str, msg: &str) {
    write(repo, file, content);
    op(repo, RepoOp::StageAll).await;
    op(
        repo,
        RepoOp::Commit {
            message: msg.to_owned(),
            amend: false,
        },
    )
    .await;
}

async fn git(sb: &Sandbox, repo: &Repo, args: &[&str]) {
    sb.git
        .run(Cmd::new(args).cwd(&repo.root))
        .await
        .expect("git");
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
async fn file_history_follows_renames_and_merges() {
    let sb = Sandbox::new();
    let repo = sb.init("h").await;
    commit_file(&repo, "old.txt", "one\ntwo\nthree\n", "Create").await;
    commit_file(&repo, "other.txt", "x\n", "Unrelated").await;
    commit_file(&repo, "old.txt", "one\nTWO\nthree\n", "Edit").await;
    git(&sb, &repo, &["mv", "old.txt", "new.txt"]).await;
    op(
        &repo,
        RepoOp::Commit {
            message: "Rename".into(),
            amend: false,
        },
    )
    .await;
    // A merge touching the file on both sides.
    op(
        &repo,
        RepoOp::CreateBranch {
            name: "side".into(),
            start: None,
            checkout: true,
        },
    )
    .await;
    commit_file(&repo, "new.txt", "one\nTWO\nthree\nside\n", "Side edit").await;
    op(&repo, RepoOp::Checkout("main".into())).await;
    commit_file(&repo, "new.txt", "main\none\nTWO\nthree\n", "Main edit").await;
    op(
        &repo,
        RepoOp::Merge {
            branch: "side".into(),
            no_ff: true,
        },
    )
    .await;

    let log = repo.file_history("new.txt", None, 100).await.expect("log");
    let summaries: Vec<&str> = log.iter().map(|f| f.commit.summary.as_str()).collect();
    assert!(!summaries.contains(&"Unrelated"), "{summaries:?}");
    for s in ["Create", "Edit", "Rename", "Side edit", "Main edit"] {
        assert!(summaries.contains(&s), "{s} missing from {summaries:?}");
    }
    let rename = log.iter().find(|f| f.commit.summary == "Rename").unwrap();
    assert_eq!(rename.change, ChangeKind::Renamed);
    assert_eq!(rename.old_path.as_deref(), Some("old.txt"));
    let create = log.iter().find(|f| f.commit.summary == "Create").unwrap();
    assert_eq!(create.path, "old.txt", "path before the rename");
    assert_eq!(create.change, ChangeKind::Added);
    assert_eq!(log.last().unwrap().commit.summary, "Create");

    // From an older revision, and the limit.
    let edit = log.iter().find(|f| f.commit.summary == "Edit").unwrap();
    let older = repo
        .file_history("old.txt", Some(edit.commit.oid.as_str()), 100)
        .await
        .expect("older");
    assert_eq!(older.len(), 2);
    assert_eq!(
        repo.file_history("new.txt", None, 2).await.unwrap().len(),
        2
    );
    assert!(repo.file_history("-x", None, 2).await.is_err());
}

#[tokio::test]
async fn blame_at_revision_and_working_tree() {
    let sb = Sandbox::new();
    let repo = sb.init("b").await;
    commit_file(&repo, "f.rs", "a\nb\nc\n", "First").await;
    let first = head(&repo).await;
    commit_file(&repo, "f.rs", "a\nB\nc\n", "Second").await;
    let second = head(&repo).await;

    let b = repo.blame("f.rs", None).await.expect("blame");
    let ids: Vec<&Oid> = b.lines.iter().map(|l| &l.oid).collect();
    assert_eq!(ids, vec![&first, &second, &first]);
    assert_eq!(b.lines[1].content, "B");
    assert_eq!(b.commit_of(&b.lines[1]).unwrap().summary, "Second");
    assert_eq!(
        b.commit_of(&b.lines[0]).unwrap().author_email,
        "test@example.com"
    );
    assert!(b.starts_group(1) && b.starts_group(2));

    let old = repo.blame("f.rs", Some(first.as_str())).await.expect("old");
    assert!(old.lines.iter().all(|l| l.oid == first));
    assert_eq!(old.lines[1].content, "b");

    // Uncommitted edits show up with the zero id.
    write(&repo, "f.rs", "a\nB\nc\nd\n");
    let wt = repo.blame("f.rs", None).await.expect("worktree");
    assert_eq!(wt.lines.len(), 4);
    assert!(wt.lines[3].oid.is_zero());

    assert!(repo.blame("missing.rs", None).await.is_err());
}

#[tokio::test]
async fn grep_messages_matches_bodies() {
    let sb = Sandbox::new();
    let repo = sb.init("g").await;
    assert!(
        repo.grep_messages("x", 10).await.unwrap().is_empty(),
        "no commits"
    );
    commit_file(&repo, "a", "1", "Plain\n\nMentions the Flux capacitor").await;
    commit_file(&repo, "a", "2", "Other").await;
    let hits = repo
        .grep_messages("flux CAPACITOR", 10)
        .await
        .expect("grep");
    assert_eq!(hits.len(), 1);
    assert!(
        repo.grep_messages("[", 10).await.unwrap().is_empty(),
        "fixed strings"
    );
    assert!(repo.grep_messages("  ", 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn undo_commit_amend_and_checkout() {
    let sb = Sandbox::new();
    let repo = sb.init("u").await;
    commit_file(&repo, "a", "1\n", "Initial").await;
    assert_eq!(repo.undo_plan().await.unwrap(), None, "initial commit");

    let base = head(&repo).await;
    commit_file(&repo, "a", "2\n", "Second").await;
    let plan = repo.undo_plan().await.unwrap().expect("commit undo");
    assert!(matches!(&plan, Undo::Commit { to, amend: false, .. } if *to == base));
    op(&repo, RepoOp::Undo(plan.clone())).await;
    assert_eq!(head(&repo).await, base);
    let s = repo.status().await.unwrap();
    assert_eq!(s.staged().len(), 1, "changes stay staged");
    assert_eq!(
        repo.undo_plan().await.unwrap(),
        None,
        "a reset is not undoable"
    );
    // A stale plan is refused.
    assert!(
        repo.execute(&RepoOp::Undo(plan), OpContext::default())
            .await
            .is_err()
    );

    // Amend → back to the commit before amending.
    op(
        &repo,
        RepoOp::Commit {
            message: "Second".into(),
            amend: false,
        },
    )
    .await;
    let before_amend = head(&repo).await;
    write(&repo, "b", "new\n");
    op(&repo, RepoOp::StageAll).await;
    op(
        &repo,
        RepoOp::Commit {
            message: "Second, amended".into(),
            amend: true,
        },
    )
    .await;
    let plan = repo.undo_plan().await.unwrap().expect("amend undo");
    assert!(matches!(&plan, Undo::Commit { amend: true, to, .. } if *to == before_amend));
    op(&repo, RepoOp::Undo(plan)).await;
    assert_eq!(head(&repo).await, before_amend);
    assert!(
        repo.status()
            .await
            .unwrap()
            .staged()
            .iter()
            .any(|f| f.path == "b")
    );
    op(&repo, RepoOp::UnstageAll).await;
    git(&sb, &repo, &["clean", "-fq"]).await;

    // Checkout → back to the previous branch; works again after a detach.
    op(
        &repo,
        RepoOp::CreateBranch {
            name: "feature".into(),
            start: None,
            checkout: true,
        },
    )
    .await;
    let plan = repo.undo_plan().await.unwrap().expect("checkout undo");
    assert!(matches!(&plan, Undo::Checkout { from, to, .. } if from == "main" && to == "feature"));
    op(&repo, RepoOp::Undo(plan)).await;
    let s = repo.status().await.unwrap();
    assert_eq!(s.branch.head.as_deref(), Some("main"));
    op(&repo, RepoOp::CheckoutDetached(base.clone())).await;
    let plan = repo.undo_plan().await.unwrap().expect("detach undo");
    op(&repo, RepoOp::Undo(plan)).await;
    assert_eq!(
        repo.status().await.unwrap().branch.head.as_deref(),
        Some("main")
    );

    // Discards are never undoable: the reflog does not record them, so the
    // plan stays the earlier checkout and does not claim to restore the file.
    commit_file(&repo, "a", "3\n", "Third").await;
    write(&repo, "a", "dirty\n");
    op(&repo, RepoOp::Discard(vec!["a".into()])).await;
    let plan = repo.undo_plan().await.unwrap().expect("commit undo");
    assert!(matches!(plan, Undo::Commit { ref summary, .. } if summary == "Third"));
}

#[tokio::test]
async fn pushed_commits_are_not_undoable() {
    let sb = Sandbox::new();
    let bare = sb.dir.path().join("remote.git");
    sb.git
        .run(Cmd::new(["init", "-q", "--bare", "-b", "main"]).arg(bare.as_os_str()))
        .await
        .expect("bare");
    let repo = sb
        .git
        .clone_repo(
            &bare.display().to_string(),
            &sb.dir.path().join("c"),
            None,
            None,
        )
        .await
        .expect("clone");
    commit_file(&repo, "a", "1\n", "One").await;
    commit_file(&repo, "a", "2\n", "Two").await;
    assert!(repo.undo_plan().await.unwrap().is_some());
    op(
        &repo,
        RepoOp::Push {
            remote: None,
            force_with_lease: false,
        },
    )
    .await;
    assert_eq!(repo.undo_plan().await.unwrap(), None);
    // The snapshot carries the plan for the toolbar.
    commit_file(&repo, "a", "3\n", "Three").await;
    assert!(repo.snapshot(10).await.unwrap().undo.is_some());
}
