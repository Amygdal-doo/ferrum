//! Temp repositories with an isolated git config, shared by the rebase,
//! conflict and bisect tests.

#![allow(dead_code, clippy::unwrap_used)]

use ferrumgit_domain::{Oid, RepoOp};
use ferrumgit_git::{Cmd, Git, GitResult, OpContext, Repo};

pub struct Sandbox {
    pub dir: tempfile::TempDir,
    pub git: Git,
}

impl Sandbox {
    pub fn new() -> Self {
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

    /// A repository in a folder with a space and a quote in its name, so
    /// shell quoting is exercised.
    pub async fn init(&self, name: &str) -> Repo {
        self.git
            .init_repo(&self.dir.path().join(format!("it's {name}")), "main")
            .await
            .expect("init")
    }
}

pub fn write(repo: &Repo, file: &str, content: &str) {
    std::fs::write(repo.root.join(file), content).expect("write");
}

pub fn read(repo: &Repo, file: &str) -> String {
    std::fs::read_to_string(repo.root.join(file)).expect("read")
}

pub async fn try_op(repo: &Repo, op: RepoOp) -> GitResult<Option<String>> {
    repo.execute(&op, OpContext::default()).await
}

pub async fn op(repo: &Repo, op: RepoOp) -> Option<String> {
    try_op(repo, op.clone())
        .await
        .unwrap_or_else(|e| panic!("{op:?}: {e}"))
}

pub async fn commit_file(repo: &Repo, file: &str, content: &str, msg: &str) -> Oid {
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
    head(repo).await
}

pub async fn head(repo: &Repo) -> Oid {
    repo.status()
        .await
        .expect("status")
        .branch
        .oid
        .expect("head")
}

/// Raw git output in `repo` (for assertions).
pub async fn git(sb: &Sandbox, repo: &Repo, args: &[&str]) -> String {
    sb.git
        .run(Cmd::new(args.iter().copied()).cwd(&repo.root))
        .await
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"))
        .stdout_str()
}

/// Subjects from HEAD back to the root, newest first.
pub async fn subjects(sb: &Sandbox, repo: &Repo) -> Vec<String> {
    git(sb, repo, &["log", "--format=%s"])
        .await
        .lines()
        .map(str::to_owned)
        .collect()
}
