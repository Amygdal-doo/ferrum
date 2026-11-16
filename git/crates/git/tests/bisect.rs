//! `git bisect` start / good / bad / skip / reset and the state the banner
//! shows, against the real `git` binary.

#![allow(clippy::unwrap_used)]

mod common;

use common::{Sandbox, commit_file, head, op, read, try_op};
use ferrumgit_domain::{BisectMark, Oid, RepoOp};
use ferrumgit_git::Repo;

/// Ten commits; the bug appears in the seventh. Returns them oldest first.
async fn history(sb: &Sandbox) -> (Repo, Vec<Oid>) {
    let repo = sb.init("bisect").await;
    let mut oids = Vec::new();
    for i in 1..=10 {
        let state = if i >= 7 { "bug" } else { "ok" };
        oids.push(
            commit_file(
                &repo,
                "state.txt",
                &format!("{state} {i}\n"),
                &format!("Commit {i}"),
            )
            .await,
        );
    }
    (repo, oids)
}

#[tokio::test]
async fn finds_the_first_bad_commit_and_resets() {
    let sb = Sandbox::new();
    let (repo, oids) = history(&sb).await;
    let tip = head(&repo).await;
    assert!(repo.status().await.unwrap().bisect.is_none());

    op(
        &repo,
        RepoOp::BisectStart {
            bad: None,
            good: None,
        },
    )
    .await;
    let state = repo.status().await.unwrap().bisect.expect("bisecting");
    assert_eq!(state.start, "main");
    assert!(state.summary().contains("mark a bad and a good"));

    // The current commit is bad; the first is good.
    op(
        &repo,
        RepoOp::BisectMark {
            mark: BisectMark::Bad,
            rev: None,
        },
    )
    .await;
    let msg = op(
        &repo,
        RepoOp::BisectMark {
            mark: BisectMark::Good,
            rev: Some(oids[0].clone()),
        },
    )
    .await
    .unwrap_or_default();
    assert!(msg.starts_with("Bisecting:"), "{msg}");
    let state = repo.status().await.unwrap().bisect.expect("bisecting");
    assert_eq!((state.good, state.remaining), (1, Some(9)));

    // Test each checked-out commit until git names the culprit; one is skipped.
    let mut skipped = false;
    for _ in 0..10 {
        let state = repo.status().await.unwrap().bisect.expect("bisecting");
        if state.found.is_some() {
            break;
        }
        let mark = if !skipped && read(&repo, "state.txt") == "ok 4\n" {
            skipped = true;
            BisectMark::Skip
        } else if read(&repo, "state.txt").starts_with("bug") {
            BisectMark::Bad
        } else {
            BisectMark::Good
        };
        let msg = op(&repo, RepoOp::BisectMark { mark, rev: None }).await;
        if msg
            .as_deref()
            .is_some_and(|m| m.starts_with("First bad commit"))
        {
            assert_eq!(
                msg.unwrap(),
                format!("First bad commit: {}", oids[6].short())
            );
        }
    }
    let state = repo
        .status()
        .await
        .unwrap()
        .bisect
        .expect("still bisecting");
    assert_eq!(state.found.as_ref(), Some(&oids[6]));
    assert!(state.summary().starts_with("Bisect done"));

    op(&repo, RepoOp::BisectReset).await;
    let status = repo.status().await.unwrap();
    assert!(status.bisect.is_none());
    assert_eq!(status.branch.head.as_deref(), Some("main"));
    assert_eq!(head(&repo).await, tip);
}

#[tokio::test]
async fn start_marks_both_ends_and_refuses_misuse() {
    let sb = Sandbox::new();
    let (repo, oids) = history(&sb).await;
    assert!(
        try_op(
            &repo,
            RepoOp::BisectMark {
                mark: BisectMark::Good,
                rev: None
            }
        )
        .await
        .is_err()
    );
    assert!(try_op(&repo, RepoOp::BisectReset).await.is_err());

    let msg = op(
        &repo,
        RepoOp::BisectStart {
            bad: Some(oids[9].clone()),
            good: Some(oids[0].clone()),
        },
    )
    .await
    .unwrap_or_default();
    assert!(msg.starts_with("Bisecting:"), "{msg}");
    let err = try_op(
        &repo,
        RepoOp::BisectStart {
            bad: None,
            good: None,
        },
    )
    .await
    .expect_err("already bisecting");
    assert!(err.to_string().contains("already"), "{err}");
    op(&repo, RepoOp::BisectReset).await;

    // A bad start leaves no session behind.
    assert!(
        try_op(
            &repo,
            RepoOp::BisectStart {
                bad: Some(Oid::new("0123456789012345678901234567890123456789")),
                good: None,
            },
        )
        .await
        .is_err()
    );
    assert!(!repo.bisecting());
}
