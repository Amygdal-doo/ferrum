//! Opening a repository and reading its state.

use crate::error::{GitError, GitResult};
use crate::parse::{self, COMMIT_FORMAT, DETAILS_FORMAT, REFS_FORMAT, STASH_FORMAT};
use crate::runner::{Cmd, Git};
use ferrumgit_domain::{
    Commit, CommitDetails, FileDiff, GitRef, Oid, Remote, RepoOperation, Stash, Undo, WorkingStatus,
};
use std::path::{Path, PathBuf};

/// Patches larger than this are cut at a file boundary for display.
pub const MAX_PATCH_BYTES: usize = 4 * 1024 * 1024;

/// An opened working tree. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Repo {
    pub(crate) git: Git,
    /// Top level of the working tree.
    pub root: PathBuf,
    /// Absolute `.git` directory (differs for worktrees/submodules).
    pub git_dir: PathBuf,
}

/// Everything the main view shows, read in one go.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub status: WorkingStatus,
    pub commits: Vec<Commit>,
    pub refs: Vec<GitRef>,
    pub stashes: Vec<Stash>,
    pub remotes: Vec<Remote>,
    /// More commits exist beyond the loaded limit.
    pub has_more: bool,
    /// The last local operation, if it can be undone safely.
    pub undo: Option<Undo>,
}

impl Repo {
    /// Resolves `path` (any directory inside a working tree).
    pub async fn open(git: &Git, path: &Path) -> GitResult<Self> {
        let out = git
            .run(Cmd::new(["rev-parse", "--show-toplevel", "--absolute-git-dir"]).cwd(path))
            .await
            .map_err(|e| match e {
                GitError::Failed { .. } => GitError::NotARepo(path.display().to_string()),
                other => other,
            })?;
        let text = out.stdout_str();
        let mut lines = text.lines();
        let (Some(root), Some(git_dir)) = (lines.next(), lines.next()) else {
            return Err(GitError::NotARepo(path.display().to_string()));
        };
        Ok(Self {
            git: git.clone(),
            root: PathBuf::from(root),
            git_dir: PathBuf::from(git_dir),
        })
    }

    pub(crate) fn cmd<I, S>(&self, args: I) -> Cmd
    where
        I: IntoIterator<Item = S>,
        S: Into<std::ffi::OsString>,
    {
        Cmd::new(args).cwd(&self.root)
    }

    pub(crate) async fn run(&self, cmd: Cmd) -> GitResult<crate::Output> {
        self.git.run(cmd).await
    }

    pub async fn status(&self) -> GitResult<WorkingStatus> {
        let out = self
            .run(self.cmd([
                "status",
                "--porcelain=v2",
                "--branch",
                "-z",
                "--untracked-files=all",
            ]))
            .await?;
        let mut s = parse::parse_status(&out.stdout);
        s.operation = self.operation();
        s.rebase = self.rebase_progress();
        Ok(s)
    }

    /// Merge / rebase / cherry-pick / revert in progress, from marker files.
    #[must_use]
    pub fn operation(&self) -> Option<RepoOperation> {
        let d = &self.git_dir;
        if d.join("rebase-merge").is_dir() || d.join("rebase-apply").is_dir() {
            Some(RepoOperation::Rebase)
        } else if d.join("MERGE_HEAD").is_file() {
            Some(RepoOperation::Merge)
        } else if d.join("CHERRY_PICK_HEAD").is_file() {
            Some(RepoOperation::CherryPick)
        } else if d.join("REVERT_HEAD").is_file() {
            Some(RepoOperation::Revert)
        } else {
            None
        }
    }

    /// Up to `limit` commits reachable from any branch, tag or HEAD (stashes excluded),
    /// newest first with children before parents.
    pub async fn log(&self, limit: usize, has_commits: bool) -> GitResult<(Vec<Commit>, bool)> {
        if !has_commits {
            return Ok((Vec::new(), false));
        }
        let out = self
            .run(self.cmd([
                "log".to_owned(),
                "--exclude=refs/stash".to_owned(),
                // Fetched pull requests (refs/ferrumgit/pr/…) stay out of the graph.
                format!("--exclude={}*", crate::PR_NAMESPACE),
                "--all".to_owned(),
                "--date-order".to_owned(),
                "-z".to_owned(),
                format!("--max-count={}", limit + 1),
                format!("--format={COMMIT_FORMAT}"),
            ]))
            .await?;
        let mut commits = parse::parse_log(&out.stdout);
        let more = commits.len() > limit;
        commits.truncate(limit);
        Ok((commits, more))
    }

    pub async fn refs(&self) -> GitResult<Vec<GitRef>> {
        let out = self
            .run(self.cmd([
                "for-each-ref".to_owned(),
                format!("--format={REFS_FORMAT}"),
                "refs/heads".to_owned(),
                "refs/remotes".to_owned(),
                "refs/tags".to_owned(),
            ]))
            .await?;
        let mut refs = parse::parse_refs(&out.stdout);
        // Mark local branches already merged into HEAD (not HEAD itself).
        if let Ok(merged) = self
            .run(self.cmd([
                "for-each-ref",
                "--merged=HEAD",
                "--format=%(refname)",
                "refs/heads",
            ]))
            .await
        {
            let names = merged.stdout_str();
            let set: std::collections::HashSet<&str> = names.lines().collect();
            for r in refs.iter_mut().filter(|r| !r.is_head) {
                r.merged = set.contains(r.full_name.as_str());
            }
        }
        Ok(refs)
    }

    pub async fn stashes(&self) -> GitResult<Vec<Stash>> {
        let out = self
            .run(self.cmd([
                "stash".to_owned(),
                "list".to_owned(),
                "-z".to_owned(),
                format!("--format={STASH_FORMAT}"),
            ]))
            .await?;
        Ok(parse::parse_stashes(&out.stdout))
    }

    pub async fn remotes(&self) -> GitResult<Vec<Remote>> {
        let out = self.run(self.cmd(["remote", "-v"])).await?;
        Ok(parse::parse_remotes(&out.stdout))
    }

    /// Status, log, refs, stashes and remotes, read concurrently.
    pub async fn snapshot(&self, limit: usize) -> GitResult<Snapshot> {
        let (status, refs, stashes, remotes, undo) = tokio::join!(
            self.status(),
            self.refs(),
            self.stashes(),
            self.remotes(),
            self.undo_plan()
        );
        let status = status?;
        let refs = refs?;
        let has_commits = status.branch.oid.is_some() || !refs.is_empty();
        let (commits, has_more) = self.log(limit, has_commits).await?;
        Ok(Snapshot {
            status,
            commits,
            refs,
            stashes: stashes?,
            remotes: remotes?,
            has_more,
            undo: undo.unwrap_or_default(),
        })
    }

    /// Cheap fingerprint of status + refs + stashes, for change polling.
    pub async fn fingerprint(&self) -> GitResult<String> {
        let (status, refs, stash) = tokio::join!(
            self.run(self.cmd([
                "status",
                "--porcelain=v2",
                "--branch",
                "-z",
                "--untracked-files=all"
            ])),
            self.run(self.cmd(["for-each-ref", "--format=%(refname) %(objectname)"])),
            self.run(self.cmd(["stash", "list", "--format=%H"])),
        );
        let mut s = status?.stdout_str();
        s.push_str(&refs?.stdout_str());
        s.push_str(&stash?.stdout_str());
        if let Some(op) = self.operation() {
            s.push_str(op.label());
        }
        Ok(s)
    }

    /// Working tree vs index for one file.
    pub async fn diff_unstaged(&self, path: &str) -> GitResult<Option<FileDiff>> {
        let out = self
            .run(self.cmd(["diff", "--no-color", "--no-ext-diff", "-U3", "--", path]))
            .await?;
        Ok(parse::parse_diff(&out.stdout_str()).into_iter().next())
    }

    /// Index vs HEAD for one file (follows a staged rename).
    pub async fn diff_staged(&self, path: &str, orig: Option<&str>) -> GitResult<Option<FileDiff>> {
        let mut cmd = self.cmd([
            "diff",
            "--cached",
            "--no-color",
            "--no-ext-diff",
            "-M",
            "-U3",
            "--",
            path,
        ]);
        if let Some(o) = orig {
            cmd = cmd.arg(o);
        }
        let out = self.run(cmd).await?;
        Ok(parse::parse_diff(&out.stdout_str()).into_iter().next())
    }

    /// An untracked file as an all-added diff.
    pub async fn diff_untracked(&self, path: &str) -> GitResult<Option<FileDiff>> {
        let out = self
            .run(
                self.cmd([
                    "diff",
                    "--no-index",
                    "--no-color",
                    "--no-ext-diff",
                    "--",
                    "/dev/null",
                    path,
                ])
                .ok_codes(&[1]),
            )
            .await?;
        Ok(parse::parse_diff(&out.stdout_str())
            .into_iter()
            .next()
            .map(|mut f| {
                f.path = path.to_owned();
                f
            }))
    }

    /// Message, people, and changes against the first parent.
    pub async fn commit_details(&self, rev: &str) -> GitResult<CommitDetails> {
        check_rev(rev)?;
        let head = self
            .run(self.cmd([
                "show".to_owned(),
                "-s".to_owned(),
                format!("--format={DETAILS_FORMAT}"),
                rev.to_owned(),
            ]))
            .await?;
        let mut details = parse::parse_details_header(&head.stdout)
            .ok_or_else(|| GitError::Parse(format!("commit header for {rev}")))?;
        let patch = self
            .run(self.cmd([
                "show",
                "--format=",
                "--diff-merges=first-parent",
                "--no-color",
                "--no-ext-diff",
                "-M",
                "-p",
                rev,
            ]))
            .await?;
        let (files, truncated) = parse_limited(&patch.stdout);
        details.files = files;
        details.truncated = truncated;
        Ok(details)
    }

    /// A stash's changes (tracked and untracked) against its base commit.
    pub async fn stash_details(&self, stash: &Stash) -> GitResult<CommitDetails> {
        let rev = stash.refname();
        let mut details = self.commit_details(stash.oid.as_str()).await?;
        let patch = self
            .run(self.cmd([
                "stash",
                "show",
                "-p",
                "--include-untracked",
                "--no-color",
                "--no-ext-diff",
                &rev,
            ]))
            .await?;
        let (files, truncated) = parse_limited(&patch.stdout);
        details.files = files;
        details.truncated = truncated;
        Ok(details)
    }

    /// Last commit message, for "Amend".
    pub async fn head_message(&self) -> GitResult<String> {
        let out = self.run(self.cmd(["log", "-1", "--format=%B"])).await?;
        Ok(out.stdout_str().trim_end().to_owned())
    }

    pub async fn head_exists(&self) -> bool {
        self.run(self.cmd(["rev-parse", "--verify", "-q", "HEAD"]))
            .await
            .is_ok()
    }

    /// `true` if `oid` has more than one parent.
    pub async fn is_merge_commit(&self, oid: &Oid) -> GitResult<bool> {
        check_rev(oid.as_str())?;
        let out = self
            .run(self.cmd(["rev-list", "--parents", "-n", "1", oid.as_str()]))
            .await?;
        Ok(out.stdout_str().split_whitespace().count() > 2)
    }
}

/// Rejects revisions that git would read as options.
pub(crate) fn check_rev(rev: &str) -> GitResult<()> {
    if rev.is_empty() || rev.starts_with('-') || rev.contains(char::is_whitespace) {
        return Err(GitError::Invalid(format!("invalid revision: {rev:?}")));
    }
    Ok(())
}

pub(crate) fn parse_limited(raw: &[u8]) -> (Vec<FileDiff>, bool) {
    if raw.len() <= MAX_PATCH_BYTES {
        return (parse::parse_diff(&String::from_utf8_lossy(raw)), false);
    }
    let text = String::from_utf8_lossy(&raw[..MAX_PATCH_BYTES]);
    let cut = text.rfind("\ndiff --git ").unwrap_or(text.len());
    (parse::parse_diff(&text[..cut]), true)
}
