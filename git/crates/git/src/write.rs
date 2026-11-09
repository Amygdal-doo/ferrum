//! Executing [`RepoOp`]s. Each maps to one or a few git commands; network
//! operations receive the profile's [`NetEnv`] and a cancellation token.

use crate::error::{GitError, GitResult};
use crate::read::{Repo, check_rev};
use crate::runner::{Cmd, Git};
use crate::ssh::NetEnv;
use ferrumgit_domain::{PullMode, RepoOp, RepoOperation};
use std::path::Path;
use tokio_util::sync::CancellationToken;

/// Context for one operation.
#[derive(Debug, Clone, Default)]
pub struct OpContext<'a> {
    /// `(name, email)` overriding git config for commits.
    pub identity: Option<(&'a str, &'a str)>,
    pub net: Option<NetEnv>,
    pub cancel: Option<CancellationToken>,
}

fn check_name(kind: &str, name: &str) -> GitResult<()> {
    let n = name.trim();
    if n.is_empty() || n.starts_with('-') || n.contains(char::is_whitespace) {
        return Err(GitError::Invalid(format!("invalid {kind} name: {name:?}")));
    }
    Ok(())
}

fn paths_cmd(repo: &Repo, args: &[&str], paths: &[String]) -> GitResult<Cmd> {
    if paths.is_empty() {
        return Err(GitError::Invalid("no files selected".into()));
    }
    Ok(repo.cmd(args.iter().copied()).arg("--").args(paths))
}

impl Repo {
    /// Runs `op`; returns git's summary line when it is informative.
    pub async fn execute(&self, op: &RepoOp, ctx: OpContext<'_>) -> GitResult<Option<String>> {
        let net = |c: Cmd| c.net(ctx.net.clone()).cancel(ctx.cancel.clone());
        let out = match op {
            RepoOp::Stage(paths) => self.run(paths_cmd(self, &["add", "-A"], paths)?).await?,
            RepoOp::StageAll => self.run(self.cmd(["add", "-A"])).await?,
            RepoOp::Unstage(paths) => {
                if self.head_exists().await {
                    self.run(paths_cmd(self, &["restore", "--staged"], paths)?)
                        .await?
                } else {
                    self.run(paths_cmd(self, &["rm", "--cached", "-r", "-q"], paths)?)
                        .await?
                }
            }
            RepoOp::UnstageAll => {
                if self.head_exists().await {
                    self.run(self.cmd(["reset", "-q"])).await?
                } else {
                    self.run(self.cmd(["rm", "--cached", "-r", "-q", "."]))
                        .await?
                }
            }
            RepoOp::Discard(paths) => return self.discard(paths).await.map(|()| None),
            RepoOp::ApplyPatch {
                patch,
                cached,
                reverse,
            } => {
                let mut c = self.cmd(["apply", "--recount", "--whitespace=nowarn"]);
                if *cached {
                    c = c.arg("--cached");
                }
                if *reverse {
                    c = c.arg("--reverse");
                }
                self.run(c.arg("-").stdin(patch.as_bytes())).await?
            }
            RepoOp::Commit { message, amend } => {
                if message.trim().is_empty() {
                    return Err(GitError::Invalid("enter a commit message".into()));
                }
                let mut c = self.cmd(["commit", "-q", "--cleanup=strip", "-F", "-"]);
                if *amend {
                    c = c.arg("--amend");
                }
                c = c.identity(ctx.identity);
                self.run(c.stdin(message.as_bytes())).await?
            }
            RepoOp::Checkout(branch) => {
                check_name("branch", branch)?;
                self.run(self.cmd(["switch", "--no-guess", branch])).await?
            }
            RepoOp::CheckoutRemote {
                remote_branch,
                local,
            } => {
                check_name("branch", local)?;
                check_name("branch", remote_branch)?;
                self.run(self.cmd(["switch", "-c", local, "--track", remote_branch]))
                    .await?
            }
            RepoOp::CheckoutDetached(oid) => {
                check_rev(oid.as_str())?;
                self.run(self.cmd(["switch", "--detach", oid.as_str()]))
                    .await?
            }
            RepoOp::CreateBranch {
                name,
                start,
                checkout,
            } => {
                check_name("branch", name)?;
                let mut c = if *checkout {
                    self.cmd(["switch", "-c", name])
                } else {
                    self.cmd(["branch", name])
                };
                if let Some(s) = start {
                    check_rev(s)?;
                    c = c.arg(s);
                }
                self.run(c).await?
            }
            RepoOp::RenameBranch { old, new } => {
                check_name("branch", old)?;
                check_name("branch", new)?;
                self.run(self.cmd(["branch", "-m", old, new])).await?
            }
            RepoOp::DeleteBranch { name, force } => {
                check_name("branch", name)?;
                self.run(self.cmd(["branch", if *force { "-D" } else { "-d" }, name]))
                    .await?
            }
            RepoOp::DeleteRemoteBranch { remote, branch } => {
                check_name("remote", remote)?;
                check_name("branch", branch)?;
                self.run(net(self.cmd(["push", remote, "--delete", branch])))
                    .await?
            }
            RepoOp::SetUpstream { branch, upstream } => {
                check_name("branch", branch)?;
                check_name("branch", upstream)?;
                self.run(self.cmd([
                    "branch".to_owned(),
                    format!("--set-upstream-to={upstream}"),
                    branch.clone(),
                ]))
                .await?
            }
            RepoOp::Merge { branch, no_ff } => {
                check_rev(branch)?;
                let mut c = self.cmd(["merge", "--no-edit"]);
                if *no_ff {
                    c = c.arg("--no-ff");
                }
                self.run(c.arg(branch)).await?
            }
            RepoOp::Rebase { onto } => {
                check_rev(onto)?;
                self.run(self.cmd(["rebase", onto])).await?
            }
            RepoOp::CherryPick(oid) => {
                let mut c = self.cmd(["cherry-pick"]);
                if self.is_merge_commit(oid).await? {
                    c = c.args(["-m", "1"]);
                }
                self.run(c.arg(oid.as_str())).await?
            }
            RepoOp::Revert(oid) => {
                let mut c = self.cmd(["revert", "--no-edit"]);
                if self.is_merge_commit(oid).await? {
                    c = c.args(["-m", "1"]);
                }
                self.run(c.arg(oid.as_str())).await?
            }
            RepoOp::Reset { target, mode } => {
                check_rev(target)?;
                self.run(self.cmd(["reset", "-q", mode.flag(), target]))
                    .await?
            }
            RepoOp::Continue => {
                let Some(op) = self.operation() else {
                    return Err(GitError::Invalid(
                        "no merge, rebase, cherry-pick or revert in progress".into(),
                    ));
                };
                let args: &[&str] = match op {
                    RepoOperation::Merge => &["commit", "--no-edit"],
                    RepoOperation::Rebase => &["rebase", "--continue"],
                    RepoOperation::CherryPick => &["cherry-pick", "--continue"],
                    RepoOperation::Revert => &["revert", "--continue"],
                };
                let mut c = self.cmd(args.iter().copied());
                c = c.identity(ctx.identity);
                self.run(c).await?
            }
            RepoOp::Abort => {
                let Some(op) = self.operation() else {
                    return Err(GitError::Invalid("nothing to abort".into()));
                };
                let sub = match op {
                    RepoOperation::Merge => "merge",
                    RepoOperation::Rebase => "rebase",
                    RepoOperation::CherryPick => "cherry-pick",
                    RepoOperation::Revert => "revert",
                };
                self.run(self.cmd([sub, "--abort"])).await?
            }
            RepoOp::TakeOurs(paths) | RepoOp::TakeTheirs(paths) => {
                let side = if matches!(op, RepoOp::TakeOurs(_)) {
                    "--ours"
                } else {
                    "--theirs"
                };
                self.run(paths_cmd(self, &["checkout", side], paths)?)
                    .await?;
                self.run(paths_cmd(self, &["add"], paths)?).await?
            }
            RepoOp::CreateTag {
                name,
                target,
                message,
            } => {
                check_name("tag", name)?;
                check_rev(target)?;
                let c = match message.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
                    Some(m) => self
                        .cmd(["tag", "-a", name, "-F", "-", target])
                        .stdin(m.as_bytes()),
                    None => self.cmd(["tag", name, target]),
                };
                let c = c.identity(ctx.identity);
                self.run(c).await?
            }
            RepoOp::DeleteTag(tag) => {
                check_name("tag", tag)?;
                self.run(self.cmd(["tag", "-d", tag])).await?
            }
            RepoOp::PushTag { remote, tag } => {
                check_name("remote", remote)?;
                check_name("tag", tag)?;
                self.run(net(self.cmd([
                    "push".to_owned(),
                    remote.clone(),
                    format!("refs/tags/{tag}"),
                ])))
                .await?
            }
            RepoOp::DeleteRemoteTag { remote, tag } => {
                check_name("remote", remote)?;
                check_name("tag", tag)?;
                self.run(net(self.cmd([
                    "push".to_owned(),
                    remote.clone(),
                    "--delete".to_owned(),
                    format!("refs/tags/{tag}"),
                ])))
                .await?
            }
            RepoOp::StashSave {
                message,
                include_untracked,
            } => {
                let mut c = self.cmd(["stash", "push"]);
                if *include_untracked {
                    c = c.arg("--include-untracked");
                }
                if !message.trim().is_empty() {
                    c = c.args(["-m", message.trim()]);
                }
                self.run(c).await?
            }
            RepoOp::StashApply(i) => {
                self.run(self.cmd([
                    "stash".to_owned(),
                    "apply".to_owned(),
                    format!("stash@{{{i}}}"),
                ]))
                .await?
            }
            RepoOp::StashPop(i) => {
                self.run(self.cmd([
                    "stash".to_owned(),
                    "pop".to_owned(),
                    format!("stash@{{{i}}}"),
                ]))
                .await?
            }
            RepoOp::StashDrop(i) => {
                self.run(self.cmd([
                    "stash".to_owned(),
                    "drop".to_owned(),
                    format!("stash@{{{i}}}"),
                ]))
                .await?
            }
            RepoOp::AddRemote { name, url } => {
                check_name("remote", name)?;
                if url.trim().is_empty() || url.trim().starts_with('-') {
                    return Err(GitError::Invalid("enter a remote URL".into()));
                }
                self.run(self.cmd(["remote", "add", name, url.trim()]))
                    .await?
            }
            RepoOp::RemoveRemote(name) => {
                check_name("remote", name)?;
                self.run(self.cmd(["remote", "remove", name])).await?
            }
            RepoOp::Fetch { remote, prune } => {
                let mut c = self.cmd(["fetch", "--tags"]);
                if *prune {
                    c = c.arg("--prune");
                }
                c = match remote {
                    Some(r) => {
                        check_name("remote", r)?;
                        c.arg(r)
                    }
                    None => c.arg("--all"),
                };
                self.run(net(c)).await?
            }
            RepoOp::Pull(mode) => {
                let flag = match mode {
                    PullMode::FastForwardOnly => "--ff-only",
                    PullMode::Merge => "--no-rebase",
                    PullMode::Rebase => "--rebase",
                };
                let mut c = self.cmd(["pull", "--no-edit", flag]);
                c = c.identity(ctx.identity);
                self.run(net(c)).await?
            }
            RepoOp::Push {
                remote,
                force_with_lease,
            } => {
                let status = self.status().await?;
                let Some(branch) = status.branch.head else {
                    return Err(GitError::Invalid(
                        "HEAD is detached; check out a branch to push".into(),
                    ));
                };
                let mut c = self.cmd(["push"]);
                if *force_with_lease {
                    c = c.arg("--force-with-lease");
                }
                c = match (status.branch.upstream, remote) {
                    (Some(_), None) => c,
                    (_, Some(r)) => {
                        check_name("remote", r)?;
                        c.args(["--set-upstream", r.as_str(), branch.as_str()])
                    }
                    (None, None) => {
                        let remotes = self.remotes().await?;
                        let Some(r) = remotes
                            .iter()
                            .find(|r| r.name == "origin")
                            .or(remotes.first())
                        else {
                            return Err(GitError::Invalid(
                                "this repository has no remotes; add one first".into(),
                            ));
                        };
                        c.args(["--set-upstream", r.name.as_str(), branch.as_str()])
                    }
                };
                self.run(net(c)).await?
            }
            RepoOp::Undo(undo) => self.undo(undo).await?,
        };
        Ok(out.summary())
    }

    /// Discards working-tree changes; deletes untracked files among `paths`.
    async fn discard(&self, paths: &[String]) -> GitResult<()> {
        if paths.is_empty() {
            return Err(GitError::Invalid("no files selected".into()));
        }
        let untracked = self
            .run(paths_cmd(
                self,
                &["ls-files", "--others", "--exclude-standard", "-z"],
                paths,
            )?)
            .await?;
        let untracked: Vec<String> = untracked
            .stdout_str()
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect();
        let tracked: Vec<String> = paths
            .iter()
            .filter(|p| !untracked.contains(p))
            .cloned()
            .collect();
        if !untracked.is_empty() {
            self.run(paths_cmd(self, &["clean", "-f", "-q"], &untracked)?)
                .await?;
        }
        if !tracked.is_empty() {
            self.run(paths_cmd(self, &["restore", "--worktree"], &tracked)?)
                .await?;
        }
        Ok(())
    }
}

impl Git {
    /// `git clone <url> <dest>`; returns the opened repository.
    pub async fn clone_repo(
        &self,
        url: &str,
        dest: &Path,
        net: Option<NetEnv>,
        cancel: Option<CancellationToken>,
    ) -> GitResult<Repo> {
        let url = url.trim();
        if url.is_empty() || url.starts_with('-') {
            return Err(GitError::Invalid("enter a repository URL".into()));
        }
        if dest.exists()
            && dest
                .read_dir()
                .map(|mut d| d.next().is_some())
                .unwrap_or(true)
        {
            return Err(GitError::Invalid(format!(
                "{} already exists and is not empty",
                dest.display()
            )));
        }
        let cmd = Cmd::new(["clone", "--", url])
            .arg(dest.as_os_str())
            .net(net)
            .cancel(cancel);
        self.run(cmd).await.map_err(GitError::with_ssh_hint)?;
        Repo::open(self, dest).await
    }

    /// `git init` (creating the directory if needed).
    pub async fn init_repo(&self, dir: &Path, initial_branch: &str) -> GitResult<Repo> {
        check_name("branch", initial_branch)?;
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| GitError::Spawn(e.to_string()))?;
        self.run(Cmd::new(["init", "-q", "-b", initial_branch]).arg(dir.as_os_str()))
            .await?;
        Repo::open(self, dir).await
    }
}
