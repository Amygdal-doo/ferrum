//! Three-way conflict resolution (ours / theirs / result) during a merge,
//! a rebase and a cherry-pick, against the real `git` binary.

#![allow(clippy::unwrap_used)]

mod common;

use common::{Sandbox, commit_file, op, read, subjects, try_op, write};
use ferrumgit_domain::{ConflictChoice, ConflictSegment, RepoOp, RepoOperation};
use ferrumgit_git::Repo;

const BASE: &str = "top\nshared\nmiddle\nend\n";

/// `main` and `topic` both rewrite line 2 and append different tails.
async fn diverged(sb: &Sandbox) -> Repo {
    let repo = sb.init("conflicts").await;
    commit_file(&repo, "f.txt", BASE, "Base").await;
    op(
        &repo,
        RepoOp::CreateBranch {
            name: "topic".into(),
            start: None,
            checkout: true,
        },
    )
    .await;
    commit_file(
        &repo,
        "f.txt",
        "top\ntheirs\nmiddle\nend\ntheirs tail\n",
        "Topic",
    )
    .await;
    op(&repo, RepoOp::Checkout("main".into())).await;
    commit_file(
        &repo,
        "f.txt",
        "top\nours\nmiddle\nend\nours tail\n",
        "Main",
    )
    .await;
    repo
}

/// Resolves `f.txt` with `choices` through the three-way view and stages it.
async fn resolve(repo: &Repo, choices: &[ConflictChoice]) -> String {
    let file = repo.conflict_file("f.txt").await.expect("conflict file");
    assert_eq!(file.conflicts(), choices.len(), "{file:?}");
    let content = file.result(choices).expect("resolved");
    op(
        repo,
        RepoOp::Resolve {
            path: "f.txt".into(),
            content: content.clone(),
        },
    )
    .await;
    assert!(repo.status().await.unwrap().conflicted().is_empty());
    content
}

#[tokio::test]
async fn merge_conflict_takes_hunks_from_either_side() {
    let sb = Sandbox::new();
    let repo = diverged(&sb).await;
    let err = try_op(
        &repo,
        RepoOp::Merge {
            branch: "topic".into(),
            no_ff: false,
        },
    )
    .await
    .expect_err("conflict");
    assert!(err.to_string().contains("CONFLICT"));
    assert_eq!(repo.operation(), Some(RepoOperation::Merge));

    // A half-edited working copy does not change what the view reads.
    write(&repo, "f.txt", "scribbles\n");
    let file = repo.conflict_file("f.txt").await.expect("conflict file");
    assert_eq!(file.segments[0], ConflictSegment::Common("top\n".into()));
    assert_eq!(
        file.segments[1],
        ConflictSegment::Conflict {
            ours: "ours\n".into(),
            base: Some("shared\n".into()),
            theirs: "theirs\n".into(),
        }
    );

    let content = resolve(&repo, &[ConflictChoice::Theirs, ConflictChoice::Both]).await;
    assert_eq!(
        content,
        "top\ntheirs\nmiddle\nend\nours tail\ntheirs tail\n"
    );
    assert_eq!(read(&repo, "f.txt"), content);
    op(&repo, RepoOp::Continue).await;
    assert!(repo.operation().is_none());
}

#[tokio::test]
async fn rebase_and_cherry_pick_conflicts_use_the_same_view() {
    let sb = Sandbox::new();
    let repo = diverged(&sb).await;
    op(&repo, RepoOp::Checkout("topic".into())).await;
    assert!(
        try_op(
            &repo,
            RepoOp::Rebase {
                onto: "main".into()
            }
        )
        .await
        .is_err()
    );
    assert_eq!(repo.operation(), Some(RepoOperation::Rebase));
    // While rebasing, "ours" is the branch being rebased onto.
    let content = resolve(&repo, &[ConflictChoice::Ours, ConflictChoice::Theirs]).await;
    assert_eq!(content, "top\nours\nmiddle\nend\ntheirs tail\n");
    op(&repo, RepoOp::Continue).await;
    assert!(repo.operation().is_none());
    assert_eq!(subjects(&sb, &repo).await, ["Topic", "Main", "Base"]);

    // Cherry-pick the rebased topic commit (only its tail differs now).
    op(&repo, RepoOp::Checkout("main".into())).await;
    commit_file(&repo, "f.txt", BASE, "Reset").await;
    commit_file(
        &repo,
        "f.txt",
        "top\nagain\nmiddle\nend\nagain tail\n",
        "Again",
    )
    .await;
    let pick = common::git(&sb, &repo, &["rev-parse", "topic"]).await;
    assert!(
        try_op(
            &repo,
            RepoOp::CherryPick(ferrumgit_domain::Oid::new(pick.trim()))
        )
        .await
        .is_err()
    );
    assert_eq!(repo.operation(), Some(RepoOperation::CherryPick));
    let file = repo.conflict_file("f.txt").await.expect("conflict file");
    assert!(matches!(
        &file.segments[1],
        ConflictSegment::Conflict { base: Some(b), .. } if b == "ours tail\n"
    ));
    let content = resolve(&repo, &[ConflictChoice::BothTheirsFirst]).await;
    assert_eq!(
        content,
        "top\nagain\nmiddle\nend\ntheirs tail\nagain tail\n"
    );
    op(&repo, RepoOp::Continue).await;
    assert!(repo.operation().is_none());
}

#[tokio::test]
async fn deleted_binary_and_clean_files_are_refused() {
    let sb = Sandbox::new();
    let repo = sb.init("refused").await;
    commit_file(&repo, "gone.txt", "a\n", "Base").await;
    op(
        &repo,
        RepoOp::CreateBranch {
            name: "topic".into(),
            start: None,
            checkout: true,
        },
    )
    .await;
    commit_file(&repo, "gone.txt", "changed\n", "Change").await;
    op(&repo, RepoOp::Checkout("main".into())).await;
    std::fs::remove_file(repo.root.join("gone.txt")).unwrap();
    op(&repo, RepoOp::StageAll).await;
    op(
        &repo,
        RepoOp::Commit {
            message: "Delete".into(),
            amend: false,
        },
    )
    .await;
    assert!(
        try_op(
            &repo,
            RepoOp::Merge {
                branch: "topic".into(),
                no_ff: false
            }
        )
        .await
        .is_err()
    );
    let err = repo.conflict_file("gone.txt").await.expect_err("deleted");
    assert!(err.to_string().contains("deleted on one side"), "{err}");
    let err = repo.conflict_file("nothing.txt").await.expect_err("clean");
    assert!(err.to_string().contains("not in conflict"), "{err}");
    assert!(repo.conflict_file("../x").await.is_err());
}
