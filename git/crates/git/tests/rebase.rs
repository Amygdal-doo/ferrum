//! Interactive rebase through git's own `rebase -i` with a generated todo
//! list, against the real `git` binary in temp directories.

#![allow(clippy::unwrap_used)]

mod common;

use common::{Sandbox, commit_file, git, head, op, read, subjects, try_op, write};
use ferrumgit_domain::{RebaseAction, RebasePlan, RepoOp, RepoOperation};
use ferrumgit_git::{OpContext, Repo};

/// `base` plus four commits on top: a, b, c, d (one file each).
async fn four_commits(sb: &Sandbox) -> (Repo, ferrumgit_domain::Oid) {
    let repo = sb.init("rebase").await;
    let base = commit_file(&repo, "base.txt", "base\n", "Base").await;
    for name in ["a", "b", "c", "d"] {
        commit_file(&repo, &format!("{name}.txt"), "x\n", &format!("Add {name}")).await;
    }
    (repo, base)
}

#[tokio::test]
async fn reorders_squashes_fixes_up_drops_and_rewords() {
    let sb = Sandbox::new();
    let (repo, base) = four_commits(&sb).await;
    let steps = repo.rebase_steps(base.as_str()).await.expect("steps");
    assert_eq!(
        steps.iter().map(|s| s.summary()).collect::<Vec<_>>(),
        ["Add a", "Add b", "Add c", "Add d"]
    );
    let mut plan = RebasePlan::new(base.clone(), steps);
    // d first, reworded; then a with b squashed in and c dropped.
    plan.move_step(3, 0);
    plan.steps[0].action = RebaseAction::Reword;
    plan.steps[0].reword = Some("Add d, reworded\n\nWith a body".into());
    plan.steps[2].action = RebaseAction::Squash;
    plan.steps[3].action = RebaseAction::Drop;
    let preview = plan.preview();
    assert_eq!(preview.len(), 2);

    // A profile identity becomes the committer; authors are kept.
    repo.execute(
        &RepoOp::InteractiveRebase(plan),
        OpContext {
            identity: Some(("Profile", "profile@example.com")),
            ..Default::default()
        },
    )
    .await
    .expect("rebase");

    assert_eq!(
        subjects(&sb, &repo).await,
        ["Add a", "Add d, reworded", "Base"]
    );
    let squashed = git(&sb, &repo, &["log", "-1", "--format=%B"]).await;
    assert!(
        squashed.contains("Add a") && squashed.contains("Add b"),
        "{squashed}"
    );
    let reworded = git(&sb, &repo, &["log", "-1", "--format=%B", "HEAD~1"]).await;
    assert_eq!(reworded.trim_end(), "Add d, reworded\n\nWith a body");
    let people = git(&sb, &repo, &["log", "-1", "--format=%an|%cn"]).await;
    assert_eq!(people.trim(), "Test|Profile");
    assert!(repo.root.join("b.txt").exists());
    assert!(!repo.root.join("c.txt").exists(), "c was dropped");
    assert!(repo.operation().is_none());
    assert!(
        !repo.git_dir.join("ferrumgit-rebase").exists(),
        "scratch files are removed"
    );
}

#[tokio::test]
async fn stops_at_edit_and_continues() {
    let sb = Sandbox::new();
    let (repo, base) = four_commits(&sb).await;
    let mut plan = RebasePlan::new(base, repo.rebase_steps("HEAD~4").await.expect("steps"));
    plan.steps[1].action = RebaseAction::Edit;
    let stop = op(&repo, RepoOp::InteractiveRebase(plan)).await;
    assert!(stop.unwrap_or_default().starts_with("Stopped at"));
    let status = repo.status().await.expect("status");
    assert_eq!(status.operation, Some(RepoOperation::Rebase));
    let progress = status.rebase.expect("progress");
    assert!(progress.editing);
    assert!(progress.current.unwrap_or_default().starts_with("edit "));
    assert!(progress.step <= progress.total);

    // Amend the stopped commit, then continue.
    write(&repo, "b.txt", "edited\n");
    op(&repo, RepoOp::StageAll).await;
    op(
        &repo,
        RepoOp::Commit {
            message: "Add b (edited)".into(),
            amend: true,
        },
    )
    .await;
    op(&repo, RepoOp::Continue).await;
    assert!(repo.operation().is_none());
    assert_eq!(
        subjects(&sb, &repo).await,
        ["Add d", "Add c", "Add b (edited)", "Add a", "Base"]
    );
    assert_eq!(read(&repo, "b.txt"), "edited\n");
}

/// Two commits rewriting the same line, swapped: the first replay conflicts.
async fn conflicting(sb: &Sandbox) -> (Repo, RebasePlan, ferrumgit_domain::Oid) {
    let repo = sb.init("conflict").await;
    let base = commit_file(&repo, "f.txt", "one\n", "Base").await;
    commit_file(&repo, "f.txt", "two\n", "Two").await;
    let tip = commit_file(&repo, "f.txt", "three\n", "Three").await;
    let mut plan = RebasePlan::new(
        base.clone(),
        repo.rebase_steps(base.as_str()).await.expect("steps"),
    );
    plan.move_step(1, 0);
    (repo, plan, tip)
}

#[tokio::test]
async fn conflicts_stop_and_can_be_skipped() {
    let sb = Sandbox::new();
    let (repo, plan, _) = conflicting(&sb).await;
    let err = try_op(&repo, RepoOp::InteractiveRebase(plan))
        .await
        .expect_err("conflict");
    assert!(err.to_string().contains("CONFLICT"), "{err}");
    let status = repo.status().await.expect("status");
    assert_eq!(status.operation, Some(RepoOperation::Rebase));
    assert_eq!(status.conflicted().len(), 1);
    assert!(!status.rebase.expect("progress").editing);

    // Skipping "Three" replays "Two" cleanly.
    op(&repo, RepoOp::Skip).await;
    assert!(repo.operation().is_none());
    assert_eq!(subjects(&sb, &repo).await, ["Two", "Base"]);
}

#[tokio::test]
async fn conflicts_can_be_aborted() {
    let sb = Sandbox::new();
    let (repo, plan, tip) = conflicting(&sb).await;
    assert!(
        try_op(&repo, RepoOp::InteractiveRebase(plan))
            .await
            .is_err()
    );
    op(&repo, RepoOp::Abort).await;
    assert!(repo.operation().is_none());
    assert_eq!(head(&repo).await, tip);
    assert!(!repo.git_dir.join("ferrumgit-rebase").exists());
}

#[tokio::test]
async fn refuses_bad_bases_merges_and_stale_plans() {
    let sb = Sandbox::new();
    let (repo, base) = four_commits(&sb).await;
    let tip = head(&repo).await;
    // HEAD itself and descendants are not bases.
    assert!(repo.rebase_steps(tip.as_str()).await.is_err());
    // A commit on another branch is not an ancestor.
    op(
        &repo,
        RepoOp::CreateBranch {
            name: "side".into(),
            start: Some(base.0.clone()),
            checkout: true,
        },
    )
    .await;
    let side = commit_file(&repo, "side.txt", "s\n", "Side").await;
    op(&repo, RepoOp::Checkout("main".into())).await;
    let err = repo
        .rebase_steps(side.as_str())
        .await
        .expect_err("not ancestor");
    assert!(err.to_string().contains("not part"), "{err}");
    assert!(repo.rebase_steps("--root").await.is_err());

    // A plan made before the branch moved is refused.
    let plan = RebasePlan::new(
        base.clone(),
        repo.rebase_steps(base.as_str()).await.unwrap(),
    );
    commit_file(&repo, "e.txt", "x\n", "Add e").await;
    let err = try_op(&repo, RepoOp::InteractiveRebase(plan))
        .await
        .expect_err("stale");
    assert!(err.to_string().contains("changed"), "{err}");

    // Merge commits in the range are refused.
    op(
        &repo,
        RepoOp::Merge {
            branch: "side".into(),
            no_ff: true,
        },
    )
    .await;
    let err = repo.rebase_steps(base.as_str()).await.expect_err("merge");
    assert!(err.to_string().contains("merge commit"), "{err}");
}
